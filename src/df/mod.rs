//! Módulo `df` — Orquestación principal DeepFilterNet3 DSP.
//!
//! Submódulos: `stft`, `erb`, `net`, `overlap`.
//! Contrato: `docs/design.md §6`, `docs/specifications.md §RF-05, RNF-04`.
//! Cada submódulo <300 líneas (D40 opción A).

pub mod erb;
pub mod net;
pub mod overlap;
pub mod stft;

use crate::df::erb::{ErbFilterbank, LSNR_MAX, MAX_DF_LSNR, NB_DF};
use crate::df::net::{DfSessions, ORT_CACHE};
use crate::df::overlap::{merge_processed_chunks, slice_into_chunks};
use crate::df::stft::{StftHelper, LOOKAHEAD, NB_BINS, SR};
use crate::errors::E;
use crate::models::{default_model_dir, DefaultModelsProvider, ModelsProvider};
use hound::{WavReader, WavSpec, WavWriter};
use rustfft::num_complex::Complex32;
use std::path::Path;

/// Procesa un archivo WAV mono 48kHz PCM16 eliminando ruido con DeepFilterNet3.
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

    // 3. Inicializar DSP helpers y sesiones ONNX
    let stft_helper = StftHelper::new();
    let erb_helper = ErbFilterbank::new();

    let mut cache_guard = ORT_CACHE.lock().unwrap();
    if cache_guard.is_none() {
        *cache_guard = Some(DfSessions::load(&model_paths)?);
    }
    let sessions = cache_guard.as_mut().unwrap();

    // 4. Dividir en chunks de 60s con 1s de solapamiento
    let chunk_slices = slice_into_chunks(&samples_f32);
    let mut processed_chunks = Vec::with_capacity(chunk_slices.len());
    let mut processed_samples_count = 0usize;

    for chunk in chunk_slices {
        let chunk_target_len = chunk.len();
        let padded = stft_helper.pad_signal(chunk);
        let stft_frames = stft_helper.forward_stft(&padded);
        let num_frames = stft_frames.len();

        let (feat_erb_vec, feat_spec_vec) = erb_helper.extract_features(&stft_frames);

        // Aplanar a vectores planos para ONNX Runtime
        let mut feat_erb_flat = Vec::with_capacity(num_frames * 32);
        for row in feat_erb_vec.iter().take(num_frames) {
            for &val in row.iter().take(32) {
                feat_erb_flat.push(val);
            }
        }

        let mut feat_spec_flat = Vec::with_capacity(2 * num_frames * 96);
        // Canal 0: partes reales [T, 96]
        for row in feat_spec_vec.iter().take(num_frames) {
            for pair in row.iter().take(96) {
                feat_spec_flat.push(pair[0]);
            }
        }
        // Canal 1: partes imaginarias [T, 96]
        for row in feat_spec_vec.iter().take(num_frames) {
            for pair in row.iter().take(96) {
                feat_spec_flat.push(pair[1]);
            }
        }

        let (mask, coefs, alpha, lsnr) =
            sessions.infer(feat_erb_flat, feat_spec_flat, num_frames)?;

        // Filtrado espectral y alineación: k + LOOKAHEAD (docs/design.md §6)
        let num_out_frames = num_frames.saturating_sub(LOOKAHEAD);
        let mut filtered_frames = Vec::with_capacity(num_out_frames);

        for k in 0..num_out_frames {
            let t = k + LOOKAHEAD;
            let mut out_frame = vec![Complex32::new(0.0, 0.0); NB_BINS];
            let frame_lsnr = if t < lsnr.len() { lsnr[t] } else { 20.0 };
            let frame_alpha = if t < alpha.len() { alpha[t] } else { 1.0 };

            for f in 0..NB_BINS {
                let orig = stft_frames[k][f];

                // Bypass si lsnr > LSNR_MAX (35 dB)
                if frame_lsnr > LSNR_MAX {
                    out_frame[f] = orig;
                    continue;
                }

                // Ganancia ERB usando mask[t]
                let band = erb_helper.band_indices[f];
                let gain = if t < mask.len() && band < mask[t].len() {
                    mask[t][band]
                } else {
                    1.0
                };
                let erb_filtered = orig * gain;

                // Deep Filtering (bins 0..96) si lsnr <= 20
                if f < NB_DF && frame_lsnr <= MAX_DF_LSNR && t < coefs.len() {
                    let mut df_sample = Complex32::new(0.0, 0.0);
                    for (p, tap_coef) in coefs[t][f].iter().enumerate().take(5) {
                        let tap_spec = if k + p >= 2 {
                            let tap_idx = k + p - 2;
                            if tap_idx < num_frames {
                                stft_frames[tap_idx][f]
                            } else {
                                Complex32::new(0.0, 0.0)
                            }
                        } else {
                            Complex32::new(0.0, 0.0)
                        };
                        let c_re = tap_coef[0];
                        let c_im = tap_coef[1];
                        let coef = Complex32::new(c_re, -c_im); // coef.conj()
                        df_sample += coef * tap_spec;
                    }
                    out_frame[f] = df_sample * frame_alpha + erb_filtered * (1.0 - frame_alpha);
                } else {
                    out_frame[f] = erb_filtered;
                }
            }

            filtered_frames.push(out_frame);
        }

        // Síntesis iSTFT
        let synthesized = stft_helper.inverse_stft(&filtered_frames, chunk_target_len);
        processed_chunks.push(synthesized);

        processed_samples_count += chunk_target_len;
        let cur_secs = (processed_samples_count as f64 / (SR as f64)).min(total_duration_secs);
        progress_cb(cur_secs, total_duration_secs);
    }

    // 5. Fusionar chunks procesados con crossfade
    let mut merged_samples = merge_processed_chunks(&processed_chunks, total_samples);

    // 6. Normalización de sonoridad / pico a -1.0 dBFS (Alternativa A)
    // Escala la señal para que el pico máximo quede exactamente en -1.0 dBFS (0.89125),
    // garantizando presencia clara de voz, volumen uniforme y 100% libre de clipping.
    let max_abs = merged_samples
        .iter()
        .map(|s| s.abs())
        .fold(0.0_f32, f32::max);
    if max_abs > 0.0001 {
        let target_peak = 10.0_f32.powf(-1.0 / 20.0); // ≈ 0.89125 (-1.0 dBFS)
        let scale = (target_peak / max_abs).min(20.0); // max +26 dB boost
        for s in &mut merged_samples {
            *s *= scale;
        }
    }

    // 7. Escribir WAV de salida PCM16
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
        let clamped = sample.clamp(-1.0, 1.0);
        let val_i16 = (clamped * 32767.0).round() as i16;
        writer
            .write_sample(val_i16)
            .map_err(|e| E::EIo(std::io::Error::other(e)))?;
    }
    writer
        .finalize()
        .map_err(|e| E::EIo(std::io::Error::other(e)))?;

    Ok(())
}
