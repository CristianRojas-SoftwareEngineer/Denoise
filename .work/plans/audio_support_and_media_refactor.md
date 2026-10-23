# Plan Canónico de Arquitectura e Implementación: Soporte Integral de Medios (Audio y Video) y Refactorización del Pipeline

> **Ubicación canónica:** `.work/plans/audio_support_and_media_refactor.md`  
> **Estado del Plan:** Aprobado para ejecución  
> **Ámbito:** `Denoise/` (CLI, librerías Rust, tests y documentación)  
> **Principio Rector:** **Cero deuda técnica y cero retrocompatibilidad innecesaria.** Dado que el producto se encuentra en fase activa de desarrollo y sin usuarios finales externos dependientes de contratos legacy, se aplica una refactorización de raíz que sustituye nombres obsoletos, elimina alias redundantes y unifica el modelo mental sin crear shims temporales.

---

## 1. Visión y Modelo Mental Canónico

### 1.1. El Problema del Diseño Inicial
La aplicación fue modelada bajo un sesgo de dominio: *"Una herramienta de video que procesa audio"*. Esto provocó que la arquitectura subordinara el flujo de datos a los contenedores de video:
- Nombres fuertemente acoplados (`clean_one_video`, `remux_copy`, `is_supported_video`, `*.part.mp4`).
- Validaciones artificiales en `probe()` que fallaban con error `E_INVALID_INPUT` si la entrada era solo-audio.
- Hardcodeo de la extensión `.mp4` en múltiples puntos de `src/cli.rs` y `src/pipeline.rs`.
- Existencia de alias duplicados por deuda técnica (ej. `verify_output_ligero` como wrapper de `verify_output_lightweight`).

### 1.2. El Nuevo Modelo Mental Unificado
El núcleo DSP ([src/df/mod.rs](file:///c:/Users/Cristian/Desktop/Proyectos/Voices/Denoise/src/df/mod.rs)) siempre ha sido un procesador de audio continuo (PCM16 / `f32` a 48 kHz). Por tanto, la abstracción canónica es:

> **`denoise` es un motor de mejora de voz sobre medios (`Media Denoise Engine`).**
> 
> Todo archivo de entrada es un **Medio (`Media`)**. El pipeline se estructura en 4 fases universales:
> 1. **Ingest (Universal):** Decodifica cualquier medio (audio o video) a WAV mono PCM16 a 48,000 Hz mediante FFmpeg.
> 2. **DSP (Puro Rust):** Inferencia neuronal DPDFNet (ONNX) + VAD Pause Gate.
> 3. **Egress (Especializado):** Ensambla la salida definitiva según el tipo de medio (`MediaKind`):
>    - `MediaKind::Video`: Stream-copy de video (`-c:v copy`) + audio limpio codificado en AAC.
>    - `MediaKind::Audio`: Transcodificación o copia del audio limpio preservando el contenedor original (`.wav`, `.mp3`, `.m4a`, etc.) y metadatos.
> 4. **Commit & Verify:** Verificación runtime adaptada al medio y renombrado atómico (`.part` $\rightarrow$ final).

---

## 2. Requerimientos Funcionales y No Funcionales

### 2.1. Requerimientos Funcionales Canónicos (RF)

- **RF-01 (Entradas de Medios Soportadas):**
  Aceptar indistintamente archivos de video (`mp4`, `mov`, `mkv`, `webm`, `avi`) y archivos de audio (`wav`, `mp3`, `m4a`, `aac`, `flac`, `ogg`, `opus`), insensibles a mayúsculas.
- **RF-02 (Clasificación Fuertemente Tipada):**
  La inspección `probe()` clasifica la entrada como `MediaKind::Video` o `MediaKind::Audio`. Un archivo de solo audio es completamente válido. Si un archivo no tiene streams válidos de audio ni video, aborta con `E_INVALID_INPUT`. Si tiene video pero carece de audio, aborta con `E_NO_AUDIO`.
- **RF-03 (Resolución Simétrica y Natural de Nombres):**
  - **Video:** Por defecto genera salida `.mp4` (para contener el video copiado y el audio AAC).
  - **Audio:** Por defecto conserva la extensión original de entrada (`entrevista.wav` $\rightarrow$ `entrevista_denoised.wav`, `podcast.mp3` $\rightarrow$ `podcast_denoised.mp3`).
  - **`--output-name NAME`:** Si `NAME` incluye extensión, se respeta estrictamente. Si `NAME` no incluye extensión, se añade `.mp4` si la entrada era video, o la extensión original si era audio.
- **RF-04 (Exclusión Recursiva Canónica):**
  En escaneos recursivos, la exclusión de archivos ya procesados comprueba si `file_stem` termina en `suffix` (ej. `_denoised`), sin acoplarse a ninguna extensión específica.
- **RF-05 (Egresión de Audio Optimizada):**
  - Salida `.wav`: Copia / escritura directa PCM16 sin pérdida (o transcodificación mínima).
  - Salida comprimida con pérdida (`.mp3`, `.m4a`, `.aac`, `.ogg`, `.opus`): Codificación mediante FFmpeg usando el `--audio-bitrate` especificado y preservación de metadatos (`-map_metadata 0`).
  - Salida comprimida sin pérdida (`.flac`): Codificación FLAC nativa.
- **RF-06 (Verificación Ligera Adaptativa):**
  `verify_output_lightweight` valida que el archivo de salida exista, tenga tamaño $>0$, duración dentro de $\pm0.5\,\text{s}$ y contenga stream de audio. Solo exige stream de video si el medio es `MediaKind::Video`.

### 2.2. Requerimientos No Funcionales y DoD

- **Cero Deuda Técnica:** Sin funciones obsoletas, sin wrappers deprecados (`verify_output_ligero` eliminado), nombres canónicos en todo el codebase (`clean_media` en vez de `clean_one_video`, `is_supported_media` en vez de `is_supported_video`).
- **DoD Estricto:**
  1. `cargo build --release`
  2. `cargo clippy --all-targets -- -D warnings` (0 warnings).
  3. `cargo test --release` (100% verde).
  4. Puerta de calidad DSP (`python tools/quality/benchmark.py run ...`) sin regresiones.

---

## 3. Matriz de Formatos y Comandos de Egresión

| Formato Entrada | Tipo de Medio | Salida Defecto | Comando de Egresión FFmpeg |
| :--- | :--- | :--- | :--- |
| `mp4, mov, mkv, webm, avi` | `Video` | `.mp4` | `ffmpeg -y -v error [-ignore_editlist 1] -i IN -i TMP.out.wav -map 0:v:0 -map 1:a:0 -c:v copy -c:a aac -b:a <kbps>k [-af loudnorm=...] -t <dur> OUT.part.mp4` |
| `wav` | `Audio` | `.wav` | `ffmpeg -y -v error -i TMP.out.wav -c:a pcm_s16le [-af loudnorm=...] OUT.part.wav` |
| `mp3` | `Audio` | `.mp3` | `ffmpeg -y -v error -i IN -i TMP.out.wav -map 1:a:0 -map_metadata 0 -c:a libmp3lame -b:a <kbps>k [-af loudnorm=...] OUT.part.mp3` |
| `m4a, aac` | `Audio` | `.m4a` / `.aac` | `ffmpeg -y -v error -i IN -i TMP.out.wav -map 1:a:0 -map_metadata 0 -c:a aac -b:a <kbps>k [-af loudnorm=...] OUT.part.m4a` |
| `flac` | `Audio` | `.flac` | `ffmpeg -y -v error -i IN -i TMP.out.wav -map 1:a:0 -map_metadata 0 -c:a flac [-af loudnorm=...] OUT.part.flac` |
| `ogg` | `Audio` | `.ogg` | `ffmpeg -y -v error -i IN -i TMP.out.wav -map 1:a:0 -map_metadata 0 -c:a libvorbis -b:a <kbps>k [-af loudnorm=...] OUT.part.ogg` |
| `opus` | `Audio` | `.opus` | `ffmpeg -y -v error -i IN -i TMP.out.wav -map 1:a:0 -map_metadata 0 -c:a libopus -b:a <kbps>k [-af loudnorm=...] OUT.part.opus` |

---

## 4. Plan de Implementación Paso a Paso (Checklist de Tareas)

### Fase 1: Abstracción Canónica en `src/ffmpeg_io.rs`

- **Tarea 1.1: Definir `MediaKind`**
  - **Estado:** Pendiente
  - **Acción:** Declarar el enum público:
    ```rust
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum MediaKind {
        Video,
        Audio,
    }
    ```
  - **Archivos:** `src/ffmpeg_io.rs`, re-exportado en `src/lib.rs`.

- **Tarea 1.2: Refactorizar `ProbeResult` y `probe()`**
  - **Estado:** Pendiente
  - **Acción:**
    - Agregar campo `pub media_kind: MediaKind` a `ProbeResult`.
    - Eliminar la validación que lanzaba error para entradas de solo-audio.
    - Asignar `MediaKind::Video` si `has_video && has_audio`.
    - Asignar `MediaKind::Audio` si `!has_video && has_audio`.
    - Si `!has_video && !has_audio` $\rightarrow$ `E::EInvalidInput`.
    - Si `has_video && !has_audio` $\rightarrow$ `E::ENoAudio`.
  - **Archivos:** `src/ffmpeg_io.rs`.

- **Tarea 1.3: Implementar `encode_audio()` y renombrar `remux_copy` a `remux_video`**
  - **Estado:** Pendiente
  - **Acción:**
    - Renombrar `remux_copy()` $\rightarrow$ `remux_video()` con parámetros claros.
    - Implementar `encode_audio(ffmpeg, input, clean_wav, out_part, audio_bitrate, target_lufs)` aplicando la matriz de códecs según la extensión del archivo de salida y pasando metadatos del input con `-map_metadata 0`.
    - Implementar función orquestadora `egress_media()` que despacha a `remux_video()` o `encode_audio()` según `MediaKind`.
  - **Archivos:** `src/ffmpeg_io.rs`.

- **Tarea 1.4: Refactorizar `verify_output_lightweight()` y eliminar alias duplicado**
  - **Estado:** Pendiente
  - **Acción:**
    - Modificar firma: `pub fn verify_output_lightweight(ffmpeg: &Path, out_file: &Path, expected_dur: f64, kind: MediaKind) -> Result<bool, E>`.
    - Si `kind == MediaKind::Audio`, omitir comprobación de stream de video y validar exclusivamente stream de audio y duración $\pm0.5\,\text{s}$.
    - **Eliminar definitivamente** la función `verify_output_ligero` (deuda técnica eliminada).
  - **Archivos:** `src/ffmpeg_io.rs`.

---

### Fase 2: Generalización y Limpieza de la CLI en `src/cli.rs`

- **Tarea 2.1: Definir listas de extensiones canónicas**
  - **Estado:** Pendiente
  - **Acción:**
    ```rust
    pub const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mov", "mkv", "webm", "avi"];
    pub const AUDIO_EXTENSIONS: &[&str] = &["wav", "mp3", "m4a", "aac", "flac", "ogg", "opus"];
    ```
    Reemplazar `is_supported_video` directamente por `is_supported_media` (que evalúa si pertenece a `VIDEO_EXTENSIONS` o `AUDIO_EXTENSIONS`) y funciones auxiliares directas `is_video` e `is_audio`.
  - **Archivos:** `src/cli.rs`.

- **Tarea 2.2: Actualizar mensajes de usuario y metadata de `clap`**
  - **Estado:** Pendiente
  - **Acción:**
    - `about = "CLI autocontenido denoise v1 — elimina ruido de audio y video con DPDFNet ONNX"`.
    - Mensajes de error: Cambiar `"Lote vacío sin videos soportados"` $\rightarrow$ `"Lote vacío sin archivos de medios soportados"`.
    - Mensaje de `--output-name`: `"exactamente 1 video"` $\rightarrow$ `"exactamente 1 archivo de medio"`.
    - Fallback de stem: Cambiar `unwrap_or("video")` $\rightarrow$ `unwrap_or("media")`.
  - **Archivos:** `src/cli.rs`.

- **Tarea 2.3: Desacoplar filtro de exclusión recursiva en `collect_dir_entries()`**
  - **Estado:** Pendiente
  - **Acción:**
    - Eliminar la concatenación rígida `format!("{}.mp4", suffix)`.
    - Evaluar la exclusión comprobando si `path.file_stem()` termina con el `suffix` (ej. `_denoised`) y la extensión es soportada. Esto previene automáticamente el re-procesamiento de cualquier video o audio ya procesado.
  - **Archivos:** `src/cli.rs`.

- **Tarea 2.4: Refactorizar `resolve_outputs()` con simetría de formatos**
  - **Estado:** Pendiente
  - **Acción:**
    - Determinar la extensión de salida:
      - Si la entrada es video soportado $\rightarrow$ extensión `.mp4`.
      - Si la entrada es audio soportado $\rightarrow$ conserva su extensión original.
    - Resolver `--output-name NAME`:
      - Si `NAME` contiene una extensión soportada $\rightarrow$ usarla directamente.
      - Si no contiene extensión $\rightarrow$ adjuntar `.mp4` para video o la extensión original para audio.
  - **Archivos:** `src/cli.rs`.

---

### Fase 3: Unificación del Pipeline en `src/pipeline.rs`

- **Tarea 3.1: Renombrar función principal a `clean_media`**
  - **Estado:** Pendiente
  - **Acción:**
    - Renombrar `clean_one_video` $\rightarrow$ `clean_media` (sin wrappers legacy).
    - Actualizar llamadas en `src/cli.rs` y módulos asociados.
  - **Archivos:** `src/pipeline.rs`, `src/cli.rs`, `src/lib.rs`.

- **Tarea 3.2: Generación dinámica de temporales y mensajes adaptativos**
  - **Estado:** Pendiente
  - **Acción:**
    - En lugar de hardcodear `.part.mp4`, generar el temporal de egresión usando el nombre y extensión final de salida: `<output_stem>.part.<ext>`.
    - Mensajes de progreso: `progress_cb(81, "remuxando video")` para video, `progress_cb(81, "empaquetando audio")` para audio.
    - Invocar `egress_media(...)` pasando el `probe_res.media_kind`.
    - Invocar `verify_output_lightweight(...)` pasando el `probe_res.media_kind`.
  - **Archivos:** `src/pipeline.rs`.

---

### Fase 4: Refactorización y Expansión de Tests en `tests/`

- **Tarea 4.1: Adaptar tests existentes en `tests/test_naming.rs`**
  - **Estado:** Pendiente
  - **Acción:**
    - Reemplazar imports y usos de `is_supported_video` por `is_supported_media`.
    - Actualizar `test_is_supported_video`: renombrar a `test_is_supported_media` y verificar que `.wav`, `.mp3`, `.flac`, etc. ahora son válidos (`true`), mientras que `.txt` o `.pdf` son `false`.
    - Añadir casos de prueba para nombrado de audio: `take.wav` $\rightarrow$ `take_denoised.wav`, `song.flac` $\rightarrow$ `song_denoised.flac`.
    - Probar `--output-name` con archivos de audio con y sin extensión.
  - **Archivos:** `tests/test_naming.rs`.

- **Tarea 4.2: Adaptar test de solo-audio en `tests/test_errors.rs`**
  - **Estado:** Pendiente
  - **Acción:**
    - En `tests/test_errors.rs`, la prueba que antes forzaba que una pista de solo audio fallara (`test_invalid_input_error`) debe actualizarse: verificar que un archivo sin streams (`has_video == false && has_audio == false`) o corrupto falle con `E_INVALID_INPUT`, mientras que un archivo de audio válido sea aceptado.
  - **Archivos:** `tests/test_errors.rs`.

- **Tarea 4.3: Crear `tests/test_audio_pipeline.rs` (Test E2E de Audio)**
  - **Estado:** Pendiente
  - **Acción:**
    - Crear test de integración que procese un archivo de audio real (`tests/data/voz_noisy.wav`) a través de `clean_media` con `FakeProvider`.
    - Verificar que el WAV de salida existe, es legible por `hound`, tiene duración correcta y no generó archivos `.part` o temporales huérfanos.
  - **Archivos:** `tests/test_audio_pipeline.rs`.

---

### Fase 5: Actualización Contractual de Documentación

- **Tarea 5.1: Actualizar `docs/design.md`**
  - **Estado:** Pendiente
  - **Acción:**
    - Actualizar §1 (Objetivo: Video y Audio), §4 (Contrato CLI) y §5 (Pipeline por medio).
    - Describir `MediaKind` y la egresión polimórfica.
    - Asegurar paridad exacta TOC $\leftrightarrow$ encabezados y enlaces GitHub.
  - **Archivos:** `docs/design.md`.

- **Tarea 5.2: Actualizar `docs/specifications.md`**
  - **Estado:** Pendiente
  - **Acción:**
    - Actualizar RF-01, RF-02, RF-03 y RF-07 reflejando el soporte universal de medios.
    - Asegurar paridad exacta TOC $\leftrightarrow$ encabezados.
  - **Archivos:** `docs/specifications.md`.

- **Tarea 5.3: Actualizar `README.md`**
  - **Estado:** Pendiente
  - **Acción:**
    - Actualizar el encabezado y características principales (resaltando soporte directo para podcasts, locuciones y pistas de audio).
    - Añadir ejemplos CLI con archivos `.wav` y `.mp3`.
  - **Archivos:** `README.md`.

- **Tarea 5.4: Registrar en `CHANGELOG.md`**
  - **Estado:** Pendiente
  - **Acción:**
    - Registrar bajo `[Unreleased]` la refactorización arquitectónica de medios y el soporte nativo de archivos de audio.
  - **Archivos:** `CHANGELOG.md`.

---

## 5. Checklist de Verificación y Criterios de Aceptación (DoD)

Para considerar la implementación terminada y aprobada:

1. [ ] `cargo build --release` finaliza exitosamente.
2. [ ] `cargo clippy --all-targets -- -D warnings` arroja **0 warnings**.
3. [ ] `cargo test --release` ejecuta y pasa todos los tests unitarios y de integración.
4. [ ] `python tools/quality/benchmark.py run --set tests_data --work out/bench --profile all` pasa con éxito la puerta de calidad DSP.
5. [ ] Verificación de prueba manual:
   ```bash
   target/release/denoise tests/data/voz_noisy.wav --output-name out_test.wav
   ```
   Comprobar que `out_test.wav` se genera limpio y reproducible.
