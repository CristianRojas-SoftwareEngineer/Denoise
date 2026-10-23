//! Módulo `df` — Orquestación principal del DSP de DPDFNet.
//!
//! Submódulos: `stft`, `net`.
//! Contrato: `docs/design.md §6`, `docs/specifications.md §2 RF-05 y §3 RNF-04`.
//!
//! DPDFNet 48 kHz opera como grafo único stateful: la normalización de features
//! (ERB y spec) ocurre dentro del ONNX, por lo que este módulo no calcula bandas
//! ERB. El pipeline es STFT -> frames al modelo encadenando estado -> iSTFT
//! -> gate de pausa (`apply_pause_gate`: −25 dB en no-voz, voz ×1.0) antes de
//! cuantizar a PCM16.

pub mod filter;
pub mod net;
pub mod stft;

use crate::df::filter::{apply_highpass, HIGHPASS_CUTOFF_HZ};
use crate::df::net::{DpdfNetSession, ORT_CACHE};
use crate::df::stft::{StftHelper, SR};
use crate::errors::E;
use crate::models::{default_model_dir, DefaultModelsProvider, ModelsProvider};
use hound::{WavReader, WavSpec, WavWriter};
use std::f32::consts::PI;
use std::path::Path;

/// Procesa un archivo WAV mono 48kHz PCM16 eliminando ruido con DPDFNet.
pub fn denoise_wav(in_wav: &Path, out_wav: &Path, progress_cb: impl Fn(f64, f64)) -> Result<(), E> {
    let model_dir = default_model_dir();
    let provider = DefaultModelsProvider::new();
    denoise_wav_with_provider(in_wav, out_wav, &model_dir, &provider, &progress_cb)
}

/// Constante canónica de atenuación de pausas en dB (-25 dB).
pub const GATE_CRUSH_DB: f64 = -25.0;

/// Procesa un archivo WAV con un proveedor de modelos específico (permite tests/custom model-dir).
pub fn denoise_wav_with_provider(
    in_wav: &Path,
    out_wav: &Path,
    model_dir: &Path,
    provider: &dyn ModelsProvider,
    progress_cb: &dyn Fn(f64, f64),
) -> Result<(), E> {
    denoise_wav_with_params(in_wav, out_wav, model_dir, provider, 0.0, GATE_CRUSH_DB, false, progress_cb)
}

/// Procesa un archivo WAV con un proveedor de modelos específico y mezcla opcional dry-blend.
pub fn denoise_wav_with_provider_and_blend(
    in_wav: &Path,
    out_wav: &Path,
    model_dir: &Path,
    provider: &dyn ModelsProvider,
    dry_blend: f32,
    progress_cb: &dyn Fn(f64, f64),
) -> Result<(), E> {
    denoise_wav_with_params(in_wav, out_wav, model_dir, provider, dry_blend, GATE_CRUSH_DB, false, progress_cb)
}

/// Procesa un archivo WAV con proveedor, dry-blend, atenuación de pause gate
/// y filtro pasa-altos opcional configurables.
#[allow(clippy::too_many_arguments)]
pub fn denoise_wav_with_params(
    in_wav: &Path,
    out_wav: &Path,
    model_dir: &Path,
    provider: &dyn ModelsProvider,
    dry_blend: f32,
    pause_gate_db: f64,
    highpass: bool,
    progress_cb: &dyn Fn(f64, f64),
) -> Result<(), E> {
    // 1. Cargar y verificar modelos
    let model_paths = provider.ensure_models(model_dir, progress_cb)?;

    // 2. Leer audio de entrada con hound
    let mut reader = WavReader::open(in_wav)
        .map_err(|e| E::EInvalidInput(format!("Error abriendo WAV: {}", e)))?;
    let spec = reader.spec();

    let samples_raw: Vec<i16> = reader
        .samples::<i16>()
        .collect::<Result<Vec<i16>, _>>()
        .map_err(|e| E::EInvalidInput(format!("Error leyendo muestras WAV: {}", e)))?;

    if samples_raw.is_empty() {
        return Err(E::EInvalidInput("WAV de entrada está vacío".to_string()));
    }

    let num_channels = spec.channels;
    if num_channels != 1 && num_channels != 2 {
        return Err(E::EInvalidInput(format!(
            "Número de canales no soportado: {} (se requiere mono o estéreo)",
            num_channels
        )));
    }

    // 3. Inicializar DSP helper y sesión ONNX
    let stft_helper = StftHelper::new();

    let mut cache_guard = ORT_CACHE.lock().unwrap();
    if cache_guard.is_none() {
        *cache_guard = Some(DpdfNetSession::load(&model_paths)?);
    }
    let session = cache_guard.as_mut().unwrap();

    if num_channels == 1 {
        let total_samples = samples_raw.len();
        let total_duration_secs = total_samples as f64 / (SR as f64);
        let samples_f32: Vec<f32> = samples_raw.iter().map(|&s| (s as f32) / 32768.0).collect();

        let merged_samples = process_channel_samples(
            &samples_f32,
            session,
            &stft_helper,
            highpass,
            pause_gate_db,
            dry_blend,
            total_duration_secs,
            progress_cb,
        )?;

        // Escribir WAV mono PCM16
        let mut writer = WavWriter::create(
            out_wav,
            WavSpec {
                channels: 1,
                sample_rate: spec.sample_rate,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .map_err(|e| E::EIo(std::io::Error::other(e)))?;

        for &sample in &merged_samples {
            let val_i16 = (sample * 32768.0).round().clamp(-32768.0, 32767.0) as i16;
            writer
                .write_sample(val_i16)
                .map_err(|e| E::EIo(std::io::Error::other(e)))?;
        }
        writer
            .finalize()
            .map_err(|e| E::EIo(std::io::Error::other(e)))?;
    } else {
        // Estéreo (2 canales)
        let num_frames = samples_raw.len() / 2;
        let total_duration_secs = num_frames as f64 / (SR as f64);

        let mut samples_l = Vec::with_capacity(num_frames);
        let mut samples_r = Vec::with_capacity(num_frames);
        for chunk in samples_raw.chunks_exact(2) {
            samples_l.push((chunk[0] as f32) / 32768.0);
            samples_r.push((chunk[1] as f32) / 32768.0);
        }

        // Progreso canal L (0% a 50%)
        let cb_l = |cur: f64, tot: f64| {
            progress_cb(cur * 0.5, tot);
        };
        let out_l = process_channel_samples(
            &samples_l,
            session,
            &stft_helper,
            highpass,
            pause_gate_db,
            dry_blend,
            total_duration_secs,
            &cb_l,
        )?;

        // Progreso canal R (50% a 100%)
        let cb_r = |cur: f64, tot: f64| {
            progress_cb(total_duration_secs * 0.5 + cur * 0.5, tot);
        };
        let out_r = process_channel_samples(
            &samples_r,
            session,
            &stft_helper,
            highpass,
            pause_gate_db,
            dry_blend,
            total_duration_secs,
            &cb_r,
        )?;

        // Escribir WAV estéreo PCM16 intercalado
        let mut writer = WavWriter::create(
            out_wav,
            WavSpec {
                channels: 2,
                sample_rate: spec.sample_rate,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .map_err(|e| E::EIo(std::io::Error::other(e)))?;

        for i in 0..num_frames {
            let val_l = (out_l[i] * 32768.0).round().clamp(-32768.0, 32767.0) as i16;
            let val_r = (out_r[i] * 32768.0).round().clamp(-32768.0, 32767.0) as i16;
            writer
                .write_sample(val_l)
                .map_err(|e| E::EIo(std::io::Error::other(e)))?;
            writer
                .write_sample(val_r)
                .map_err(|e| E::EIo(std::io::Error::other(e)))?;
        }
        writer
            .finalize()
            .map_err(|e| E::EIo(std::io::Error::other(e)))?;
    }

    Ok(())
}

/// Procesa las muestras de un único canal a través del pipeline STFT -> inferencia -> iSTFT -> post-filtros.
#[allow(clippy::too_many_arguments)]
fn process_channel_samples(
    samples_f32: &[f32],
    session: &mut DpdfNetSession,
    stft_helper: &StftHelper,
    highpass: bool,
    pause_gate_db: f64,
    dry_blend: f32,
    total_duration_secs: f64,
    progress_cb: &dyn Fn(f64, f64),
) -> Result<Vec<f32>, E> {
    let total_samples = samples_f32.len();
    let padded = stft_helper.pad_signal(samples_f32);
    let total_frames = (padded.len().saturating_sub(crate::df::stft::FFT_SIZE))
        / crate::df::stft::HOP_SIZE
        + 1;

    if total_frames <= crate::df::stft::FFT_SIZE / crate::df::stft::HOP_SIZE {
        return Err(E::EInvalidInput(
            "Audio demasiado corto para el modelo".to_string(),
        ));
    }

    let step = (total_frames / 20).max(1);
    let mut stream = session.stream();

    let synthesized = stft_helper.process_streaming(
        &padded,
        total_samples,
        |frame, out_dest| {
            let out_ref = stream.process_frame(frame)?;
            out_dest.copy_from_slice(out_ref);
            Ok(())
        },
        |frames_done, total_f| {
            if frames_done % step == 0 || frames_done == total_f {
                let done_secs = frames_done as f64 / total_f as f64 * total_duration_secs;
                progress_cb(done_secs, total_duration_secs);
            }
        },
    )?;

    // Post-filtrado pasa-altos opcional
    let mut merged_samples: Vec<f32> = synthesized;
    if highpass {
        apply_highpass(&mut merged_samples, SR, HIGHPASS_CUTOFF_HZ);
    }

    // Pause gate
    apply_pause_gate(&mut merged_samples, pause_gate_db);

    // Dry blend opcional
    if dry_blend > 0.0 {
        let dry_factor = dry_blend.clamp(0.0, 1.0);
        let wet_factor = 1.0 - dry_factor;
        for (wet, &dry) in merged_samples.iter_mut().zip(samples_f32) {
            *wet = *wet * wet_factor + dry * dry_factor;
        }
    }

    Ok(merged_samples)
}

/// Muestras por marco del gate: 30 ms a 48 kHz.
const GATE_FRAME: usize = 1440;

/// Distancia al pico del clip por debajo de la cual el marco cuenta como voz.
const GATE_HI_DB: f64 = 30.0;

/// Distancia al pico por debajo de la cual el marco cuenta como pausa: con
/// `GATE_EXIT_FRAMES` y el colchón de `GATE_HANGOVER` forma la histéresis que
/// evita trocear la voz por un marco suelto por debajo del umbral.
const GATE_LO_DB: f64 = 45.0;

/// Marcos seguidos en zona `GATE_LO_DB` exigidos para salir de voz.
const GATE_EXIT_FRAMES: usize = 3;

/// Marcos que la voz sigue a ganancia 1.0 tras dejar de detectarse (150 ms).
const GATE_HANGOVER: usize = 5;

/// Tasa de cruce por cero mínima para clasificar marcos de energía intermedia
/// (`db >= peak - GATE_LO_DB`) como fonemas sordos/fricativos y evitar cortes de voz.
const GATE_ZCR_FRICATIVE_THRESHOLD: f64 = 0.22;

/// Muestras del fundido raised-cosine centrado en cada transición de ganancia.
const GATE_FADE: usize = GATE_FRAME;

/// Aplica el gate de pausa validado (R3c) a la señal ya sintetizada, justo
/// antes de cuantizar a PCM16: atenúa `crush_db` donde no hay voz y deja
/// el resto intacto. Si `crush_db >= 0.0`, el gate se desactiva por completo.
fn apply_pause_gate(samples: &mut [f32], crush_db: f64) {
    if samples.len() < GATE_FRAME || crush_db >= 0.0 {
        return;
    }
    let n_frames = samples.len() / GATE_FRAME;

    let mut rms = Vec::with_capacity(n_frames);
    let mut zcr = Vec::with_capacity(n_frames);
    for frame in samples[..n_frames * GATE_FRAME].chunks_exact(GATE_FRAME) {
        let mean = frame.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>() / GATE_FRAME as f64;
        rms.push(10.0 * mean.max(1e-20).log10());
        let crossings = frame
            .windows(2)
            .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
            .count();
        zcr.push(crossings as f64 / (GATE_FRAME - 1) as f64);
    }
    let peak = rms.iter().copied().fold(f64::NEG_INFINITY, f64::max);

    let mut voice = false;
    let mut quiet = 0usize;
    let mut state = Vec::with_capacity(n_frames);
    for i in 0..n_frames {
        let db = rms[i];
        let fricative = db >= peak - GATE_LO_DB && zcr[i] >= GATE_ZCR_FRICATIVE_THRESHOLD;
        let hi = db >= peak - GATE_HI_DB || fricative;
        if voice {
            quiet = if hi { 0 } else { quiet + 1 };
            if db <= peak - GATE_LO_DB && quiet >= GATE_EXIT_FRAMES {
                voice = false;
                quiet = 0;
            }
        } else if hi {
            voice = true;
        }
        state.push(voice);
    }

    let mut keep = Vec::with_capacity(n_frames);
    let mut hang = 0usize;
    for &is_voice in &state {
        hang = if is_voice {
            GATE_HANGOVER + 1
        } else if hang > 0 {
            hang - 1
        } else {
            0
        };
        keep.push(is_voice || hang > 0);
    }

    let crush = 10f64.powf(crush_db / 20.0) as f32;
    let mut gain = Vec::with_capacity(n_frames * GATE_FRAME);
    for &is_voice in &keep {
        gain.extend(std::iter::repeat_n(
            if is_voice { 1.0 } else { crush },
            GATE_FRAME,
        ));
    }

    let mid = (1.0 + crush) / 2.0;
    let transitions: Vec<usize> = gain
        .windows(2)
        .enumerate()
        .filter(|(_, w)| (w[0] > mid) != (w[1] > mid))
        .map(|(i, _)| i)
        .collect();

    for &t in &transitions {
        let start = t.saturating_sub(GATE_FADE / 2 - 1);
        let end = gain.len().min(t + GATE_FADE / 2 + 1);
        if end <= start {
            continue;
        }
        let span = end - start;
        let a = gain[t];
        let b = gain.get(t + 1).copied().unwrap_or(a);
        let denom = (span - 1).max(1) as f32;
        for (k, g) in gain[start..end].iter_mut().enumerate() {
            let ramp = 0.5 - 0.5 * (PI * k as f32 / denom).cos();
            *g = a + (b - a) * ramp;
        }
    }

    for (sample, &g) in samples.iter_mut().zip(gain.iter()) {
        *sample *= g;
    }
    let tail_gain = gain.last().copied().unwrap_or(1.0);
    for sample in &mut samples[n_frames * GATE_FRAME..] {
        *sample *= tail_gain;
    }
}
