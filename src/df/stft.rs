//! Módulo `df::stft` — Framing, Vorbis STFT, iSTFT y normalización WNORM.
//!
//! Contrato: `docs/design.md §6`, `docs/specifications.md §RF-05, RNF-04`.
//! Constantes: `SR48000 / FFT960 / HOP480 / LOOKAHEAD2 / WNORM=1/FFT`.

use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};
use std::f32::consts::PI;
use std::sync::Arc;

pub const SR: usize = 48000;
pub const FFT_SIZE: usize = 960;
pub const HOP_SIZE: usize = 480;
pub const NB_BINS: usize = 481; // FFT_SIZE / 2 + 1
pub const LOOKAHEAD: usize = 2;
pub const WNORM: f32 = 1.0 / (FFT_SIZE as f32);

/// Genera la ventana Vorbis de tamaño 960.
/// w[i] = sin( pi/2 * sin^2( pi * (i + 0.5) / N ) )
pub fn vorbis_window(n: usize) -> Vec<f32> {
    let mut win = Vec::with_capacity(n);
    let n_f = n as f32;
    for i in 0..n {
        let t = (i as f32 + 0.5) / n_f;
        let inner = (PI * t).sin();
        let val = (PI * 0.5 * inner * inner).sin();
        win.push(val);
    }
    win
}

/// Estructura para STFT / iSTFT streaming con ventana Vorbis y buffers precalculados.
pub struct StftHelper {
    window: Vec<f32>,
    fft_forward: Arc<dyn Fft<f32>>,
    fft_inverse: Arc<dyn Fft<f32>>,
}

impl StftHelper {
    pub fn new() -> Self {
        let mut planner = FftPlanner::new();
        let fft_forward = planner.plan_fft_forward(FFT_SIZE);
        let fft_inverse = planner.plan_fft_inverse(FFT_SIZE);
        let window = vorbis_window(FFT_SIZE);
        Self {
            window,
            fft_forward,
            fft_inverse,
        }
    }

    /// Aplica padding inicial (HOP) y cola (FFT + LOOKAHEAD * HOP) para framing.
    pub fn pad_signal(&self, signal: &[f32]) -> Vec<f32> {
        let initial_pad = HOP_SIZE;
        let tail_pad = FFT_SIZE + LOOKAHEAD * HOP_SIZE;
        let mut padded = Vec::with_capacity(initial_pad + signal.len() + tail_pad);
        padded.extend(std::iter::repeat_n(0.0, initial_pad));
        padded.extend_from_slice(signal);
        padded.extend(std::iter::repeat_n(0.0, tail_pad));
        padded
    }

    /// Calcula la STFT de una señal con padding.
    /// Retorna una matriz de frames, cada frame con 481 bins complejos.
    pub fn forward_stft(&self, padded_signal: &[f32]) -> Vec<Vec<Complex32>> {
        let num_frames = (padded_signal.len().saturating_sub(FFT_SIZE)) / HOP_SIZE + 1;
        let mut frames = Vec::with_capacity(num_frames);
        let mut buffer = vec![Complex32::new(0.0, 0.0); FFT_SIZE];

        for t in 0..num_frames {
            let offset = t * HOP_SIZE;
            for i in 0..FFT_SIZE {
                let sample = padded_signal[offset + i];
                let win_val = self.window[i];
                buffer[i] = Complex32::new(sample * win_val * WNORM, 0.0);
            }

            self.fft_forward.process(&mut buffer);

            // Guardar los 481 bins positivos
            let mut frame_bins = Vec::with_capacity(NB_BINS);
            frame_bins.extend_from_slice(&buffer[..NB_BINS]);
            frames.push(frame_bins);
        }

        frames
    }

    /// Calcula la iSTFT con síntesis overlap-add y ventana Vorbis.
    pub fn inverse_stft(&self, frames: &[Vec<Complex32>], target_len: usize) -> Vec<f32> {
        let num_frames = frames.len();
        let total_samples = num_frames * HOP_SIZE + FFT_SIZE;
        let mut out_buffer = vec![0.0_f32; total_samples];
        let mut buffer = vec![Complex32::new(0.0, 0.0); FFT_SIZE];

        for (t, frame) in frames.iter().enumerate() {
            buffer[0] = Complex32::new(frame[0].re, 0.0);
            for k in 1..NB_BINS - 1 {
                buffer[k] = frame[k];
                buffer[FFT_SIZE - k] = frame[k].conj();
            }
            buffer[NB_BINS - 1] = Complex32::new(frame[NB_BINS - 1].re, 0.0);

            self.fft_inverse.process(&mut buffer);

            let offset = t * HOP_SIZE;
            for i in 0..FFT_SIZE {
                // Inversión Vorbis: ventana de síntesis
                let sample = buffer[i].re * self.window[i];
                out_buffer[offset + i] += sample;
            }
        }

        // Recorte: HOP_SIZE .. HOP_SIZE + target_len
        let start = HOP_SIZE;
        let end = (start + target_len).min(out_buffer.len());
        if start < end {
            out_buffer[start..end].to_vec()
        } else {
            Vec::new()
        }
    }
}

impl Default for StftHelper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stft_perfect_reconstruction() {
        let stft = StftHelper::new();
        let num_samples = 48000; // 1s
        let mut original = Vec::with_capacity(num_samples);
        for i in 0..num_samples {
            let t = i as f32 / (SR as f32);
            original.push((2.0 * PI * 440.0 * t).sin() * 0.5);
        }

        let padded = stft.pad_signal(&original);
        let frames = stft.forward_stft(&padded);
        let reconstructed = stft.inverse_stft(&frames, num_samples);

        assert_eq!(original.len(), reconstructed.len());

        let mut max_err = 0.0_f32;
        for i in 0..num_samples {
            let err = (original[i] - reconstructed[i]).abs();
            if err > max_err {
                max_err = err;
            }
        }

        eprintln!("STFT reconstruction max error: {}", max_err);
        assert!(
            max_err < 1e-4,
            "STFT debe tener reconstrucción perfecta, max_err = {}",
            max_err
        );
    }
}
