# Nuevo diseño — CLI autocontenida `denoise-videos` v1 Rust

> Alcance: única funcionalidad — eliminar ruido de la pista de audio de uno o varios videos.
> Repo autocontenido: este repo (`Video-Noise-Remover/` raíz). Sin runtime Python.
> Proyecto original solo como referencia de lectura Python: `../NextgenUp/` (`app.py`, `paths.py`, `model_store.py`, `audio_engine.py`, `video_tools.py`, `Flask`, `Tauri`). Se reutiliza su **lógica** probada (pipeline extract → DeepFilterNet3 → remux) portándola a Rust solo cuando corresponda, con **nueva implementación** mínima y dedicada. Origen fijado: `../NextgenUp@16e01bb` (2026-09-09, `Release v1.2.0`); tras el port manda el test dorado, no el original.
> Documentos canónicos y autocontenidos en la raíz: este diseño + `especificaciones.md`. En caso de divergencia, el contrato CLI de §4 manda.

## 1. Objetivo y no-objetivos

**Objetivo:** CLI `denoise` que recibe 1..N videos, les quita ruido de fondo/hiss/ambiente de la voz y entrega nuevos `.mp4` con el video intacto.

**No-objetivos explícitos (se prescinde del original):**

* Sin servidor web, sin desktop app, sin base de datos de tareas, sin polling.
* Sin `Vocal Remover / Mastering / Tag Editor / Compresor / GIF / Upscale / Restauración facial`.
* Sin separación MDX (`Kim_Vocal_2`). Solo `denoise` voz.
* Sin concurrencia masiva v1. Batch secuencial, determinista.

## 2. Principios de diseño

1. **Autocontenida Rust:** `std + ort + ndarray + rustfft + hound + clap + indicatif + reqwest + ffmpeg` binario. Sin `Python/torch/librosa/Flask/pillow/opencv`.
2. **Portar Python→Rust, no reinventar DSP:** constantes y orden de operaciones de DeepFilterNet3 desde `audio_engine.py` se copian exactos. El riesgo es regresión numérica.
3. **Video nunca se re-codifica:** `-c:v copy`. Solo el audio se procesa.
4. **Fallo explícito y limpio:** exit codes, temps siempre borrados, `String::from_utf8_lossy` en Windows.
5. **CLI predecible para batch y scripting:** `--dry-run`, `--json`, `--skip-existing`, lote secuencial.
6. **Implementación legible Rust:** `stable 1.75+ edition 2021`, `PathBuf`, `clippy+rustfmt`, `anyhow/thiserror`, funciones pequeñas, sin estado global salvo sesiones `ort` cacheadas (`Mutex`).
7. **Testing por pirámide:** unitarias rápidas sin I/O (`cargo test`), doradas DSP bloqueantes + e2e `remux` con `#[ignore]` (slow) vía `cargo test -- --ignored`.
8. **Documentación como contrato:** `README.md` (MIT + atribución DFN3) + `--help` + ejemplos idénticos a §4; `CHANGELOG.md` por release (inicial `1.0.0`); `LICENSE` MIT en raíz.

## 3. Estructura mínima propuesta (todo dentro de este repo, raíz `./`)

```
./  (Video-Noise-Remover/)
  nuevo-diseño.md            # este documento (arquitectura + pipeline)
  especificaciones.md        # RF/RNF canónicos + aceptación
  LICENSE                  # MIT (código nuevo)
  README.md                  # instalación, 3 ejemplos, atribución MIT © Rikorose/DeepFilterNet
  CHANGELOG.md               # por release (inicial 1.0.0)
  Cargo.toml                 # bin denoise, version="1.0.0" fuente única (D1-Rust), edition 2021
  Cargo.lock                 # congelado en repo
  src/
    main.rs                  # bin denoise, parseo clap + salida proceso
    cli.rs                   # clap, expansión entradas, naming salida, reporte lote
    pipeline.rs              # clean_one_video(): extract → denoise → remux + verificación
    df.rs                    # DSP DeepFilterNet3 + inferencia ort (puro, sin I/O proceso)
    models.rs                # descarga/cache/verificación de dfn3_*.onnx
    ffmpeg_io.rs             # localización ffmpeg, has_audio, extract, remux, verify
    errors.rs                # E_* + exit codes
  tests/
    test_naming.rs           # reglas prefix/suffix/output-dir/-o (sin ffmpeg/red)
    common/si_sdr.rs         # helper SI-SDR Rust puro
    test_golden.rs           # tono 10dB ruido blanco → mejora SI-SDR (#[ignore] slow)
    test_remux.rs            # codec/res/fps/duración/bitrate, sin re-encode (#[ignore] slow)
    test_errors.rs           # E_* + overwrite/skip + Ctrl+C simulado
    test_reporter.rs         # orden [i/N], JSONL parseable, summary ok/failed/skipped
    data/README.md           # spec vectores sintéticos + generador determinista (seed 0)
```

Nada Python en runtime. `../NextgenUp@16e01bb/` es solo referencia de lectura Python. `models.rs` porta `../NextgenUp@16e01bb/model_store.py` (solo rama denoise); `ffmpeg_io.rs` porta el `_ffmpeg()` disperso de `../NextgenUp@16e01bb/audio_engine.py` y `../NextgenUp@16e01bb/video_tools.py`; `df.rs` porta la sección denoise de `../NextgenUp@16e01bb/audio_engine.py`; `pipeline.rs` porta `../NextgenUp@16e01bb/video_tools.clean_audio(mode='denoise')` (solo rama denoise). Se porta lógica solo cuando corresponda, sin ejecutar el original.

## 4. Contrato CLI (v1)

```text
denoise INPUT... [-o OUT | --output-dir DIR] [--prefix STR] [--suffix STR]
  [--recursive] [--overwrite | --skip-existing]
  [--audio-bitrate KBPS] [--model-dir DIR] [--ffmpeg-path PATH]
  [--dry-run] [--json] [-v] [--version]
```

* `INPUT...`: 1..N rutas. Cada una puede ser archivo (`mp4/mov/mkv/webm/avi`) o directorio. Entry-point congelado: bin `denoise` desde `src/main.rs` (`cargo run -- ...` equivalente, `version` desde `Cargo.toml`).
* `--audio-bitrate`: bitrate AAC de salida en kbps (defecto `192`, rango `64-320`).
* Reglas de salida (precedencia):
  1. Si `N==1` y `-o OUT`: ese archivo exacto.
  2. Si `--output-dir DIR`: `DIR/<prefix><stem><suffix>.mp4`, recreando subcarpetas si `--recursive`.
  3. Por defecto: junto al original como `<stem><suffix>.mp4` con `suffix=_denoised`, `prefix=""`.
  4. Colisión sin `--overwrite`: error salvo `--skip-existing` (marca `skipped`, exit 0).
* Ejemplos:
  * `denoise boda.mp4`
  * `denoise boda.mp4 --prefix pod- --suffix _clean --audio-bitrate 128`
  * `denoise ./crudos/ --recursive --output-dir ./limpios/ --skip-existing --json`

## 5. Pipeline por video (el corazón reutilizado)

```
expandir → validar extensión/existencia → resolver salida (puras, sin I/O)
  → has_audio? → modelo listo? → colisión?
  → [1-5%] ffmpeg extract mono 48k a TMP.in.wav
  → [6-80%] df::denoise() por chunks 60s/1s con callback (d,t)
  → [81-95%] ffmpeg remux copy+AAC a OUT.part.mp4 → rename atómico `std::fs::rename` a OUT.mp4 (con `--overwrite` reemplaza; decisión Ronda 4)
  → [96-99%] verificar OUT existe y >0B → limpiar temps → [100%] report {ok,failed,skipped}
```

Responsabilidades estrictas:
* `cli.rs`: parseo `clap`, expansión determinista, `resolve_output()` pura, bucle lote secuencial, reporte. No DSP. D6: expansión excluye `--output-dir` si está dentro de `INPUT` + aviso en `-v`.
* `pipeline.rs`: orquesta un video, traduce progreso a `0-100` según D4, garantiza limpieza + borrado parcial en error/`Ctrl+C`. D5 verificación ligera runtime: `>0B + duración ±0.5s + Audio AAC presente`.
* `df.rs`: solo `ndarray+rustfft+ort`, firma `denoise_wav(in_wav: &Path, out_wav: &Path, progress_cb)`. Sin `Command`, sin `println!` en núcleo. D7: I/O WAV con `hound` PCM16 ↔ `f32` (`/32768.0`, clip antes de `i16`).
* `ffmpeg_io.rs`: `find_ffmpeg()`, `has_audio()`, `extract_mono48k()`, `remux_copy()`, `verify_output_ligero()` D5. Todo `Command` argv, `String::from_utf8_lossy`.
* `models.rs`: `ensure_models(model_dir, progress_cb)` idempotente + verificación tamaño (+SHA según D2).

Comandos ffmpeg exactos a reimplementar:

```text
# 1. Extracción (mono 48k exigido por DFN3)
ffmpeg -y -v error -i IN -vn -ac 1 -ar 48000 TMP.in.wav

# 2. Remux (video intacto, audio limpio a AAC con bitrate parametrizable)
ffmpeg -y -v error -i IN -i TMP.out.wav
  -map 0:v:0 -map 1:a:0 -c:v copy -c:a aac -b:a <audio-bitrate>k -shortest OUT.mp4
```

Detección de audio: `ffmpeg -hide_banner -i IN` contiene `Audio:`. Sin audio → error `E_NO_AUDIO`, no se genera salida.

Temporales por trabajo: `<out>.tmp.in.wav`, `<out>.tmp.out.wav` junto a la salida (decisión Ronda 3: se mantiene junto a salida, no `tempdir`). Limpieza garantizada en todos los caminos; con `-v` se conservan para debug. D8-Rust: handler `ctrlc` → `Child::kill`, borra `.part` + temps, `exit 3`; en Win hijo con `CREATE_NEW_PROCESS_GROUP`, observable único Win/POSIX.

## 6. Módulo `df.rs` — especificación DSP (no cambiar valores)

Parámetros fijos:

| Constante | Valor | Origen |
|---|---|---|
| `SR` | 48000, mono `f32` | contrato DFN3 |
| `FFT/HOP` | 960 / 480, ventana vorbis | libDF v0.5.6 |
| `WNORM` | `1/(FFT*FFT/(2*HOP))` | escala análisis |
| `NB_ERB/NB_DF/ORDER` | 32 / 96 / 5 | export oficial |
| `LOOKAHEAD` | 2 frames | alineación salida |
| `ALPHA` | 0.99 | media móvil features |
| `MIN/MAX_ERB/MAX_DF` | -10 / 30 / 20 dB | gating LSNR |
| `CHUNK/OVERLAP` | 60s / 1s, crossfade lineal | memoria constante |

Secuencia por chunk:

1. Framing streaming `frame t = [(t-1)*hop,(t+1)*hop)` + pad inicial `HOP` + cola `FFT+LOOKAHEAD*HOP`.
2. `STFT * ventana vorbis * WNORM` → `spec`.
3. Features: `ERB log-power mean-norm /40` + `unit-norm compleja` con `alpha=0.99`.
4. Inferencia CPU 3 grafos: `enc(feat_erb,feat_spec) → emb,e0..e3,c0,lsnr`; `erb_dec(emb,e3,e2,e1,e0) → mask`; `df_dec(emb,c0) → coefs`.
5. Alineación `k+LOOKAHEAD`, `out = spec*(mask@erb_inv)`; deep-filter taps `k-2..k+2` en bins `0..96` si `lsnr<=20`; si `lsnr>30` intacto; si `lsnr<-10` mute.
6. `iSTFT * FFT * ventana`, overlap-add, recorte `HOP:HOP+n`, crossfade entre chunks.

Criterio de fidelidad D3: `SI-SDR(denoised,voz)-SI-SDR(mezcla,voz) >=5dB` y `SI-SDR(denoised,referencia_dfn3) >=60dB` (bloqueante). Reproducir la salida de referencia del proyecto original (`../NextgenUp@16e01bb`, Python solo lectura) con paridad alta (ref informativa `~77dB`, pipeline oficial `~20.8dB`). Vectores deterministas en `tests/data/`: seno `440Hz 3s` + ruido blanco `SNR 10dB`, `seed 0`, `SR 48k` mono `f32` (`voz.wav`, `mezcla10dB.wav`, `referencia_dfn3.wav` generada una vez con el port validado). `SI-SDR` implementado Rust puro en `tests/common/si_sdr.rs` (D11 zero-mean, `eps=1e-8`). Cualquier desviación bajo umbrales = bug bloqueante.

## 7. Módulo `models.rs` — modelo autocontenido

* Artefactos: `dfn3_enc.onnx`, `dfn3_erb_dec.onnx`, `dfn3_df_dec.onnx` (~8MB total, tarball `7983136B`).
* Origen único: `https://github.com/Rikorose/DeepFilterNet/raw/v0.5.6/models/DeepFilterNet3_onnx.tar.gz`, miembros `tmp/export/{enc,erb_dec,df_dec}.onnx`. Verificación D2: tamaño `7983136B >=98%` siempre + `SHA256` interino solo tamaño+warning hasta T2.5, tras registro `SHA256` obligatorio y mismatch → `E_MODEL_MISSING`; tag `v1.0.0` bloqueado hasta registrarlo (en primera descarga con tamaño ok se calcula el hash real con `Get-FileHash`/`sha256sum` y se sustituye en `especificaciones.md RF-06/RNF-05` + este `§7`).
* Cache: `--model-dir` (defecto `~/.cache/denoise-videos/models/` vía `home`/`dirs` + `PathBuf` en Win/macOS/Linux). Sin `NEXTGENUP_DATA` ni `static/models` del original (`../NextgenUp/paths.py`, `../NextgenUp/static/`).
* Comportamiento: si faltan → descarga con `User-Agent: Video-Noise-Remover/1.0.0`, progreso `indicatif`, verificación tamaño+(SHA según D2), extracción `tar.gz`, borrado archivo. D11: `timeout 30s + retry 3 con backoff`, anti `tar-slip` (solo miembros `tmp/export/{enc,erb_dec,df_dec}.onnx`, rechaza `..`/absolutos), chequeo espacio disco antes de descargar, `.part + rename atómico`. Si la descarga falla → error `E_MODEL_MISSING` con URL y ruta manual esperada. Sesiones `ort` `CPUExecutionProvider`, cacheadas (`Mutex`), un lock de inferencia.

## 8. Módulo `ffmpeg_io.rs` + errores

Resolución: `--ffmpeg-path` → `PATH` (búsqueda `split_paths`) → error `E_FFMPEG_NOT_FOUND`. Requiere `ffmpeg 6+`, verificado con `ffmpeg -version` D11 con regex `ffmpeg version (\d+)\.` (major>=6, `from_utf8_lossy`), sin `ffprobe` (probe y `has_audio` vía `ffmpeg -hide_banner -i`, heredado de `../NextgenUp@16e01bb/` Python). Receta por OS en `README.md`: Win `winget install Gyan.FFmpeg` / `choco install ffmpeg`, macOS `brew install ffmpeg`, Linux `apt install ffmpeg`.

Taxonomía estable (código → exit):
* `E_INVALID_INPUT` → 2: ruta inexistente, extensión no soportada, `0B`, `bitrate` fuera de `64-320`, `-o` con `N>1`, `prefix+suffix` ambos vacíos con salida in-place, D9 directorio sin videos/lote vacío.
* `E_OUTPUT_EXISTS` → 2: destino existe sin `--overwrite` ni `--skip-existing`.
* `E_NO_AUDIO` → 2: sin pista `Audio:`.
* `E_FFMPEG_NOT_FOUND`, `E_MODEL_MISSING`, `E_FFMPEG_FAILED` → 1.
* `E_CANCELLED` (`Ctrl+C` D8) → 3.

| Código | Caso | Salida |
|---|---|---|
| 0 | ok / `skipped` con `--skip-existing` | archivo verificado D5 ligero |
| 1 | `E_FFMPEG_NOT_FOUND`, `E_MODEL_MISSING`, `E_FFMPEG_FAILED` | stderr + no salida parcial |
| 2 | `E_NO_AUDIO`, `E_INVALID_INPUT`, `E_OUTPUT_EXISTS` | nada escrito |
| 3 | `E_CANCELLED` | proceso hijo matado (`Child::kill`, Win `CREATE_NEW_PROCESS_GROUP`), temps y `.part` borrados |

Batch nunca aborta en el primer fallo (salvo `Ctrl+C`): continúa y resume `ok/failed/skipped` con exit D9: `3` si hubo cancelación, si no `1` si hubo algún fallo `1`, si no `2` si hubo fallos `2`, si no `0`.

Reporte dinámico pero ordenado (sin nuevos flags):
* Humano (defecto, TTY): cabecera `[i/N] in → out`, una barra viva por video a `stderr` (`indicatif`, `unit=chunk`), más línea final por video `done|failed|skipped + MB + segundos`. Resumen final siempre visible. Flush explícito, sin emojis, ASCII seguro en `pwsh`.
* Humano sin TTY/CI: sin animación, líneas ` [i/N] name ... 45% msg` cada cambio de fase.
* `-v`: añade a `stderr` comando `ffmpeg` exacto, `model-dir`, tiempos por fase, `chunks d/t`, tamaños `in.wav/out.wav`, y conserva temps.
* `--json`: desactiva animación; `stdout` = `JSONL` una línea por archivo `{input,output,status,message,pct}` + línea final `{summary:{ok,failed,skipped}}` (contrato congelado Ronda 3 + D4: `status ∈ {ok,failed,skipped,dry-run}`, `pct` según mapeo `0/1-5/6-80/81-95/96-99/100`, `summary` sin `pct`); progreso humano suprimido. Parseable con `jq`. `status ∈ {ok,failed,skipped,dry-run}`, `pct 0-100`, `summary` con conteos del lote. Ver `especificaciones.md RF-08`.
* `--dry-run`: tabla `input → output (skip: motivo)` sin crear nada, mismo orden que el lote real (formato congelado Ronda 3 + D4: con `--json` emite `JSONL` con `status="dry-run" pct=0`).
* `--version`: imprime `denoise-videos 1.0.0 + modelo DFN3 v0.5.6 + ffmpeg <ver>` desde `Cargo.toml` vía `env!("CARGO_PKG_VERSION")` (fuente única D1-Rust, formato congelado Ronda 3 + D9: sin `ffmpeg` imprime `ffmpeg missing`). Ver `especificaciones.md RF-10`.

Ejemplo humano:
```text
[2/5] boda.mp4 -> boda_denoised.mp4
  extract 100% | denoise chunk 5/7 71% | remux 100%
[2/5] done 24.1MB en 38s
Summary: ok=4 failed=1 skipped=0
```

## 9. Qué se descarta / qué se porta (trazabilidad)

| Original (`../NextgenUp/`, solo lectura) | Decisión nueva app (en este repo) |
|---|---|
| `video_tools.py: clean_audio(mode='denoise')` (Python lectura) | → `pipeline::clean_one_video()` (solo rama denoise, portar cuando corresponda) |
| `audio_engine.py: load/save/probe/_erb_widths/_df_constants/_df_sessions/_denoise_chunk/denoise` (Python lectura) | → `df.rs` + parte `ffmpeg_io.rs`, valores idénticos |
| `model_store.py: ['denoise_speech']` (Python lectura) | → `models.rs` simplificado, sin resto de modelos |
| `app.py (/api/video/audio)`, threads, `tasks`, `vtools-app.js`, `templates/` | descartado, sustituido por `cli.rs` directo |
| `audio_engine.separate (MDX)`, `compress`, `to_gif` | descartado v1 |
| `paths.py`, `src-tauri/`, `static/vendor/` | descartado |

## 10. Plan de verificación mínima

1. `test_naming` (rápido, sin ffmpeg/red): 1 archivo, N archivos, `--output-dir`, `prefix/suffix`, `-o` solo `N==1`, colisión + `overwrite/skip`, `--recursive` recrea árbol + D6 exclusión, caracteres inválidos → `E_INVALID_INPUT`.
2. `test_golden` (`#[ignore]` slow): wav sintético determinista (seno `440Hz 3s` + ruido blanco `SNR 10dB`, `seed 0`, `SR 48k` mono `f32`) en `tests/data/` → D3: `mejora >=5dB` y `paridad vs referencia >=60dB` (refs informativas `+5dB` / `20.8dB` / `~77dB`). Falla si DSP difiere.
3. `test_remux` (`#[ignore]` slow): video 5s barras+tono → mismo `vcodec/res/fps`, duración `±0.2s`, `acodec=aac,sr=48000,bitrate±10%`, D10 `vcodec` sin re-encode por hash stream video/`extradata` (prohibido comparar tamaño fichero total ±5%).
4. `test_errors`: sin audio → `E_NO_AUDIO`; ffmpeg ausente → `E_FFMPEG_NOT_FOUND`; descarga rota → `E_MODEL_MISSING`; destino existe → `E_OUTPUT_EXISTS`/`skipped`; `bitrate 9999` → `E_INVALID_INPUT`. Nada parcial en disco.
5. `test_reporter`: lote 3 simulado verifica orden `[1/3..3/3]`, `JSONL` parseable, resumen `ok/failed/skipped`, sin animación con `--json` o sin TTY.
6. Manual: 1 video corto Win + lote 5 videos, `--dry-run` primero, luego real + `--json`. Comandos: `cargo test` y `cargo test -- --ignored`.

## 11. Alcance de lenguaje (pivot congelado)

* V1 Rust directo, sin V2. Python (`../NextgenUp`) solo referencia de lectura. Se evaluará reutilizar los crates oficiales `DeepFilterNet` (`libDF`) en lugar de re-portar DSP a mano solo si pasan D3, pero el port exacto desde `audio_engine.py` manda hasta entonces. `ffmpeg 6+` externo se mantiene.
