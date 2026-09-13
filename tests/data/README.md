# Datos de test

Vectores PCM16 deterministas para `test_golden` (RNF-04).

## Generación

Generados manualmente con `examples/gen_vectors.rs` (`cargo run --example gen_vectors`).
Nunca se generan en `cargo test` ni CI (preserva el congelado D14).

## Archivos

- `voz.wav` — seno 440Hz 3s, SR 48k, mono PCM16 (D_l cerrado 2026-09-13, opción A)
- `mezcla10dB.wav` — `voz.wav` + ruido blanco SNR 10dB, semilla 0
- `referencia_dfn3.wav` — generada con el propio port tras validar mejora, luego congelada (D37)
- `voz65s.wav` — seno 440Hz 65s, SR 48k, mono PCM16 (2 chunks + crossfade)
- `mezcla65s10dB.wav` — `voz65s.wav` + ruido blanco SNR 10dB, semilla 1
- `referencia65s_dfn3.wav` — referencia larga congelada

> **⚠️ Estos archivos NO están en git.** Se generan manualmente con el ejemplo `gen_vectors.rs`.
