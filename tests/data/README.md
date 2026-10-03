# Datos de test

Vectores PCM16 deterministas para `test_golden` (RNF-04).

## Generación

Generados con `examples/gen_vectors.rs`:

```bash
cargo run --release --example gen_vectors -- <dir_eval>
```

donde `<dir_eval>` contiene `Clean/` y `Noisy/` del conjunto de evaluación de
DPDFNet (`Ceva-IP/DPDFNet_EvalSet`, Apache 2.0). Nunca se generan en
`cargo test` ni CI (congelado).

## Modelo

Las referencias se generaron con **DPDFNet 48 kHz HR**
(`dpdfnet8_48khz_hr.onnx`, Ceva-IP/DPDFNet, Apache 2.0) ejecutado por este
mismo port en Rust y congeladas como línea base de regresión. La del par 3 s
sigue **pre-gate**; la del par 65 s se **re-congeló el 2026-10-03** tras
validar el gate de pausa, regenerándola con este mismo port sobre
`voz65s_noisy.wav` congelado (`examples/process_wav.rs`, mismo camino
`denoise_wav`; el EvalSet completo no está en la máquina). Paridad verificada
contra sherpa-onnx (1 LSB PCM16) en el pipeline pre-gate: con el gate activo la
voz sigue bit-idéntica (×1.0), pero la salida completa ya no, porque las pausas
se atenúan −25 dB tras la síntesis. Ambos dorados en verde — 67,77 dB y
100,00 dB — sin relajar el umbral ≥60 dB (ver
[docs/design.md](../../docs/design.md) §6).

## Archivos

- `voz_clean.wav` - voz real 3s, SR 48k, mono PCM16 (ventana de máxima energía)
- `voz_noisy.wav` - `voz_clean.wav` + ruido a SNR 0 dB
- `referencia_dpdfnet.wav` - salida de DPDFNet sobre `voz_noisy.wav`, congelada
- `voz65s_clean.wav` - voz real 65s, SR 48k, mono PCM16
- `voz65s_noisy.wav` - `voz65s_clean.wav` + ruido a SNR 10 dB
- `referencia65s_dpdfnet.wav` - salida de DPDFNet sobre `voz65s_noisy.wav`, congelada
- `<escena>_snr<N>_clean.wav` / `<escena>_snr<N>_noisy.wav` - ventanas de 15s
  con >=40% de voz activa, cortadas de clips largos del EvalSet
  (`pub`/`car`/`office`/`train`/`restaurant` a SNR 0/5 dB) y remuestreadas de
  16kHz a 48kHz. Solo para el benchmark (sin referencia congelada: la salida
  la genera `run` en cada corrida).

> **Nota:** Estos archivos son deterministas y se mantienen congelados. Si se requiere regenerarlos manualmente, usar `cargo run --release --example gen_vectors -- <dir_eval>`; la referencia del par 65 s puede re-congelarse sin EvalSet con `examples/process_wav.rs` sobre `voz65s_noisy.wav` (mismo camino `denoise_wav`).

## Benchmark de calidad

El par (`voz_clean.wav`, `voz_noisy.wav`) es además parte del set `tests_data` del benchmark (`tools/quality/clips.json`): las salidas del DSP sobre estos mismos archivos se puntúan contra la voz limpia. Ver [docs/metrics.md](../../docs/metrics.md) y [docs/benchmark.md](../../docs/benchmark.md). El par de 65 s salió del set el 2026-10-03 (ver `docs/benchmark.md` §8) y queda solo como fixture de `test_golden_65s`. Si se regeneran o sustituyen los vectores, hay que re-ejecutar el benchmark y actualizar `tools/quality/baseline.json`.
