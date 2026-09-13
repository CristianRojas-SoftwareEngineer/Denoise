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
    assert_eq!(x.len(), x_hat.len(), "Las señales deben tener la misma longitud");

    let eps = 1e-8_f64;

    // Convertir a f64 para precisión
    let x: Vec<f64> = x.iter().map(|&v| v as f64).collect();
    let x_hat: Vec<f64> = x_hat.iter().map(|&v| v as f64).collect();
    let n = x.len() as f64;

    // ||x||²
    let norm_x_sq: f64 = x.iter().map(|&v| v * v).sum();

    // x · x̂ (producto punto)
    let dot_product: f64 = x.iter().zip(&x_hat).map(|(a, b)| a * b).sum();

    // ||x̂||²
    let norm_xhat_sq: f64 = x_hat.iter().map(|&v| v * v).sum();

    // Si x̂ es casi cero, retornar -infinito o 0 según convención
    if norm_xhat_sq < eps {
        return f64::NEG_INFINITY;
    }

    // ŝ = (x·x̂ / ||x||²) · x — proyección escalada
    let scale = dot_product / norm_x_sq.max(eps);

    // ||x · ŝ||² = scale² * ||x||²
    let numerator = scale * scale * norm_x_sq;

    // ||x - ŝ||² = ||x||² - 2*scale*(x·x̂) + scale²*||x||²
    //              = ||x||² - 2*scale*dot_product + scale²*||x||²
    //              = ||x||² - 2*dot_product²/norm_x_sq + dot_product²/norm_x_sq
    //              = ||x||² - dot_product²/norm_x_sq
    let denominator = norm_x_sq - dot_product * dot_product / norm_x_sq.max(eps);

    if denominator.abs() < eps {
        return f64::INFINITY;
    }

    // SI-SDR = 10 · log10(numerator / denominator)
    10.0 * (numerator / denominator.max(eps)).log10()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_si_sdr_identical() {
        // Señal idéntica → SI-SDR = +infinito (o muy alto)
        let x = vec![1.0_f32; 100];
        let x_hat = x.clone();
        let result = si_sdr(&x, &x_hat);
        assert!(result.is_infinite() && result > 0.0, "Señal idéntica debe dar SI-SDR infinito, got {}", result);
    }

    #[test]
    fn test_si_sdr_zero() {
        // x_hat todo ceros → -infinito
        let x = vec![1.0_f32; 100];
        let x_hat = vec![0.0_f32; 100];
        let result = si_sdr(&x, &x_hat);
        assert!(result.is_infinite() && result < 0.0, "x_hat cero debe dar -inf, got {}", result);
    }
}
