# Changelog

## [Unreleased]

### Docs
- `README.md`: overhaul de precisión y estructura. Secciones y subsecciones numeradas (`1.`-`11.` y `N.M`) con índice navegable de 24 entradas; nuevas secciones `Limitaciones Conocidas` y `Troubleshooting` (tabla error → exit → causa → solución); subsección de coste del primer uso (modelo ~15 MB, red solo en la primera corrida, ~50 MB libres); contrato CLI exacto (`--output-name`, `--output-dir`, `--json` como JSONL en streaming, `--dry-run`, `--verbose`, `--version`) y ejemplos corregidos.
- `docs/design.md` y `docs/specifications.md`: índices navegables nuevos, con la numeración `§N` conservada por ser contrato citado desde `src/` y los tests. Numeración de §4 normalizada a `1.`-`8.`, referencias cruzadas rotas corregidas y residuos de sintaxis limpiados.
- Documentación alineada con el comportamiento real del código: se retiran afirmaciones que ninguna verificación implementaba (`±0.2s` de duración en RF-05 y `±10%` de bitrate en RF-05B, ambos sustituidos por los umbrales realmente verificados, `±0.5s`), y se corrige la descripción de `--verbose` en RF-10, que atribuía un log de comandos ffmpeg inexistente.
- Eliminados `docs/plan.md` y `verify.ps1`; el fixture manual renombrado a `assets/e2e_vertical_1080x1920_16s.mp4`.

### Tests
- `test_golden_65s`: el log marcaba la mejora con un umbral `mín 5.0 dB` que ese par no asserta (solo verifica paridad); ahora se informa como dato no verificado.

## 1.0.0 (2026-09-13)

### Features
- Versión inicial de `denoise` CLI v1.0.0 Rust (DPDFNet ONNX + ort CPU).
- Batch secuencial, mono, primera pista de audio. Video con `-c:v copy`.
- Salida `.mp4` con audio AAC limpio al bitrate pedido.
- Flags: `--output-name`, `--output-dir`, `--prefix`, `--suffix`, `--recursive`, `--overwrite|--skip-existing`, `--audio-bitrate`, `--model-dir`, `--ffmpeg-path`, `--dry-run`, `--json`, `--verbose`, `--version`.
- Modelo DPDFNet (`dpdfnet8_48khz_hr.onnx`, Ceva-IP, Apache 2.0): descarga verificada (SHA256), cacheada en `~/.cache/denoise/models/`. Grafo único stateful, inferencia secuencial sin trocear, salida bit-exacta con sherpa-onnx (1 LSB PCM16, media SI-SDR 14.74 dB en el EvalSet, RTF ≤ 1.0).
- Cancelación con `Ctrl+C` → exit 3, sin archivos parciales.
- Sin normalización artificial: la salida conserva la escala exacta del modelo.
- Sincronización A/V perfecta en contenedores `mov/mp4` con edit lists (`-ignore_editlist 1`).

### Docs
- Especificación completada: `docs/design.md`, `docs/specifications.md`.
- Autocontenido: binario nativo standalone, sin servidor, sin desktop app.
