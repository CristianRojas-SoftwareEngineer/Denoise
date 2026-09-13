//! Generador de vectores de test deterministas para `test_golden` (RNF-04).
//!
//! Ejecutar con `cargo run --example gen_vectors`.
//! Nunca se ejecuta en `cargo test` ni CI (preserva el congelado D14).
//!
//! Genera:
//! - `voz.wav`: seno 440Hz 3s, SR 48k, mono PCM16
//! - `mezcla10dB.wav`: voz + ruido blanco SNR 10dB, seed 0
//! - `referencia_dfn3.wav`: generada con el propio port tras validar mejora
//! - `voz65s.wav`: seno 440Hz 65s (2 chunks + crossfade)
//! - `mezcla65s10dB.wav`: voz65s + ruido blanco SNR 10dB, seed 1
//! - `referencia65s_dfn3.wav`: referencia larga congelada
//!
//! Ver `docs/design.md §6` y `docs/specifications.md RNF-04`.

use hound::WavSpec;
use std::fs::File;
use std::io::Write;

fn main() {
    // TODO: implementar generador con rand StdRng seed 0/1 + Box-Muller manual
    println!("gen_vectors: ejecutar para generar vectores de test (Fase 3)");
}

/// Genera un WAV PCM16 mono 48k a partir de una forma de onda.
fn write_wav(_path: &str, _spec: WavSpec, _samples: &[i16]) -> anyhow::Result<()> {
    // TODO: implementar escritura WAV con hound
    anyhow::bail!("gen_vectors not yet implemented (Fase 3)");
}
