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
    // produce y rompe la paridad con la referencia.
    let merged_samples: Vec<f32> = synthesized;

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
