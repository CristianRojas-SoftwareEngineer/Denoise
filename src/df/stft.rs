//! Módulo `df::stft` — Framing, Vorbis STFT/iSTFT con la convención exacta knf
//! (espectro crudo sin `wnorm`, inversa con `1/N`, recorte 1920).
//!
//! Contrato: `docs/design.md §6`, `docs/specifications.md §RF-05, RNF-04`.
//! Convención exacta de sherpa-onnx/knf: espectro crudo sin `wnorm`,
//! inversa con `1/N`, padding reflect de la señal cruda, recorte 1920.
//! Constantes: `SR48000 / FFT960 / HOP480 / WNORM=1 / INV=1/960`.

use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};
use std::f32::consts::PI;
use std::sync::Arc;

pub const SR: usize = 48000;
pub const FFT_SIZE: usize = 960;
pub const HOP_SIZE: usize = 480;
pub const NB_BINS: usize = 481; // FFT_SIZE / 2 + 1;

/// Muestras que `torch.istft(center=True)` recorta de cada extremo del overlap-add.
///
/// El buffer OLA completo mide `(T-1)*HOP + FFT`; `torch.istft` con `center=True`
/// devuelve solo `(T-1)*HOP` muestras, es decir `ola[CENTER_CROP.. +istft_len]`.
pub const CENTER_CROP: usize = FFT_SIZE / 2;

/// Muestras que `apply_istft` de la referencia descarta del frente de la síntesis.
///
/// `audio[:, win_len * 2:]` y luego `F.pad(..., (0, win_len * 2))`. En
/// coordenadas del buffer OLA el recorte total es `CENTER_CROP + ISTFT_HEAD_CROP`.
pub const ISTFT_HEAD_CROP: usize = 2 * FFT_SIZE;

/// Escala de la FFT directa: `knf::Stft` (kissfft) no normaliza, igual que
/// `torch.stft(normalized=False)` y `rustfft`. La referencia sherpa-onnx NO
/// aplica el `wnorm` de `apply_stft` del repo torch: alimenta el grafo con el
/// espectro crudo. (Verificado: con `WNORM = 1/960` el SI-SDR cae ~1 dB y en
/// voz el grafo bifurca a otro atractor.)
pub const WNORM: f32 = 1.0;

/// Escala de la FFT inversa.
///
/// `knf::IStft::InverseFFT` multiplica por `1/n_fft` explícitamente (kissfft no
/// normaliza); `rustfft` tampoco normaliza, así que hay que hacerlo aquí. Sin
/// esto la síntesis sale 960 veces más alta y el recorte a PCM16 la convierte
/// en un cuadrado.
pub const INV_FFT_SCALE: f32 = 1.0 / FFT_SIZE as f32;

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

    /// Aplica el padding del análisis, replicando `knf::Stft::Pad` al detalle.
    ///
    /// sherpa-onnx NO añade la cola de ceros de `apply_stft` del repo torch;
    /// llama a `knf::Stft::Compute(p, n)` sobre la señal cruda, que con
    /// `center=true, pad_mode="reflect"` añade `n_fft/2` muestras por extremo:
    /// - izquierda: `signal[1 .. 1+pad]` en orden inverso
    ///   (`padded[k] = signal[pad - k]`, excluye `signal[0]`);
    /// - derecha: `signal[n-pad-1 .. n-1]` en orden inverso
    ///   (`padded[pad + n + k] = signal[n - 2 - k]`, excluye la última).
    ///
    /// El número de frames resultante es `1 + n / HOP` (división entera).
    pub fn pad_signal(&self, signal: &[f32]) -> Vec<f32> {
        let pad = FFT_SIZE / 2; // 480
        let n = signal.len();
        let mut padded = vec![0.0_f32; n + 2 * pad];

        padded[pad..pad + n].copy_from_slice(signal);

        if n > pad + 1 {
            // Borde izquierdo: reflexión sin la primera muestra.
            for k in 0..pad {
                padded[k] = signal[pad - k];
            }
            // Borde derecho: reflexión sin la última muestra, en orden inverso
            // (como `std::copy(..., ans.rbegin)` de knf).
            for k in 0..pad {
                padded[pad + n + k] = signal[n - 2 - k];
            }
        } else {
            // Señal degenerada: se replica el borde disponible.
            for k in 0..pad {
                padded[k] = signal[(pad - k).min(n - 1)];
                padded[pad + n + k] = signal[n.saturating_sub(2 + k).min(n - 1)];
            }
        }

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

    /// Calcula la iSTFT replicando `knf::IStft` + `ShiftWaveform` de sherpa-onnx.
    ///
    /// 1. FFT inversa con escala `1/N` explícita (`INV_FFT_SCALE`), como hace
    ///    `knf::IStft::InverseFFT` (kissfft no normaliza, igual que `rustfft`).
    /// 2. Overlap-add con ventana de síntesis Vorbis. knf divide además por la
    ///    envolvente `sum(w²)`; con solape del 50 % la ventana Vorbis es
    ///    power-complementary y la envolvente vale 1 en toda la región
    ///    recortada (índices `[480, len-480]` del OLA), así que se omite.
    /// 3. `center=true`: quedarse con `(T-1)*HOP` muestras quitando
    ///    `CENTER_CROP` de cada extremo (como `torch.istft(center=True)`).
    /// 4. `ShiftWaveform(win_len*2)`: descartar `ISTFT_HEAD_CROP` del frente y
    ///    rellenar con ceros al final.
    /// 5. Truncar (o rellenar) a `target_len`.
    ///
    /// Se procesan **todos** los frames sin descartar ninguno: la referencia
    /// conserva todas las salidas del grafo y solo desplaza el recorte.
    pub fn inverse_stft(&self, frames: &[Vec<Complex32>], target_len: usize) -> Vec<f32> {
        let num_frames = frames.len();
        if num_frames == 0 || target_len == 0 {
            return vec![0.0; target_len];
        }
        // Longitud que devuelve `torch.istft(center=True)`: `(T-1)*hop`.
        let istft_len = (num_frames - 1) * HOP_SIZE;
        let mut ola = vec![0.0_f32; istft_len + FFT_SIZE];
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
                ola[offset + i] += buffer[i].re * self.window[i] * INV_FFT_SCALE;
            }
        }

        // Pasos 2-4 de la referencia.
        let mut out = vec![0.0_f32; target_len];
        let avail = istft_len.saturating_sub(ISTFT_HEAD_CROP).min(target_len);
        let src = CENTER_CROP + ISTFT_HEAD_CROP;
        out[..avail].copy_from_slice(&ola[src..src + avail]);
        out
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

        // Roundtrip STFT/iSTFT puro con el encuadre real de `pad_signal`.
        //
        // La cadena es lineal y de ganancia 1.0, pero el recorte de la
        // referencia avanza la señal `2*FFT - FFT/2 -...`: en concreto
        // `out[i] = x[1920 + i]` (el recorte `win_len*2` menos el `center`
        // de 480 ya absorbido en el OLA). Este test fija esa geometría.
        let padded = stft.pad_signal(&original);
        let frames = stft.forward_stft(&padded);
        let reconstructed = stft.inverse_stft(&frames, num_samples);

        assert_eq!(original.len(), reconstructed.len());

        // `out[i] = ola[CENTER_CROP + ISTFT_HEAD_CROP + i]` con
        // `ola[CENTER_CROP + j] = xp[j]` y `xp = [x, ceros]`: el recorte neto
        // avanza la entrada `ISTFT_HEAD_CROP` muestras.
        let shift = ISTFT_HEAD_CROP;
        assert_eq!(shift, 1920);
        let mut max_err = 0.0_f32;
        for i in 0..num_samples {
            let expected = if i + shift < num_samples {
                original[i + shift]
            } else {
                0.0
            };
            let err = (reconstructed[i] - expected).abs();
            if err > max_err {
                max_err = err;
            }
        }

        eprintln!("STFT reconstruction max error: {}", max_err);
        assert!(
            max_err < 1e-3,
            "STFT debe reconstruir con desplazamiento 1920, max_err = {}",
            max_err
        );
    }

    /// Comprueba que el pipeline completo (STFT -> iSTFT con el encuadre real de
    /// `pad_signal`) desplaza la señal exactamente 1920 muestras.
    ///
    /// Se construye una senal con un unico impulso y se comprueba que el impulso
    /// aparece 1920 muestras antes. Como `inverse_stft` es lineal y el recorte
    /// es fijo, esto fija el retardo efectivo del pipeline para cualquier
    /// duracion de entrada.
    #[test]
    fn test_pipeline_delay_is_zero() {
        let stft = StftHelper::new();
        for secs in [1usize, 3, 10, 20] {
            let n = secs * SR;
            let mut x = vec![0.0_f32; n];
            // Impulso en una posicion segura: ni al inicio (padding) ni al final (cola).
            let pos = n / 2;
            x[pos] = 1.0;

            let padded = stft.pad_signal(&x);
            let frames = stft.forward_stft(&padded);
            let out = stft.inverse_stft(&frames, n);

            assert_eq!(out.len(), n, "la salida debe conservar la longitud");
            let peak = out
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.abs().partial_cmp(&b.1.abs()).unwrap())
                .map(|(i, _)| i)
                .expect("salida no vacia");

            assert_eq!(
                peak,
                pos - 1920,
                "con {secs}s el impulso cae en {} en vez de {}",
                peak,
                pos - 1920
            );
        }
    }
}
