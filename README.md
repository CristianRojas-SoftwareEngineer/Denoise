# denoise — CLI autocontenido v1.0.0

Elimina ruido de la pista de audio de uno o varios videos usando **DeepFilterNet3 ONNX** + **ort CPU**.

## Instalación

Requisitos:
- **Rust stable 1.88+** (`rustup`)
- **ffmpeg 6+** (`winget install Gyan.FFmpeg`, `choco install ffmpeg`, `brew install ffmpeg`, `apt install ffmpeg`)

```bash
cargo build --release
```

## Uso

```bash
# Un archivo
denoise boda.mp4

# Un archivo con prefijo, sufijo y bitrate
denoise boda.mp4 --prefix pod- --suffix _clean --audio-bitrate 128

# Lote recursivo con directorio de salida
denoise ./crudos/ --recursive --output-dir ./limpios/ --skip-existing --json

# Nombre personalizado (solo lote == 1)
denoise a.mp4 --output-name final --output-dir limpio
```

Ver `docs/design.md §4` para el contrato CLI completo.

## Limitaciones v1

- Solo denoise de voz con DeepFilterNet3 (sin separación MDX, sin compresión, sin efectos adicionales).
- Batch secuencial, mono, primera pista de audio.
- Video sin re-encode (`-c:v copy`); solo se procesa el audio.
- `<dur_video>` del remux = duración del contenedor (`Duration:` del probe). Si la pista de audio dura más que el video, la salida se alarga a la duración del contenedor (limitación v1, D_k).

## Atribución

- Código: MIT License (`LICENSE`).
- Modelo DeepFilterNet3: `MIT © Rikorose/DeepFilterNet` — https://github.com/Rikorose/DeepFilterNet
- ffmpeg: externo, licencias GPL/LGPL.

## Verificación

```powershell
# Windows (PowerShell)
.\verify.ps1
```

`verify.ps1` ejecuta `cargo build --release` + `cargo test` (sin `--ignored`) → PASS/FAIL.

## Estructura

- `docs/design.md` — arquitectura, DSP, contrato CLI.
- `docs/specifications.md` — requerimientos funcionales (RF), no funcionales (RNF), casos borde, DoD.
- `docs/plan.md` — plan de implementación por fases.
- `src/` — código fuente Rust.
- `tests/` — tests de integración.
- `examples/` — generador de vectores de test (`gen_vectors.rs`).
