//! Módulo `df` — Orquestación principal del DSP de DPDFNet.
//!
//! Submódulos: `stft`, `net`.
//! Contrato: `docs/design.md §6`, `docs/specifications.md §RF-05, RNF-04`.
//!
//! DPDFNet 48 kHz opera como grafo único stateful: la normalización de features
//! (ERB y spec) ocurre dentro del ONNX, por lo que este módulo no calcula bandas
//! ERB. El pipeline es STFT -> frames al modelo encadenando estado -> iSTFT.

pub mod net;
pub mod stft;

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

/// Procesa un archivo WAV con un proveedor de modelos específico (permite tests/custom model-dir).
pub fn denoise_wav_with_provider(
    in_wav: &Path,
    out_wav: &Path,
    model_dir: &Path,
    provider: &dyn ModelsProvider,
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

    let total_samples = samples_raw.len();
    let total_duration_secs = total_samples as f64 / (SR as f64);

    // Convertir i16 a f32 [-1.0, 1.0]
    let samples_f32: Vec<f32> = samples_raw.iter().map(|&s| (s as f32) / 32768.0).collect();

    // 3. Inicializar DSP helper y sesión ONNX
    let stft_helper = StftHelper::new();

    let mut cache_guard = ORT_CACHE.lock().unwrap();
    if cache_guard.is_none() {
        *cache_guard = Some(DpdfNetSession::load(&model_paths)?);
    }
    let session = cache_guard.as_mut().unwrap();

    // 4. Recorrido secuencial: STFT -> inferencia encadenando estado -> iSTFT.
    // El modelo es stateful, así que no se trocea ni se reinicia el estado.
    let padded = stft_helper.pad_signal(&samples_f32);
    let stft_frames = stft_helper.forward_stft(&padded);

    if stft_frames.len() <= crate::df::stft::FFT_SIZE / crate::df::stft::HOP_SIZE {
        return Err(E::EInvalidInput(
            "Audio demasiado corto para el modelo".to_string(),
        ));
    }

    let enhanced_frames = session.infer(&stft_frames)?;

    // Síntesis iSTFT con el recorte exacto de la referencia (`apply_istft`).
    let synthesized = stft_helper.inverse_stft(&enhanced_frames, total_samples);

    progress_cb(total_duration_secs, total_duration_secs);

    // 5. Sin normalización ni limiter: sherpa-onnx tampoco normaliza (con
    // `attenuation_limit_db = 0`); solo recorta a [-1, 1] al cuantizar.
    // Cualquier ganancia global o soft-clip altera la escala que el modelo
    // produce y rompe la paridad con la referencia. El gate de pausa no
    // reescala: solo atenúa donde no hay voz y deja la voz por 1.0.
    let mut merged_samples: Vec<f32> = synthesized;
    apply_pause_gate(&mut merged_samples);

    // 6. Escribir WAV de salida PCM16
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
        // Como `soundfile.write(..., subtype=PCM_16)`: escala por 32768 y recorta.
        let val_i16 = (sample * 32768.0).round().clamp(-32768.0, 32767.0) as i16;
        writer
            .write_sample(val_i16)
            .map_err(|e| E::EIo(std::io::Error::other(e)))?;
    }
    writer
        .finalize()
        .map_err(|e| E::EIo(std::io::Error::other(e)))?;

    Ok(())
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

/// Atenuación de las pausas en dB; la ganancia lineal vale `10^(dB / 20)`.
const GATE_CRUSH_DB: f64 = -25.0;

/// Muestras del fundido raised-cosine centrado en cada transición de ganancia.
const GATE_FADE: usize = GATE_FRAME;

/// Aplica el gate de pausa validado (R3c) a la señal ya sintetizada, justo
/// antes de cuantizar a PCM16: atenúa `GATE_CRUSH_DB` donde no hay voz y deja
/// el resto intacto. No normaliza ni reescala la señal: la voz queda
/// multiplicada por 1.0 y, por tanto, bit-idéntica a la síntesis.
///
/// Por qué: el residuo que el modelo deja en las pausas está muy por encima de
/// la señal limpia — medido sobre la salida, oficina queda +27 dB sobre el pico
/// de la limpia, restaurante +6 dB y tren +4 dB, mientras que en las zonas de
/// voz el nivel ya está a pico. Ese residuo hunde SI-SDR, p5, STOI y PESQ sin
/// que la voz pueda compensarlo, así que el gate es la única palanca que sube
/// las métricas sin canjear calidad de voz. Algoritmo R3c validado en la
/// batería (7/7 verde); ver `docs/benchmark.md`.
///
/// Cómo: VAD solo sobre la salida — RMS de `GATE_FRAME` muestras en dB contra el
/// pico del clip, voz a `GATE_HI_DB`, salida con histéresis a `GATE_LO_DB` tras
/// `GATE_EXIT_FRAMES` marcos y colchón de `GATE_HANGOVER` marcos tras la voz —
/// y después ganancia por marco con fundido raised-cosine de `GATE_FADE`
/// muestras en cada transición. Solo se analiza la parte múltiple de
/// `GATE_FRAME`: la cola final conserva la ganancia del último marco y una
/// señal más corta que un marco se devuelve intacta.
fn apply_pause_gate(samples: &mut [f32]) {
    if samples.len() < GATE_FRAME {
        return;
    }
    let n_frames = samples.len() / GATE_FRAME;

    let mut rms = Vec::with_capacity(n_frames);
    for frame in samples[..n_frames * GATE_FRAME].chunks_exact(GATE_FRAME) {
        let mean = frame.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>() / GATE_FRAME as f64;
        rms.push(10.0 * mean.max(1e-20).log10());
    }
    let peak = rms.iter().copied().fold(f64::NEG_INFINITY, f64::max);

    let mut voice = false;
    let mut quiet = 0usize;
    let mut state = Vec::with_capacity(n_frames);
    for &db in &rms {
        let hi = db >= peak - GATE_HI_DB;
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

    let crush = 10f64.powf(GATE_CRUSH_DB / 20.0) as f32;
    let mut gain = Vec::with_capacity(n_frames * GATE_FRAME);
    for &is_voice in &keep {
        gain.extend(std::iter::repeat_n(if is_voice { 1.0 } else { crush }, GATE_FRAME));
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
