//! Filtros digitales IIR biquad para pre-procesamiento de la señal.
//!
//! Contrato: `docs/design.md §6`.
//! Implementa un filtro pasa-altos Butterworth de 2º orden (biquad) a 70 Hz
//! para eliminar rumble subgrave (golpes de mesa, viento, fuentes de
//! alimentación) antes de la STFT, evitando que componentes inaudibles
//! sesguen la red neuronal y el VAD del pause gate.

use std::f64::consts::PI;

/// Coeficientes de un filtro biquad (Direct Form I).
///
/// Ecuación: `y[n] = b0*x[n] + b1*x[n-1] + b2*x[n-2] - a1*y[n-1] - a2*y[n-2]`
/// (coeficientes ya normalizados por `a0`).
#[derive(Debug, Clone, Copy)]
struct BiquadCoeffs {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

impl BiquadCoeffs {
    /// Diseña un filtro pasa-altos Butterworth de 2º orden.
    ///
    /// `fc` — frecuencia de corte en Hz.
    /// `sample_rate` — frecuencia de muestreo en Hz.
    ///
    /// Referencia: Audio EQ Cookbook de Robert Bristow-Johnson.
    fn highpass(fc: f64, sample_rate: f64) -> Self {
        let w0 = 2.0 * PI * fc / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        // Q = 1/√2 ≈ 0.7071 para Butterworth (máximamente plano en pasabanda)
        let alpha = sin_w0 / (2.0 * std::f64::consts::FRAC_1_SQRT_2);

        let a0 = 1.0 + alpha;
        Self {
            b0: ((1.0 + cos_w0) / 2.0) / a0,
            b1: (-(1.0 + cos_w0)) / a0,
            b2: ((1.0 + cos_w0) / 2.0) / a0,
            a1: (-2.0 * cos_w0) / a0,
            a2: (1.0 - alpha) / a0,
        }
    }
}

/// Estado del filtro biquad (Direct Form I, 2 muestras de historia).
struct BiquadState {
    coeffs: BiquadCoeffs,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl BiquadState {
    fn new(coeffs: BiquadCoeffs) -> Self {
        Self {
            coeffs,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    /// Procesa una muestra y devuelve la muestra filtrada.
    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        let c = &self.coeffs;
        let y = c.b0 * x + c.b1 * self.x1 + c.b2 * self.x2
              - c.a1 * self.y1 - c.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// Frecuencia de corte por defecto del filtro pasa-altos (Hz).
pub const HIGHPASS_CUTOFF_HZ: f64 = 70.0;

/// Aplica un filtro pasa-altos cascadeado (dos secciones biquad de 2º orden)
/// in-place, ofreciendo rolloff efectivo de 4º orden (−24 dB/octava).
///
/// Elimina frecuencias por debajo de `cutoff_hz` para limpiar rumble
/// subgrave antes de la STFT. Opera en `f64` internamente para
/// preservar precisión numérica en los coeficientes IIR.
pub fn apply_highpass(samples: &mut [f32], sample_rate: usize, cutoff_hz: f64) {
    if samples.is_empty() {
        return;
    }
    let coeffs = BiquadCoeffs::highpass(cutoff_hz, sample_rate as f64);
    // Dos pasadas cascadeadas: 2×2º orden = 4º orden efectivo (−24 dB/oct).
    let mut stage1 = BiquadState::new(coeffs);
    let mut stage2 = BiquadState::new(coeffs);
    for s in samples.iter_mut() {
        let x = stage1.process(*s as f64);
        *s = stage2.process(x) as f32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifica que el filtro pasa-altos atenúa significativamente un tono
    /// de 30 Hz (debajo de cutoff 70 Hz) a 48 kHz.
    #[test]
    fn test_highpass_attenuates_subsonic() {
        let sr = 48000;
        let freq = 30.0; // Hz — por debajo del cutoff
        let n = sr * 2; // 2 segundos
        let mut samples: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * freq as f32 * i as f32 / sr as f32).sin())
            .collect();
        let rms_before = (samples.iter().map(|s| s * s).sum::<f32>() / n as f32).sqrt();

        apply_highpass(&mut samples, sr, HIGHPASS_CUTOFF_HZ);

        // Descartar el transitorio inicial (0.2 s)
        let skip = sr / 5;
        let steady = &samples[skip..];
        let rms_after = (steady.iter().map(|s| s * s).sum::<f32>() / steady.len() as f32).sqrt();

        // Debe atenuar al menos 20 dB
        let attenuation_db = 20.0 * (rms_after / rms_before).log10();
        assert!(
            attenuation_db < -20.0,
            "Atenuación insuficiente a 30 Hz: {:.1} dB",
            attenuation_db
        );
    }

    /// Verifica que el filtro pasa-altos deja pasar un tono de 1 kHz
    /// prácticamente sin pérdida.
    #[test]
    fn test_highpass_passes_voice_band() {
        let sr = 48000;
        let freq = 1000.0; // Hz — bien dentro de la pasabanda
        let n = sr * 2;
        let mut samples: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * freq as f32 * i as f32 / sr as f32).sin())
            .collect();
        let rms_before = (samples.iter().map(|s| s * s).sum::<f32>() / n as f32).sqrt();

        apply_highpass(&mut samples, sr, HIGHPASS_CUTOFF_HZ);

        let skip = sr / 5;
        let steady = &samples[skip..];
        let rms_after = (steady.iter().map(|s| s * s).sum::<f32>() / steady.len() as f32).sqrt();

        // Pérdida máxima 0.5 dB en pasabanda
        let loss_db = 20.0 * (rms_after / rms_before).log10();
        assert!(
            loss_db > -0.5,
            "Pérdida excesiva a 1 kHz: {:.2} dB",
            loss_db
        );
    }

    /// Verifica que un buffer vacío no genera pánico.
    #[test]
    fn test_highpass_empty_buffer() {
        let mut empty: Vec<f32> = vec![];
        apply_highpass(&mut empty, 48000, HIGHPASS_CUTOFF_HZ);
        assert!(empty.is_empty());
    }
}
