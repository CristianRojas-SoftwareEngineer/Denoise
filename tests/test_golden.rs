//! Test golden `#[ignore]` — verificación SI-SDR con vectores PCM16 deterministas.
//!
//! Ver `specifications.md RNF-04` y `design.md §6`.
//! Requiere modelo DFN3 descargado + `ffmpeg 6+`.
//! Ejecutar con `cargo test -- --ignored`.

use std::path::Path;

// TODO: implementar test golden con vectores tests/data/
// SI-SDR(denoised,voz)-SI-SDR(mezcla,voz) >= 5dB Y SI-SDR(denoised,referencia) >= 60dB

#[test]
#[ignore]
fn test_golden_3s() {
    // TODO: test con voz.wav + mezcla10dB.wav + referencia_dfn3.wav
}

#[test]
#[ignore]
fn test_golden_65s() {
    // TODO: test con voz65s.wav + mezcla65s10dB.wav + referencia65s_dfn3.wav
}
