//! Módulo `df::erb` — constantes ERB y features log-power.
//!
//! Ver `plan.md T4.1` y `design.md §6`.
//! Constantes: `MIN_ERB=-15 / MAX_ERB=35 dB`, `ALPHA=0.99`.

/// Features: ERB log-power mean-norm / 40 + unit-norm compleja.
pub fn compute_erb_features(_spectrum: &[Vec<f32>]) -> Vec<Vec<f32>> {
    // TODO: implementar features ERB
    vec![]
}

/// Gating LSNR: si lsnr <= -15 → mute, si lsnr >= 35 → intacto, si lsnr > 20 → sin deep-filter.
pub fn apply_lsnr_gating(_lsnr: f64, _spectrum: &[Vec<f32>]) -> Vec<Vec<f32>> {
    // TODO: implementar gating
    vec![]
}
