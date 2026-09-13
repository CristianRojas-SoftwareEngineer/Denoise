/// Módulo `tests/common` — utilidades compartidas entre tests de integración.
///
/// Incluye `si_sdr.rs` (implementación Rust puro de SI-SDR).
/// Ver `specifications.md RNF-04`.

// Re-exporta el módulo si_sdr para que cada test pueda usarlo con
// `#[path = "common/si_sdr.rs"] mod si_sdr;`
pub mod si_sdr;
