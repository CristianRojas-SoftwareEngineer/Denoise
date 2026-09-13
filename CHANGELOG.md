# Changelog

## 1.0.0 (2026-09-13)

- Versión inicial de `denoise` CLI v1.0.0 Rust (DeepFilterNet3 ONNX + ort CPU).
- Especificación y planificación completadas: `docs/design.md`, `docs/specifications.md`, `docs/plan.md`.
- Fase 0 Bootstrap: estructura del repo Rust con Cargo.toml, esqueleto de módulos, tests canarios.
- Autocontenido: binario nativo standalone, sin servidor, sin desktop app.
- Batch secuencial, mono, primera pista de audio. Video con `-c:v copy`.
- Salida `.mp4` con audio AAC limpio al bitrate pedido.
- Flags: `--output-name`, `--output-dir`, `--prefix`, `--suffix`, `--recursive`, `--overwrite|--skip-existing`, `--audio-bitrate`, `--model-dir`, `--ffmpeg-path`, `--dry-run`, `--json`, `--verbose`, `--version`.
- Modelo DeepFilterNet3: descarga verificada (SHA256), cacheada en `~/.cache/denoise/models/`.
- Cancelación con `Ctrl+C` → exit 3, sin archivos parciales.
- `verify.ps1` local (build + test rápido → PASS/FAIL).
