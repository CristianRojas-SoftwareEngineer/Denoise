# Changelog

## 1.0.0 (2026-09-13)

### Features
- Versión inicial de `denoise` CLI v1.0.0 Rust (DPDFNet ONNX + ort CPU).
- Batch secuencial, mono, primera pista de audio. Video con `-c:v copy`.
- Salida `.mp4` con audio AAC limpio al bitrate pedido.
- Flags: `--output-name`, `--output-dir`, `--prefix`, `--suffix`, `--recursive`, `--overwrite|--skip-existing`, `--audio-bitrate`, `--model-dir`, `--ffmpeg-path`, `--dry-run`, `--json`, `--verbose`, `--version`.
- Modelo DPDFNet (`dpdfnet8_48khz_hr.onnx`, Ceva-IP, Apache 2.0): descarga verificada (SHA256), cacheada en `~/.cache/denoise/models/`. Grafo único stateful, inferencia secuencial sin trocear, salida bit-exacta con sherpa-onnx (1 LSB PCM16, media SI-SDR 14.74 dB en el EvalSet, RTF ≤ 1.0).
- Cancelación con `Ctrl+C` → exit 3, sin archivos parciales.
- `verify.ps1` local (build + test rápido → PASS/FAIL).
- Sin normalización artificial: la salida conserva la escala exacta del modelo.
- Sincronización A/V perfecta en contenedores `mov/mp4` con edit lists (`-ignore_editlist 1`).

### Docs
- Especificación y planificación completadas: `docs/design.md`, `docs/specifications.md`, `docs/plan.md`.
- Autocontenido: binario nativo standalone, sin servidor, sin desktop app.
