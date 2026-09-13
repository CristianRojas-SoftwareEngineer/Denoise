//! Módulo `df::erb` — Filtro ERB, extracción de features y constantes DeepFilterNet3.
//!
//! Contrato: `docs/design.md §6`, `docs/specifications.md §RF-05, RNF-04`.
//! Constantes: `NB_ERB=32 / NB_DF=96 / ALPHA=0.99 / MIN_ERB=-15.0 / MAX_ERB=35.0 / MAX_DF=20.0`.

use crate::df::stft::{FFT_SIZE, NB_BINS, SR};
use rustfft::num_complex::Complex32;

pub const NB_ERB: usize = 32;
pub const NB_DF: usize = 96;
pub const ALPHA: f32 = 0.99;
pub const LSNR_MIN: f32 = -15.0; // D37 opción B
pub const LSNR_MAX: f32 = 35.0; // D37 opción B
pub const MAX_DF_LSNR: f32 = 20.0; // D37 opción B

/// Genera los anchos de banda ERB para 48kHz y 960 FFT.
pub fn erb_widths() -> Vec<usize> {
    let freq2erb = |f: f64| 9.2645 * (1.0 + f / 228.8455).ln();
    let erb2freq = |e: f64| 228.8455 * ((e / 9.2645).exp() - 1.0);

    let max_freq = (SR / 2) as f64;
    let erb_max = freq2erb(max_freq);

    let mut bin_indices = Vec::with_capacity(NB_ERB + 1);
    for i in 0..=NB_ERB {
        let e = (i as f64) * erb_max / (NB_ERB as f64);
        let f = erb2freq(e);
        let bin = ((f * (FFT_SIZE as f64) / (SR as f64)).round() as usize).min(NB_BINS);
        bin_indices.push(bin);
    }

    // Asegurar que comience en 0 y termine en NB_BINS (481)
    bin_indices[0] = 0;
    bin_indices[NB_ERB] = NB_BINS;

    let mut widths = Vec::with_capacity(NB_ERB);
    for i in 0..NB_ERB {
        let w = (bin_indices[i + 1] - bin_indices[i]).max(1);
        widths.push(w);
    }

    // Ajustar si la suma difiere ligeramente de NB_BINS
    let sum: usize = widths.iter().sum();
    if sum != NB_BINS {
        let diff = NB_BINS as isize - sum as isize;
        if let Some(last) = widths.last_mut() {
            *last = (*last as isize + diff).max(1) as usize;
        }
    }

    widths
}

/// Estructura para cálculo de filtros ERB y normalización de features.
pub struct ErbFilterbank {
    pub widths: Vec<usize>,
    pub band_indices: Vec<usize>, // Mapea cada bin (0..481) a su banda ERB (0..32)
}

impl ErbFilterbank {
    pub fn new() -> Self {
        let widths = erb_widths();
        let mut band_indices = Vec::with_capacity(NB_BINS);
        for (b, &w) in widths.iter().enumerate() {
            for _ in 0..w {
                if band_indices.len() < NB_BINS {
                    band_indices.push(b);
                }
            }
        }
        while band_indices.len() < NB_BINS {
            band_indices.push(NB_ERB - 1);
        }

        Self {
            widths,
            band_indices,
        }
    }

    /// Extrae `feat_erb` [T, 32] y `feat_spec` [T, 96, 2] a partir de los frames STFT.
    pub fn extract_features(
        &self,
        frames: &[Vec<Complex32>],
    ) -> (Vec<Vec<f32>>, Vec<Vec<[f32; 2]>>) {
        let num_frames = frames.len();
        let mut feat_erb = Vec::with_capacity(num_frames);
        let mut feat_spec = Vec::with_capacity(num_frames);

        let mut mean_erb = [-60.0_f32; NB_ERB];
        let mut mean_spec_norm = 0.04_f32;

        for frame in frames {
            // 1. Energía por bin
            let mut power = [0.0_f32; NB_BINS];
            for (k, c) in frame.iter().enumerate() {
                power[k] = c.norm_sqr();
            }

            // 2. Energía por banda ERB
            let mut erb_power = [0.0_f32; NB_ERB];
            for (k, &p) in power.iter().enumerate().take(NB_BINS) {
                let b = self.band_indices[k];
                erb_power[b] += p;
            }
            for (b, width) in self.widths.iter().enumerate().take(NB_ERB) {
                erb_power[b] /= *width as f32;
            }

            // Log power en dB
            let mut erb_db = [0.0_f32; NB_ERB];
            for (b, &p) in erb_power.iter().enumerate().take(NB_ERB) {
                erb_db[b] = 10.0 * (p + 1e-10).log10();
            }

            // 3. Spec norm para los primeros 96 bins
            let mut spec_energy = 0.0_f32;
            for p in power.iter().take(NB_DF) {
                spec_energy += *p;
            }
            let current_spec_norm = (spec_energy + 1e-10).sqrt();

            // 4. Actualización de medias móviles (EMA alpha=0.99)
            for b in 0..NB_ERB {
                mean_erb[b] = ALPHA * mean_erb[b] + (1.0 - ALPHA) * erb_db[b];
            }
            mean_spec_norm = ALPHA * mean_spec_norm + (1.0 - ALPHA) * current_spec_norm;

            // 5. Normalización feat_erb: (erb_db - mean) / 40.0
            let mut norm_erb = Vec::with_capacity(NB_ERB);
            for b in 0..NB_ERB {
                norm_erb.push((erb_db[b] - mean_erb[b]) / 40.0);
            }
            feat_erb.push(norm_erb);

            // 6. Normalización feat_spec: complex / mean_spec_norm
            let inv_spec_norm = 1.0 / mean_spec_norm.max(1e-10);
            let mut norm_spec = Vec::with_capacity(NB_DF);
            for c in frame.iter().take(NB_DF) {
                norm_spec.push([c.re * inv_spec_norm, c.im * inv_spec_norm]);
            }
            feat_spec.push(norm_spec);
        }

        (feat_erb, feat_spec)
    }
}

impl Default for ErbFilterbank {
    fn default() -> Self {
        Self::new()
    }
}
