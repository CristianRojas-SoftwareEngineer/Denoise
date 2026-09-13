//! Implementación Rust puro de SI-SDR (Scale-Invariant Signal-to-Distortion Ratio).
//!
//! Usado por `test_golden` (RNF-04).
//! Fórmula: `SI-SDR(x, x̂) = 10 · log10( ||x · ŝ||² / ||x - ŝ||² )`
//! donde `ŝ = (x·x̂ / ||x||²) · x` = proyección escalada de `x̂` sobre `x`.
//!
// Ver `docs/specifications.md §6 Fórmula SI-SDR` y `docs/design.md §6`.

/// Calcula SI-SDR entre señal limpia `x` y señal estimada `x_hat`.
///
/// `eps = 1e-8` para evitar división por cero.
pub fn si_sdr(x: &[f32], x_hat: &[f32]) -> f64 {
    assert_eq!(
        x.len(),
        x_hat.len(),
        "Las señales deben tener la misma longitud"
    );

    let eps = 1e-8_f64;

    // Zero-mean (docs/specifications.md §6)
    let mean_x: f64 = x.iter().map(|&v| v as f64).sum::<f64>() / (x.len() as f64);
    let mean_xhat: f64 = x_hat.iter().map(|&v| v as f64).sum::<f64>() / (x_hat.len() as f64);

    let x: Vec<f64> = x.iter().map(|&v| v as f64 - mean_x).collect();
    let x_hat: Vec<f64> = x_hat.iter().map(|&v| v as f64 - mean_xhat).collect();

    // ||x||²
    let norm_x_sq: f64 = x.iter().map(|&v| v * v).sum();

    // x · x̂ (producto punto)
    let dot_product: f64 = x.iter().zip(&x_hat).map(|(a, b)| a * b).sum();

    // ||x̂||²
    let norm_xhat_sq: f64 = x_hat.iter().map(|&v| v * v).sum();

    // Si x̂ es casi cero o x es cero
    if norm_xhat_sq < eps || norm_x_sq < eps {
        return f64::NEG_INFINITY;
    }

    // s_target = (dot_product / norm_x_sq) * x
    // ||s_target||² = dot_product² / norm_x_sq
    let numerator = dot_product * dot_product / norm_x_sq;

    // e_noise = x_hat - s_target
    // ||e_noise||² = ||x_hat||² - ||s_target||²
    let denominator = norm_xhat_sq - numerator;

    if denominator <= eps {
        return 100.0; // Prácticamente idéntica / paridad perfecta
    }

    if numerator <= eps {
        return f64::NEG_INFINITY;
    }

    // SI-SDR = 10 · log10( ||s_target||² / ||e_noise||² )
    10.0 * (numerator / denominator).log10()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_si_sdr_identical() {
        // Señal idéntica con energía AC → SI-SDR = 100.0 dB (paridad perfecta)
        let x: Vec<f32> = (0..100).map(|i| (i as f32 * 0.1).sin()).collect();
        let x_hat = x.clone();
        let result = si_sdr(&x, &x_hat);
        assert!(
            result >= 60.0,
            "Señal idéntica debe dar SI-SDR >= 60 dB, got {}",
            result
        );
    }

    #[test]
    fn test_si_sdr_zero() {
        // x_hat todo ceros → -infinito
        let x: Vec<f32> = (0..100).map(|i| (i as f32 * 0.1).sin()).collect();
        let x_hat = vec![0.0_f32; 100];
        let result = si_sdr(&x, &x_hat);
        assert!(
            result.is_infinite() && result < 0.0,
            "x_hat cero debe dar -inf, got {}",
            result
        );
    }
}
