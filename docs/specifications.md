# Especificaciones — CLI `denoise` v1 Rust (autocontenida en este repo)

> Este documento junto a `docs/design.md` forma el contrato canónico.
> Convención de rutas: `docs/<fichero>` es relativo a la raíz del repo.
> Idioma CLI y mensajes: inglés técnico para flags/ayuda, este documento en español.
> Convención: `INPUT...` = 1..N rutas; `stem` = nombre sin extensión; salida siempre `.mp4`.

## 1. Resumen

CLI offline-first que limpia ruido de 1..N videos, parametrizando entradas, salida, `prefix/suffix`, `audio-bitrate` y lote. Entrega `.mp4` con video idéntico y audio `AAC` limpio al bitrate pedido.

## 2. Requerimientos funcionales

### RF-01 Entradas individuales y múltiples
* **ID:** RF-01. **Prioridad:** Alta.
* Aceptar `INPUT...` con 1..N rutas de archivo `mp4/mov/mkv/webm/avi` (insensible a mayúsculas).
* Rechazar extensiones no soportadas y rutas inexistentes con error `E_INVALID_INPUT`. Directorio expandido sin videos → `E_INVALID_INPUT` lote vacío; duplicados por absoluto normalizado lexical (absolutizar contra cwd + normalizar `./`/`../` sin tocar disco ni resolver symlinks).
* **Aceptación:** `denoise a.mp4 b.mov` produce 2 salidas; con 1 ruta mala, procesa las válidas y reporta la mala sin abortar lote.

### RF-02 Entrada por directorio y recursividad
* **ID:** RF-02. **Prioridad:** Alta.
* Si `INPUT` es directorio: expande a videos `mp4/mov/mkv/webm/avi` según `--recursive`.
* Orden determinista byte-wise UTF-8 del path absoluto . Sin duplicados tras expandir (absoluto lexical, case-insensitive solo en Win vía `to_lowercase`).
* Si `--output-dir` está dentro de un `INPUT` directorio con `--recursive`, sus contenidos se excluyen del escaneo y se avisa en `--verbose`; evita loop infinito en re-corridas.
*  En escaneo con `--recursive` se excluyen además los ficheros que ya terminan en `<suffix>.mp4` vigente (por defecto `*_denoised.mp4`) y se avisa en `--verbose`; evita generar `*_denoised_denoised.mp4` en re-corridas in-place.
* **Aceptación:** `denoise ./crudos/ --recursive` procesa incluyendo subcarpetas, en orden estable.

### RF-03 Salida parametrizable: nombre, directorio, prefijo, sufijo
* **ID:** RF-03. **Prioridad:** Alta.
* Flags: `-o/--output-name NAME` (solo si lote expandido ==1 video; `--output-name` con lote>1 → `E_INVALID_INPUT`), `--output-dir DIR`, `--prefix STR`, `--suffix STR (defecto `_denoised`)`. `--output-name` y `--output-dir` son COMPLEMENTARIOS (`--output-name` define el nombre base, `--output-dir` el directorio; si `--output-dir` falta se usa cwd. (si `--output-name` no termina en `.mp4` case-insensitive se añade `.mp4` automáticamente; ej. `--output-name final` → `final.mp4`).
* Precedencia: `1) --output-name en DIR (--output-dir o cwd) con auto-.mp4 > 2) --output-dir/<relativo-cwd>/<prefix><stem><suffix>.mp4 recreando árbol si --recursive > 3) junto al original`. `--output-name` sin `--output-dir` → DIR=cwd. (si 2+ entradas del lote resuelven al MISMO path de salida —p.ej. `a.mp4`+`a.mov`→`a_denoised.mp4`— se aplica auto-sufijo incremental al 2º y siguientes: `a_denoised_1.mp4`, `a_denoised_2.mp4`..., con aviso SIEMPRE a `stderr`/`message` no solo `-v`; el reporte lista cada salida real).
*  Se crean directorios padre automáticamente para `--output-name`, `--output-dir` y `--model-dir`; si no creables → `E_IO`.
* `--recursive` + `--output-dir` recrea árbol relativo a cwd: `DIR/<ruta-dada-relativa-a-cwd>/<prefix><stem><suffix>.mp4`; ej. `dirA/sub/x.mp4 → out/dirA/sub/x_denoised.mp4`; archivo suelto `a.mp4 → out/a_denoised.mp4`; absoluto fuera de cwd → solo `stem` + aviso `--verbose`. La exclusión aplica si `DIR` está dentro de `INPUT`.
* `prefix/suffix` solo caracteres `[A-Za-z0-9._-]`, prohibidos `.` y `..` exactos (vacío permitido para uno de los dos, no ambos vacíos si salida es mismo directorio que entrada (evita sobrescribirse). Remux vía `OUT.part.mp4` + rename atómico: `std::fs::rename` reemplaza atómicamente el destino tanto en POSIX como en Win (`MoveFileExW` + `REPLACE_EXISTING`; la garantía de no-sobrescritura descansa exclusivamente en el colisión-check previo de RF-04 `E_OUTPUT_EXISTS`/`skipped`). I/O WAV con `hound` PCM16 + `f32` (`i16→f32 /32768.0` y vuelta con clip); `ffmpeg` produce/consume `PCM16 48k mono`.
* `--audio-bitrate` no afecta al nombre (ver RF-05B). `--output-name` que resuelve a la propia entrada sin `prefix/suffix` efectivo → `E_INVALID_INPUT` exit `2` siempre, incluso con `--overwrite`.
* **Aceptación:**
  * `denoise boda.mp4` → `boda_denoised.mp4`
  * `denoise boda.mp4 --prefix pod- --suffix _clean` → `pod-boda_clean.mp4`
  * `denoise a.mp4 --output-name final` → `final.mp4` (cwd)
  * `denoise a.mp4 --output-name final --output-dir limpio` → `limpio/final.mp4`
  * `denoise a.mp4 --verbose` → salida de debug `stderr` sin modificar salida

* **Tabla de decisión — composición de nombres de salida **:

  | `--output-name` | `--output-dir` | `--prefix` | `--suffix` | Resultado |
  |---|---|---|---|---|
  | NAME | — | — | — | `NAME.mp4` (cwd) |
  | NAME | DIR | — | — | `DIR/NAME.mp4` |
  | NAME | — | (ignorado) | (ignorado) | `NAME.mp4` (cwd) |
  | NAME | DIR | (ignorado) | (ignorado) | `DIR/NAME.mp4` |
  | — | DIR | — | suf | `DIR/<stem><suf>.mp4` |
  | — | DIR | pre | — | `DIR/<pre><stem>.mp4` |
  | — | — | pre | suf | `<pre><stem><suf>.mp4` (cwd, junto al original) |
  | — | — | — | suf | `<stem><suf>.mp4` (cwd, junto al original; defecto suf=`_denoised`) |

  **Reglas:**
 1. `--output-name` tiene **prioridad absoluta**: cuando está presente, `NAME` es el nombre base final y `prefix`/`suffix` se **ignoran** (no se aplican). `--output-name` sin `.mp4` → auto-añade `.mp4`. `--output-name` con lote>1 → `E_INVALID_INPUT`. `--output-name` resolviendo a la propia entrada sin prefix/suffix efectivo → `E_INVALID_INPUT` exit `2` siempre.
 2. `--prefix` y `--suffix` se aplican **solo** cuando `--output-name` NO está presente. Se insertan entre `stem` y `.mp4` (o entre `DIR/` y el nombre final en regla 2). Si `suffix` es `None` → `_denoised`; si `prefix` y `suffix` explícitos son ambos `""` y la salida es in-place (mismo dir que entrada) → `E_INVALID_INPUT`.
  3. `--output-dir` modifica solo la **ruta directorio**, no el nombre. Si `--output-dir` está dentro de `INPUT` → aviso `--verbose` (exclusión).
  4. `--output-name` y `--output-dir` son **COMPLEMENTARIOS**: no son excluyentes; definen nombre base y directorio respectivamente.

### RF-04 No sobrescribir por defecto
* **ID:** RF-04. **Prioridad:** Alta.
* Sin `--overwrite`, si destino existe → error `E_OUTPUT_EXISTS`. Con `--skip-existing` → marca `skipped`, exit 0 si todo lo demás ok.
* `--overwrite` y `--skip-existing` mutuamente excluyentes.
* **Aceptación:** segunda corrida sin flags falla con mensaje claro; con `--skip-existing` la salta; con `--overwrite` la reemplaza.

### RF-05 Denoise de pista de audio (única transformación IA)
* **ID:** RF-05. **Prioridad:** Alta.
* Extraer con `ffmpeg -vn -map 0:a:0 -ac 1 -ar 48000` (`-map 0:a:0` obligatorio para fijar primera pista en multi-audio), inferir DPDFNet (grafo único stateful) en CPU de forma secuencial sin trocear, sin normalización ni limiter (la salida conserva la escala exacta del modelo, bit-exacta con sherpa-onnx), remezclar.
* Video: `-map 0:v:0 -map 1:a:0 -c:v copy -c:a aac -b:a <audio-bitrate>k -t <dur_video>` (sustituye a `-shortest`; `<dur_video>` = duración del CONTENEDOR leída en probe (`Duration:`), dato que el probe sin `ffprobe` no puede exponer; si el audio limpio es más corto, la cola queda muda; si el audio del input es más largo que el video, la salida se alarga a la duración del contenedor (limitación v1 documentada en `docs/design.md §9`); coherente con verificación ligera `±0.5s`).
* **Aceptación:** salida conserva `codec/res/fps` originales, duración `±0.2s`, audio `aac 48k` al bitrate pedido; mejora audible/SI-SDR en test dorado. Verificación runtime ligera: `OUT existe >0B + duración ±0.5s vía ffmpeg -i + presencia stream Audio presente`; verificación `±0.5s` + hash h264 en `test_remux`.

### RF-05B Bitrate de audio parametrizable
* **ID:** RF-05B. **Prioridad:** Media.
* Flag `--audio-bitrate KBPS` (defecto `192`, rango `64-320`) para `-b:a`.
* **Aceptación:** `denoise boda.mp4 --audio-bitrate 128` genera `aac 128k` verificado vía `ffmpeg -hide_banner -i` (±10%, sin `ffprobe`); valor fuera de rango → error `E_INVALID_INPUT`.

### RF-06 Modelo autocontenido on-demand
* **ID:** RF-06. **Prioridad:** Alta.
* Artefacto único `dpdfnet8_48khz_hr.onnx` (14857107 B) en `--model-dir` (defecto `~/.cache/denoise/models` en Win/macOS/Linux vía `home_dir()+.cache` con `PathBuf`; sin `LOCALAPPDATA` ni `Library/Caches`) (se mantiene `~/.cache` en las 3 OS por simplicidad;: nombre canónico todo `denoise`).
* Si falta → descargar el ONNX oficial (URL canónica `https://huggingface.co/Ceva-IP/DPDFNet/resolve/main/onnx/dpdfnet8_48khz_hr.onnx` devuelve 200 con 14857107B exactos): `timeout 60s + retry 3 con backoff + chequeo espacio >=50MB libres en disco de `--model-dir` vía `sysinfo` (solo `model-dir`; `OUT/temps` sin pre-chequeo, fallo al escribir → `E_IO`)`, verificar en descarga tamaño `14857107B >=98%` + `SHA256` registrado en este documento y en `docs/design.md §7` (si el ONNX ya existe con tamaño >0B se reutiliza sin re-verificar; el mismatch es `E_MODEL_MISSING` bloqueante). HTTPS vía `reqwest 0.12 (blocking + rustls-tls-webpki-roots)` (sustituye a `rustls-tls-manual-roots`, que habilitaba TLS sin raíces de confianza y rompía la validación; `webpki-roots` = raíces Mozilla empaquetadas, sin OpenSSL del sistema, idéntico en Win/macOS/Linux). Valor `SHA256`: `7b3afbb260a08fe9af3d16e3bda992971be1e7e951d1dee7c2d235f5c43f5631` (SHA obligatorio; mismatch → `E_MODEL_MISSING`). Si la descarga falla por red/modelo (timeout, HTTP, tamaño, SHA) → `E_MODEL_MISSING` + URL + ruta manual esperada. Fallos de disco (lleno/sin permiso/sin espacio) en descarga → `E_IO`. (red→`E_MODEL_MISSING`, disco→`E_IO`). Licencia del modelo: Apache 2.0 (Ceva-IP/DPDFNet).
* **Aceptación:** primera corrida descarga una vez (~14.9MB); segunda offline funciona; corrupto/incompleto reintenta con error legible.

### RF-07 Pre-chequeos ffmpeg y audio
* **ID:** RF-07. **Prioridad:** Alta.
* Resolver ffmpeg vía `--ffmpeg-path` o `PATH`; si ausente → `E_FFMPEG_NOT_FOUND`. Requiere `ffmpeg 6+` (verificado con `ffmpeg -version` con regex `ffmpeg version (\d+)\.` major>=6 y fallback a prefijo `N-` para builds git BtbN/gyan aceptados como válidos; si tampoco matchea → `E_FFMPEG_NOT_FOUND` con línea de versión cruda en el mensaje, sin `ffprobe`.
* Receta instalación documentada en `README.md`: Win `winget install Gyan.FFmpeg` / `choco install ffmpeg`, macOS `brew install ffmpeg`, Linux `apt install ffmpeg` (verificar `ffmpeg -version` muestra `6+`).
* `probe().has_audio` (campo de `ProbeResult`) y duración vía `ffmpeg -hide_banner -i` (`Duration:` = duración del contenedor, fuente de `<dur_video>`; en contenedores `mov/mp4` se utiliza `-ignore_editlist 1` para alinear la línea de tiempo física 1:1 con el video copiado); sin audio → `E_NO_AUDIO`, sin salida.  Solo-video sin `Audio:` → `E_NO_AUDIO`; solo-audio/imagen renombrada a `.mp4`/corrupto que falla en probe → `E_INVALID_INPUT`; `E_FFMPEG_FAILED` solo si `ffmpeg` falla en `extract/remux`.
* **Aceptación:** video sin audio, ffmpeg ausente y entrada corrupta reportan códigos distintos y no dejan parciales.

### RF-08 Lote robusto, dry-run, resumen JSON/humano
* **ID:** RF-08. **Prioridad:** Media.
* Procesamiento secuencial; un fallo no aborta lote (salvo `Ctrl+C`). Resumen final `ok=N failed=N skipped=N`.
* `--dry-run`: solo lista `input → output (skip: motivo)` sin escribir/crear nada ni invocar IA/ffmpeg, en el mismo orden del lote real. Solo lectura `stat` para colisión-check; sin `ffmpeg/modelo/has_audio` (`dry-run` = expandir+resolver+colisión por lectura, cero escrituras). (exit `0` siempre que la CLI sea estructuralmente válida; errores estructurales —flags inválidos/combinados (e.g. `--output-name` con lote>1), bitrate fuera de rango, prefix/suffix inválidos, lote vacío `E_INVALID_INPUT`— fallan antes de simular con exit `2` sin reporte, igual que la corrida real; colisiones/inválidos per-archivo se reportan en `message`/`summary` con exit `0`). (el dry-run simula la intención de los flags de colisión — destino existente con `--overwrite` → `would overwrite`; sin `--overwrite` → `would fail: E_OUTPUT_EXISTS`; con `--skip-existing` → `would skip`; todo per-archivo con exit `0`). Con `--dry-run --json` emite la misma tabla en `JSONL` con `status="dry-run"` y `pct=0`, sin crear nada.
* Humano: cabecera `[i/N]`, una barra viva por video a `stderr` (`indicatif`; `ProgressDrawTarget::hidden()` solo con `--json`), línea final por video con `MB + segundos`, resumen final. Sin TTY: mismo comportamiento que con TTY (líneas por fase, sin animación real porque `indicatif` va a `stderr`). Orden estricto, flush explícito tras cada línea. Mapeo `pct`: `0 inicio/colisión-check, 1-5 extract, 6-80 denoise secuencial, 81-95 remux, 96-99 verificar+limpiar, 100 ok/failed; skipped SIEMPRE pct=0` (un archivo `skipped` nunca se procesó, consistente con `dry-run pct=0`).
* `--json`: desactiva animación; `stdout` en `JSONL` una línea por archivo `{input,output,status,message,pct}` + línea final `{summary:{ok,failed,skipped}}` (`status ∈ {ok,failed,skipped,dry-run}`, `pct 0-100` según mapeo —`skipped` → `pct=0`—, `summary` sin `pct`). Sin `--json`: salida humana a `stderr`. (`--json + --verbose` combinables —`stdout` JSONL inalterado parseable con `jq`, debug a `stderr` + conserva temps como `--verbose` solo).
* **Aceptación:** corrida real lote 5 con 1 corrupto → 4 ok + 1 failed, exit `!=0`, JSON parseable; `--dry-run` del mismo lote → exit `0` con `status="dry-run"`; en modo humano el proceso muestra avance por video sin quedarse silencioso >2s.

### RF-09 Temporales, cancelación y limpieza
* **ID:** RF-09. **Prioridad:** Media.
* Temps `<out>.tmp.in.wav` y `<out>.tmp.out.wav` junto a la salida. Limpieza garantizada en todos los caminos salvo `--verbose` debug (por defecto guardia `Drop`/limpieza explícita + borrado `.part`; con `--verbose` se conservan `.wav`/`.part` para inspeccionar).
* `Ctrl+C`: handler `ctrlc` + flag atómico → `Child::kill` al hijo `ffmpeg`/cierre de inferencia, borra parcial y temps, exit 3. 2º `Ctrl+C` fuerza salida inmediata exit 3 aunque quede limpieza. (la descarga del modelo no es cancelable cooperativamente: `reqwest blocking` no consulta el flag; latencia máx = timeout 60s; exit `3` consistente para cancelación en pipeline). Inferencia `ort` cancelable cooperativamente (latencia máxima = inferencia en curso). Hijo `ffmpeg` vía `Command::status()` bloqueante sin `creation_flags`; cancelación vía flag cooperativo `AtomicBool` + limpieza `Drop` (`TempCleaner`). Comportamiento observable único Win/POSIX. Disco lleno/sin permiso al escribir salida/temps → `E_IO`.
* **Aceptación:** interrumpir a mitad no deja `.mp4` parcial ni `.wav`.

### RF-10 Ayuda, versión y verbosidad
* **ID:** RF-10. **Prioridad:** Media.
* `--help` documenta todos los flags con ejemplos; `--version` imprime `denoise 1.0.0 + modelo DPDFNet + ffmpeg <ver>` (fuente única `Cargo.toml [package] version="1.0.0"` leída vía `env!("CARGO_PKG_VERSION")`; nombre canónico todo `denoise`); `--verbose` activa depuración detallada a `stderr` (comando `ffmpeg` exacto, `model-dir`, tiempos por fase, tamaños).
* Niveles: defecto fases, `--verbose` añade depuración a `stderr`. Sin nivel silencioso separado: `--json` ya es apto para scripting.
* **Aceptación:** `denoise --help` es suficiente para usar sin leer este doc; `--verbose` permite reproducir manualmente cada `ffmpeg`.

## 3. Requerimientos no funcionales

### RNF-01 Autocontención
* Todo vive en este repo (`Denoise/` raíz). Stack Rust: `Rust stable 1.88+ (`rust-version="1.88"`, `edition="2021"`, MSRV 1.88 exigido por `ort 2`) + ort 2 pinnado (`ort = "=2.0.0-rc.13"` con `default-features = false` y `features = ["std", "copy-dylibs", "download-binaries", "tls-rustls"]`, sin `tls-native` ni defaults; TLS 100% rustls) + rustfft + hound + clap 4 + indicatif + reqwest 0.12 (blocking + rustls-tls-webpki-roots; raíces Mozilla empaquetadas sin OpenSSL, ver RF-06) + sha2 + home + which (PATHEXT Win) + ctrlc + sysinfo + anyhow (bin) / thiserror (lib) + serde 1 (+derive) + serde_json + regex 1` + binario `ffmpeg 6+`. `Cargo.lock` versionado en git; `[profile.release] opt-level=3, strip=true`. Toolchain: `cargo + clippy + rustfmt` (detalle de features autoritativo en `Cargo.toml`). Implementación íntegra en Rust. (`reqwest blocking`, sin dependencia directa a `tokio` —`tokio` solo transitivo vía `reqwest`—;: `sysinfo` para chequeo disco ≥50MB en `model-dir`).
* Estilo: `std::path::PathBuf`, `clippy+rustfmt`, `std::process::Command` con argv sin shell, `String::from_utf8_lossy`, ASCII seguro en `pwsh`.
* **Verificación:** `cargo build --release` (con `ort download-binaries`, sin cmake ni runtime del sistema) + `./target/release/denoise --help` funciona con este repo copiado a otra máquina con toolchain Rust + `ffmpeg 6+`.

### RNF-02 Portabilidad
* `Windows 10+ / macOS 13+ / Linux x64`. Rutas con espacios y no-latinas. Hijos `ffmpeg` siempre con `String::from_utf8_lossy`. Sin `NUL` vs `/dev/null` hardcodeado.
* **Verificación:** matriz manual por OS (1 video por OS) + `cargo build --release` + `cargo test`.

### RNF-03 Rendimiento y recursos
* Memoria proporcional a la duración (un frame STFT por vez, estado recurrente encadenado); CPU 1 job secuencial; sin GPU requerida. RTF medido `<= 1.0` (0.82 en CPU de referencia, comparable a sherpa-onnx 0.84).
* No re-encode de video → tiempo dominado por DPDFNet + remux ligero. Telemetría en `--verbose` (segundos por fase).

### RNF-04 Fiabilidad numérica
* Constantes DSP (ver `docs/design.md §6`: `SR48000/FFT960/HOP480`, ventana Vorbis, padding reflect `center`, recorte de síntesis 1920 muestras; sin `wnorm`: el grafo recibe el espectro crudo como sherpa-onnx/knf). Test dorado obligatorio y bloqueante antes de release con `SI-SDR` en Rust puro en `tests/common/si_sdr.rs` (zero-mean por señal, `eps=1e-8`; cada test lo declara con `#[path = "common/si_sdr.rs"] mod si_sdr;`), y vectores deterministas en `tests/data/` versionados en git (generador `examples/gen_vectors.rs` con voz real del EvalSet de DPDFNet, ejecutado solo manualmente vía `cargo run --release --example gen_vectors -- <dir_eval>`, nunca en `cargo test`/CI; preserva el congelado). `SR 48k` mono `PCM16` en disco —dato en memoria `f32` vía `i16→f32/32768.0`, mismo camino que el pipeline real—: `voz_clean.wav` + `voz_noisy.wav` (SNR 0 dB) + `referencia_dpdfnet.wav` (generada una vez con el propio port tras validar mejora, luego congelada) + par largo 65s (`voz65s_clean.wav`, `voz65s_noisy.wav` a SNR 10 dB, `referencia65s_dpdfnet.wav`) para estabilidad en clips largos; asserts `SI-SDR(denoised,voz)-SI-SDR(mezcla,voz) >=5dB` Y `SI-SDR(denoised,referencia) >=60dB` en el par 3s y paridad `>=60dB` en el par 65s; paridad bit-exacta con sherpa-onnx (1 LSB PCM16) y media SI-SDR 14.74 dB en el EvalSet verificadas. Desviación bajo umbrales = bloqueante (estricto, 55-59dB también bloquea).

### RNF-05 Seguridad
* Sin red salvo descarga del modelo desde origen fijo único (`huggingface.co/Ceva-IP/DPDFNet`). Sin ejecución de nombres de archivo como shell (`Command` con argv, sin shell). `--output-name` fuera del cwd permitido pero se advierte si sobreescribe entrada sin `prefix/suffix`.
* Modelo verificado en descarga por tamaño `14857107B >=98%` + `SHA256 7b3afbb260a08fe9af3d16e3bda992971be1e7e951d1dee7c2d235f5c43f5631` registrado en RF-06 y en `docs/design.md §7` (si el ONNX ya existe con tamaño >0B se reutiliza sin re-verificar; SHA obligatorio en descarga; mismatch → `E_MODEL_MISSING` bloqueante para `v1.0.0`); nunca se ejecuta código descargado salvo `.onnx` vía `ort`.

### RNF-06 Usabilidad y scripting
* Mensajes de error accionables (`qué pasó + qué hacer`). Exit codes estables: `0 ok/skip, 1 ffmpeg/modelo/IO, 2 entrada/salida, 3 cancelado`. `E_IO → 1`: disco lleno, sin permiso escritura, sin espacio en descarga, `--output-name`/`--output-dir`/`--model-dir` no creables. Batch mixto con fallos `1` y `2` → exit `1` (prioridad `1>2`, `3` siempre gana si hubo cancelación). `--json` estable para `jq`. `--version` sin `ffmpeg` imprime `ffmpeg missing` en lugar de fallar. (`--dry-run` exit `0` con CLI estructuralmente válida; errores estructurales → exit `2` antes de simular).

### RNF-07 Mantenibilidad y test
* Módulos <400 líneas c/u (`df/{mod,stft,net}.rs`), funciones puras donde sea posible (`resolve_output()` en `cli.rs` vía `lib.rs` testeable sin ffmpeg). `tests/test_naming.rs` sin red ni modelo; `test_golden/test_remux` marcados `#[ignore]` (slow).
* Comandos: `cargo test` (rápido, requiere `ffmpeg 6+` real + `FakeProvider` sin red/modelo) y `cargo test -- --ignored` (dorado+e2e). Sin warnings de `clippy`.

### RNF-08 Licencias y atribución
* Código nuevo: `MIT` (`LICENSE` en raíz, `version="1.0.0"` en `Cargo.toml`). Modelo DPDFNet: `Apache 2.0 © Ceva-IP/DPDFNet` — mantener aviso y enlace en `README.md`. `ffmpeg` externo `GPL/LGPL` — no se vende empaquetado sin revisar.

### RNF-09 Documentación
* `README.md` mínimo: instalación, guía de uso con ejemplos que cubren `docs/design.md §4` (`1 archivo`, `1 archivo con prefix/suffix/bitrate`, `lote --recursive --output-dir --skip-existing --json`), limitaciones (mono, primera pista, secuencial), atribución `Apache 2.0 © Ceva-IP/DPDFNet`. `--help` idéntico al contrato `docs/design.md §4`. `CHANGELOG.md` por release con versión de `Cargo.toml` (inicial `1.0.0`). `LICENSE` MIT en raíz.

## 4. Matriz flags ↔ RF (resumen)

| Flag | RF |
|---|---|
| `INPUT..., --recursive` | RF-01, RF-02 |
| `--output-name, --output-dir, --prefix, --suffix` | RF-03 |
| `--overwrite, --skip-existing` | RF-04 |
| (pipeline interno) | RF-05 |
| `--audio-bitrate` | RF-05B |
| `--model-dir` | RF-06 |
| `--ffmpeg-path` | RF-07 |
| `--dry-run, --json, --verbose` | RF-08, RF-09, RF-10 |
| `cargo build --release` + `cargo test` | RNF-02 |

## 5. Casos borde obligatorios

1. Video vertical/teléfono, 4K, `mkv` con múltiples audios → se usa `0:v:0` + primera pista a mono vía `-map 0:a:0`; resto de pistas/subs se pierden (limitación documentada en `README.md` §Limitaciones Conocidas).
2. Nombre con espacios/acentos/emoji + ruta >150 caracteres en Win.
3. Video sin audio/solo-video → `E_NO_AUDIO`, imagen renombrada a `.mp4`/solo-audio/corrupto en probe → `E_INVALID_INPUT`, archivo `0B`, `bitrate` fuera de rango.
4. Salida en disco distinto / sin permiso escritura (`E_IO`) / `--output-name`, `--output-dir`, `--model-dir` a carpeta inexistente (debe crearla, si no creable → `E_IO`) vs `--output-name` con lote expandido `>1` (debe fallar `E_INVALID_INPUT`) o `--output-name` resolviendo a la propia entrada sin `prefix/suffix` efectivo (debe fallar `E_INVALID_INPUT` siempre, incluso con `--overwrite`). (`--output-name` sin `.mp4` auto-añade, no falla).
5. Lote con mezcla de ok + sin-audio + colisión existente + 1 ruta inexistente.
6. Corte `Ctrl+C` durante `extract`, durante `inferencia`, durante `remux` → exit 3, sin `.part` ni `.wav`.
7. Duración contenedor vs audio stream: si la pista de audio dura más que el video (duración del contenedor `Duration:` del probe), la salida se alarga a la duración del contenedor (el video termina antes / último frame extendido). Sin decode extra del stream de video en v1. Este comportamiento se documenta como limitación v1 y se verifica en `test_remux` con fixture donde audio y video tienen duraciones diferentes.

## 6. Criterios de aceptación de entrega v1 (DoD de ejecución — y verificado)

* [x] `denoise 1.mp4` y `denoise dir/ --recursive --output-dir out/ --suffix _clean` funcionan offline tras primera descarga.
* [x] `cargo test` en verde + `cargo test -- --ignored` (`test_golden/test_remux`) en verde en 1 máquina Win.
* [x] Corrida real `--json` en lote 5 con 1 fallo inducido → exit `!=0` y JSON parseable; `--dry-run` del mismo lote → exit `0`; progreso humano muestra `[i/N]` por video.
* [x] Repo copiado a otra máquina compila directamente con `cargo build --release`.
* [x] `cargo build --release` + `cargo test` (sin `--ignored`) en verde en Win.
* [x] Fixture manual `assets/e2e_vertical_1080x1920_16s.mp4` (vertical 1080x1920, 16s, con audio): `--dry-run` primero, luego corrida real.
* [x] Este doc + `docs/design.md` con contrato idéntico y sin flags fuera de lista.

### Fórmula SI-SDR (para `test_golden`, RNF-04)

El criterio de aceptación del DSP (`test_golden`) usa **SI-SDR** (Scale-Invariant Signal-to-Distortion Ratio) como métrica de referencia. La fórmula es:

```
SI-SDR(x, x̂) = 10 · log10(||x · ŝ||² / ||x - ŝ||² )
```

donde `x` = señal limpia de referencia, `x̂` = señal denoizada del modelo, `ŝ = (x·x̂ / ||x||²) · x` = proyección escalada de `x̂` sobre `x` (normalización de escala invariante). El test golden (`tests/data/`, voz real del EvalSet DPDFNet: par 3s a `SNR 0dB` + par 65s a `SNR 10dB`, `SR 48k` mono `PCM16`) exige:

* **Mejora SI-SDR ≥ 5 dB** respecto a la entrada con ruido en el par 3s (bloqueante, RNF-04).
* **Paridad SI-SDR ≥ 60 dB** vs referencia congelada generada con el propio port (bloqueante, RNF-04).

Si el DSP difiere numéricamente de la referencia → `test_golden` falla. La fórmula se implementa en `tests/common/si_sdr.rs`.

## 7. Alcance

* Implementación íntegra en Rust según `RNF-01`. Contrato CLI (`docs/design.md §4`), errores, mapeo `pct` y umbrales de `SI-SDR` mandan sin cambios.
