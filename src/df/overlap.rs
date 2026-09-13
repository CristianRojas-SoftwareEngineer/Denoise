//! Módulo `df::overlap` — Crossfade lineal y partición de audio en chunks.
//!
//! Contrato: `docs/design.md §6`, `docs/specifications.md §RF-05, RNF-03, RNF-04`.
//! Constantes: `CHUNK = 60s (2880000 samples) / OVERLAP = 1s (48000 samples)`.

use crate::df::stft::SR;

pub const CHUNK_SAMPLES: usize = 60 * SR; // 2,880,000 samples
pub const OVERLAP_SAMPLES: usize = SR; // 48,000 samples
pub const STEP_SAMPLES: usize = CHUNK_SAMPLES - OVERLAP_SAMPLES; // 2,832,000 samples

/// Divide una señal completa en chunks de hasta 60s con 1s de solapamiento.
pub fn slice_into_chunks(samples: &[f32]) -> Vec<&[f32]> {
    if samples.is_empty() {
        return Vec::new();
    }
    if samples.len() <= CHUNK_SAMPLES {
        return vec![samples];
    }

    let mut chunks = Vec::new();
    let mut start = 0;
    while start < samples.len() {
        let end = (start + CHUNK_SAMPLES).min(samples.len());
        chunks.push(&samples[start..end]);
        if end == samples.len() {
            break;
        }
        start += STEP_SAMPLES;
    }
    chunks
}

/// Realiza crossfade lineal de dos segmentos de audio de longitud `overlap_len`.
pub fn linear_crossfade(prev: &[f32], current: &[f32]) -> Vec<f32> {
    let len = prev.len().min(current.len());
    let mut out = Vec::with_capacity(len);
    let len_f = len as f32;

    for i in 0..len {
        let w = i as f32 / len_f;
        let blended = (1.0 - w) * prev[i] + w * current[i];
        out.push(blended);
    }
    out
}

/// Fusiona chunks procesados aplicando crossfade en las zonas de solapamiento de 1s.
pub fn merge_processed_chunks(chunks: &[Vec<f32>], total_target_len: usize) -> Vec<f32> {
    if chunks.is_empty() {
        return Vec::new();
    }
    if chunks.len() == 1 {
        let mut out = chunks[0].clone();
        out.truncate(total_target_len);
        return out;
    }

    let mut merged = Vec::with_capacity(total_target_len);

    for (idx, chunk) in chunks.iter().enumerate() {
        if idx == 0 {
            // Primer chunk: tomamos todo excepto el último 1s
            if chunk.len() > OVERLAP_SAMPLES {
                merged.extend_from_slice(&chunk[..chunk.len() - OVERLAP_SAMPLES]);
            } else {
                merged.extend_from_slice(chunk);
            }
        } else {
            let prev_chunk = &chunks[idx - 1];
            let prev_tail = if prev_chunk.len() >= OVERLAP_SAMPLES {
                &prev_chunk[prev_chunk.len() - OVERLAP_SAMPLES..]
            } else {
                prev_chunk
            };

            let curr_head_len = OVERLAP_SAMPLES.min(chunk.len());
            let curr_head = &chunk[..curr_head_len];

            // Crossfade de 1s
            let crossfaded = linear_crossfade(prev_tail, curr_head);
            merged.extend_from_slice(&crossfaded);

            // Resto del chunk actual
            let is_last = idx == chunks.len() - 1;
            if is_last {
                if chunk.len() > curr_head_len {
                    merged.extend_from_slice(&chunk[curr_head_len..]);
                }
            } else if chunk.len() > curr_head_len + OVERLAP_SAMPLES {
                merged.extend_from_slice(&chunk[curr_head_len..chunk.len() - OVERLAP_SAMPLES]);
            }
        }
    }

    merged.truncate(total_target_len);
    merged
}
