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
mismo port en Rust. Congeladas como línea base de regresión del pipeline
actual. Paridad verificada contra sherpa-onnx (1 LSB PCM16).

## Archivos

- `voz_clean.wav` - voz real 3s, SR 48k, mono PCM16 (ventana de máxima energía)
- `voz_noisy.wav` - `voz_clean.wav` + ruido a SNR 0 dB
- `referencia_dpdfnet.wav` - salida de DPDFNet sobre `voz_noisy.wav`, congelada
- `voz65s_clean.wav` - voz real 65s, SR 48k, mono PCM16
- `voz65s_noisy.wav` - `voz65s_clean.wav` + ruido a SNR 10 dB
- `referencia65s_dpdfnet.wav` - salida de DPDFNet sobre `voz65s_noisy.wav`, congelada

> **Nota:** Estos archivos son deterministas y se mantienen congelados. Si se requiere regenerarlos manualmente, usar `cargo run --example gen_vectors -- <dir_eval>`.
