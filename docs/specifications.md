# Especificaciones — CLI `denoise` v1 Rust (autocontenida en este repo)

> Este documento junto a `docs/design.md` forma el contrato canónico.
> Convención de rutas: `docs/<fichero>` es relativo a la raíz del repo.
> Idioma CLI y mensajes: inglés técnico para flags/ayuda, este documento en español.
> Convención: `INPUT...` = 1..N rutas; `stem` = nombre sin extensión; salida siempre `.mp4`.
> Registro de numeración de decisiones: D1–D45 + D_a–D_j (ciclo 2026-09-13); D43 reservada/absorbida por D45, sin contenido propio (D_g cerrado 2026-09-13, ver `docs/design.md` encabezado).

## 1. Resumen

CLI offline-first que limpia ruido de 1..N videos, parametrizando entradas, salida, `prefix/suffix`, `audio-bitrate` y lote. Entrega `.mp4` con video idéntico y audio `AAC` limpio al bitrate pedido.

## 2. Requerimientos funcionales

### RF-01 Entradas individuales y múltiples
* **ID:** RF-01. **Prioridad:** Alta.
* Aceptar `INPUT...` con 1..N rutas de archivo `mp4/mov/mkv/webm/avi` (insensible a mayúsculas).
* Rechazar extensiones no soportadas y rutas inexistentes con error `E_INVALID_INPUT`. Directorio expandido sin videos → `E_INVALID_INPUT` lote vacío; duplicados por absoluto normalizado lexical (D28 cerrado 2026-09-13: absolutizar contra cwd + normalizar `./`/`../` sin tocar disco ni resolver symlinks).
* **Aceptación:** `denoise a.mp4 b.mov` produce 2 salidas; con 1 ruta mala, procesa las válidas y reporta la mala sin abortar lote.

### RF-02 Entrada por directorio y recursividad
* **ID:** RF-02. **Prioridad:** Alta.
* Si `INPUT` es directorio: expande a videos `mp4/mov/mkv/webm/avi` según `--recursive`.
* Orden determinista byte-wise UTF-8 del path absoluto (D21 cerrado). Sin duplicados tras expandir (absoluto lexical D28, case-insensitive solo en Win vía `to_lowercase`).
* Si `--output-dir` está dentro de un `INPUT` directorio con `--recursive`, sus contenidos se excluyen del escaneo y se avisa en `-v`; evita loop infinito en re-corridas.
* (D4 cerrado) En escaneo con `--recursive` se excluyen además los ficheros que ya terminan en `<suffix>.mp4` vigente (por defecto `*_denoised.mp4`) y se avisa en `-v`; evita generar `*_denoised_denoised.mp4` en re-corridas in-place.
* **Aceptación:** `denoise ./crudos/ --recursive` procesa incluyendo subcarpetas, en orden estable.

### RF-03 Salida parametrizable: archivo, directorio, prefijo, sufijo
* **ID:** RF-03. **Prioridad:** Alta.
* Flags: `-o/--output OUT` (solo si lote expandido ==1 video, sea `INPUT` archivo o directorio con 1 video; D20 cerrado: se permite `-o` con directorio si expande a 1 video), `--output-dir DIR`, `--prefix STR`, `--suffix STR (defecto `_denoised`)`. (D12 cerrado 2026-09-13: `-o/--output` y `--output-dir` mutuamente excluyentes; combinación → `E_INVALID_INPUT`). `-o` con lote expandido >1 → `E_INVALID_INPUT`. (D35 cerrado 2026-09-13, revisado: si `-o/--output` no termina en `.mp4` case-insensitive se añade `.mp4` automáticamente; ej. `-o final` → `final.mp4`, `-o final.mkv` → `final.mkv.mp4`; coherente con `salida siempre .mp4`).
* Precedencia: `1) -o/--output exacto con auto-.mp4 D35 (si lote==1) > 2) --output-dir/<prefix><stem><suffix>.mp4 > 3) junto a original` (alternativas excluyentes por D12, no acumulables; D20: `N` = videos tras expandir). (D_e cerrado 2026-09-13, opción C: si 2+ entradas del lote resuelven al MISMO path de salida —p.ej. `a.mp4`+`a.mov`→`a_denoised.mp4`— se aplica auto-sufijo incremental al 2º y siguientes: `a_denoised_1.mp4`, `a_denoised_2.mp4`..., con aviso SIEMPRE a `stderr`/`message` no solo `-v`; el reporte lista cada salida real).
* (D7 cerrado) Se crean directorios padre automáticamente para `-o/--output`, `--output-dir` y `--model-dir`; si no creables → `E_IO`.
* `--recursive` + `--output-dir` recrea árbol relativo a cwd (D27 cerrado 2026-09-13, opción A): `DIR/<ruta-dada-relativa-a-cwd>/<prefix><stem><suffix>.mp4`; ej. `dirA/sub/x.mp4 → out/dirA/sub/x_denoised.mp4`; archivo suelto `a.mp4 → out/a_denoised.mp4`; absoluto fuera de cwd → solo `stem` + aviso `-v`. La exclusión aplica si `DIR` está dentro de `INPUT`.
* `prefix/suffix` solo caracteres `[A-Za-z0-9._-]`, prohibidos `.` y `..` exactos (D33 cerrado 2026-09-13); vacío permitido para uno de los dos, no ambos vacíos si salida es mismo directorio que entrada (evita sobrescribirse). Remux vía `OUT.part.mp4` + rename atómico: `std::fs::rename` reemplaza atómicamente el destino tanto en POSIX como en Win (`MoveFileExW` + `REPLACE_EXISTING`, D_j cerrado 2026-09-13: corrige la afirmación previa «Win sin `--overwrite` falla si existe», que era fácticamente incorrecta; la garantía de no-sobrescritura descansa EXCLUSIVAMENTE en el colisión-check previo de RF-04 `E_OUTPUT_EXISTS`/`skipped`). I/O WAV con `hound` PCM16 + `f32` (`i16→f32 /32768.0` y vuelta con clip); `ffmpeg` produce/consume `PCM16 48k mono`.
* `--audio-bitrate` no afecta al nombre (ver RF-05B). `-o/--output` que resuelve a la propia entrada sin `prefix/suffix` efectivo emite warning a `stderr` aunque `--overwrite` lo permita (D33 cerrado 2026-09-13: warning condicionado).
* **Aceptación:**
  * `denoise boda.mp4` → `boda_denoised.mp4`
  * `denoise boda.mp4 --prefix pod- --suffix _clean` → `pod-boda_clean.mp4`
  * `denoise a.mp4 -o limpio/final.mp4` → ese path exacto, creando `limpio/`.

### RF-04 No sobrescribir por defecto
* **ID:** RF-04. **Prioridad:** Alta.
* Sin `--overwrite`, si destino existe → error `E_OUTPUT_EXISTS`. Con `--skip-existing` → marca `skipped`, exit 0 si todo lo demás ok.
* `--overwrite` y `--skip-existing` mutuamente excluyentes.
* **Aceptación:** segunda corrida sin flags falla con mensaje claro; con `--skip-existing` la salta; con `--overwrite` la reemplaza.

### RF-05 Denoise de pista de audio (única transformación IA)
* **ID:** RF-05. **Prioridad:** Alta.
* Extraer con `ffmpeg -vn -map 0:a:0 -ac 1 -ar 48000` (D3 cerrado: `-map 0:a:0` obligatorio para fijar primera pista en multi-audio), inferir DeepFilterNet3 3-grafos en CPU, remezclar.
* Video: `-map 0:v:0 -map 1:a:0 -c:v copy -c:a aac -b:a <audio-bitrate>k -t <dur_video>` (D_d cerrado 2026-09-13, opción A: sustituye a `-shortest`; `<dur_video>` = duración del stream de video del probe, la salida conserva SIEMPRE la duración del video —si el audio limpio es más corto, la cola queda muda—; coherente con verificación ligera `±0.5s`).
* **Aceptación:** salida conserva `codec/res/fps` originales, duración `±0.2s`, audio `aac 48k` al bitrate pedido; mejora audible/SI-SDR en test dorado. Verificación runtime ligera: `OUT existe >0B + duración ±0.5s vía ffmpeg -i + presencia stream Audio AAC`; verificación estricta `±0.2s/bitrate±10%` solo en `test_remux`.

### RF-05B Bitrate de audio parametrizable
* **ID:** RF-05B. **Prioridad:** Media.
* Flag `--audio-bitrate KBPS` (defecto `192`, rango `64-320`) para `-b:a`.
* **Aceptación:** `denoise boda.mp4 --audio-bitrate 128` genera `aac 128k` verificado vía `ffmpeg -hide_banner -i` (±10%, sin `ffprobe`); valor fuera de rango → error `E_INVALID_INPUT`.

### RF-06 Modelo autocontenido on-demand
* **ID:** RF-06. **Prioridad:** Alta.
* Artefactos `dfn3_enc.onnx, dfn3_erb_dec.onnx, dfn3_df_dec.onnx` en `--model-dir` (defecto `~/.cache/denoise/models` en Win/macOS/Linux vía `home_dir()+.cache` con `PathBuf`; sin `LOCALAPPDATA` ni `Library/Caches`) (D8 cerrado: se mantiene `~/.cache` en las 3 OS por simplicidad; D16 cerrado: nombre canónico todo `denoise`).
* Si faltan → descargar tarball oficial `v0.5.6` (D2 cerrado-verificado 2026-09-13 en `docs/plan.md T2.0`: URL canónica `https://github.com/Rikorose/DeepFilterNet/raw/v0.5.6/models/DeepFilterNet3_onnx.tar.gz` devuelve 200 con 7983136B exactos y miembros `tmp/export/{enc,erb_dec,df_dec}.onnx` (+`config.ini` ignorado); no se canoniza `releases/download`): `timeout 30s + retry 3 con backoff + chequeo espacio >=50MB libres en disco de `--model-dir` vía `sysinfo` (D17 + D29 cerrado 2026-09-13: solo `model-dir`; `OUT/temps` sin pre-chequeo, fallo al escribir → `E_IO`) + anti tar-slip (allowlist `tmp/export/{enc,erb_dec,df_dec}.onnx`)`, verificar tamaño `7983136B >=98%` + `SHA256` registrado en este documento y en `docs/design.md §7` (el mismatch es `E_MODEL_MISSING` bloqueante), extraer, borrar `.tar.gz`. Descarga HTTPS vía `reqwest 0.12 (blocking + rustls-tls-webpki-roots)` (D_a cerrado 2026-09-13: sustituye a `rustls-tls-manual-roots`, que habilitaba TLS sin raíces de confianza y rompía la validación hacia `github.com`; `webpki-roots` = raíces Mozilla empaquetadas, sin OpenSSL del sistema, idéntico en Win/macOS/Linux). Valor `SHA256`: `C94D91F70911001C946E0FABB4AA9ADC37045F45A03B56008CB0C8244CB63616` (verificado 2026-09-13 vía `Invoke-WebRequest` + `Get-FileHash`/`tar -tzf`; SHA obligatorio desde ahora, sin tag `v1.0.0` sin él — D1 cerrado definitivo, fin de la política interina de solo-tamaño). Si la descarga falla por red/modelo (timeout, HTTP, tamaño, SHA, tar-slip) → `E_MODEL_MISSING` + URL + ruta manual esperada. Fallos de disco (lleno/sin permiso/sin espacio) en descarga → `E_IO`. (D24 cerrado 2026-09-13: red→`E_MODEL_MISSING`, disco→`E_IO`).
* **Aceptación:** primera corrida descarga una vez (~8MB); segunda offline funciona; corrupto/incompleto reintenta con error legible.

### RF-07 Pre-chequeos ffmpeg y audio
* **ID:** RF-07. **Prioridad:** Alta.
* Resolver ffmpeg vía `--ffmpeg-path` o `PATH`; si ausente → `E_FFMPEG_NOT_FOUND`. Requiere `ffmpeg 6+` (verificado con `ffmpeg -version` con regex `ffmpeg version (\d+)\.` major>=6 y fallback a prefijo `N-` para builds git BtbN/gyan aceptados como válidos; si tampoco matchea → `E_FFMPEG_NOT_FOUND` con línea de versión cruda en el mensaje, D_c cerrado 2026-09-13), sin `ffprobe`.
* Receta instalación documentada en `README.md`: Win `winget install Gyan.FFmpeg` / `choco install ffmpeg`, macOS `brew install ffmpeg`, Linux `apt install ffmpeg` (verificar `ffmpeg -version` muestra `6+`).
* `has_audio` y duración vía `ffmpeg -hide_banner -i`; sin audio → `E_NO_AUDIO`, sin salida. (D6 cerrado) Solo-video sin `Audio:` → `E_NO_AUDIO`; solo-audio/imagen renombrada a `.mp4`/corrupto que falla en probe → `E_INVALID_INPUT`; `E_FFMPEG_FAILED` solo si `ffmpeg` falla en `extract/remux`.
* **Aceptación:** video sin audio, ffmpeg ausente y entrada corrupta reportan códigos distintos y no dejan parciales.

### RF-08 Lote robusto, dry-run, resumen JSON/humano
* **ID:** RF-08. **Prioridad:** Media.
* Procesamiento secuencial; un fallo no aborta lote (salvo `Ctrl+C`). Resumen final `ok=N failed=N skipped=N`.
* `--dry-run`: solo lista `input → output` sin escribir/crear nada ni invocar IA/ffmpeg, en el mismo orden del lote real. Solo lectura `stat` para colisión-check; sin `ffmpeg/modelo/has_audio` (D23 cerrado + D25 cerrado 2026-09-13: `dry-run` = expandir+resolver+colisión por lectura, cero escrituras). (D5 cerrado 2026-09-13, refinado por D_f cerrado 2026-09-13, opción A: exit `0` SIEMPRE que la CLI sea estructuralmente válida; errores estructurales —D12 `-o`+`--output-dir`, D20 `-o` con lote>1, `bitrate` fuera de rango, `prefix/suffix` inválidos D33, lote vacío `E_INVALID_INPUT`— fallan ANTES de simular con exit `2` sin reporte, igual que la corrida real; colisiones/inválidos per-archivo se reportan en `message`/`summary` con exit `0`). Con `--dry-run --json` emite la misma tabla en `JSONL` con `status="dry-run"` y `pct=0`, sin crear nada.
* Humano: cabecera `[i/N]`, una barra viva por video a `stderr` (`indicatif`; `ProgressDrawTarget::hidden()` con `--json` o sin TTY), línea final por video con `MB + segundos`, resumen final. Sin TTY: líneas de porcentaje sin animación. Orden estricto, flush explícito tras cada línea. Mapeo `pct`: `0 inicio/colisión-check, 1-5 extract, 6-80 denoise por chunks (d/t), 81-95 remux, 96-99 verificar+limpiar, 100 ok/failed/skipped`.
* `--json`: desactiva animación; `stdout` en `JSONL` una línea por archivo `{input,output,status,message,pct}` + línea final `{summary:{ok,failed,skipped}}` (`status ∈ {ok,failed,skipped,dry-run}`, `pct 0-100` según mapeo, `summary` sin `pct`). Sin `--json`: salida humana a `stderr`. (D38 cerrado 2026-09-13, opción A: `--json + -v` combinables —`stdout` JSONL inalterado parseable con `jq`, debug `log` a `stderr` + conserva temps como `-v` solo).
* **Aceptación:** corrida real lote 5 con 1 corrupto → 4 ok + 1 failed, exit `!=0`, JSON parseable; `--dry-run` del mismo lote → exit `0` con `status="dry-run"`; en modo humano el proceso muestra avance por video sin quedarse silencioso >2s.

### RF-09 Temporales, cancelación y limpieza
* **ID:** RF-09. **Prioridad:** Media.
* Temps `<out>.tmp.in.wav` y `<out>.tmp.out.wav` junto a la salida. Limpieza garantizada en todos los caminos salvo `-v` debug (D31 cerrado 2026-09-13: por defecto guardia `Drop`/limpieza explícita + borrado `.part`; con `-v` se conservan `.wav`/`.part` para inspeccionar).
* `Ctrl+C`: handler `ctrlc` + flag atómico → `Child::kill` al hijo `ffmpeg`/cierre de inferencia, borra parcial y temps, exit 3. 2º `Ctrl+C` fuerza salida inmediata exit 3 aunque quede limpieza. (D_i cerrado 2026-09-13, opción A: la cancelación TAMBIÉN aplica durante la descarga del modelo — el flag atómico se consulta entre retries y la guardia `Drop` de `models.rs` borra el `.part`; `reqwest blocking` no aborta el request en curso, latencia máx = ventana timeout `30s`; exit `3` consistente). Inferencia `ort` cancelable cooperativamente entre chunks (latencia máx 1 chunk en curso). En Win crear hijo con `CREATE_NEW_PROCESS_GROUP` para terminación limpia; en POSIX grupo por defecto. Comportamiento observable único Win/POSIX. Disco lleno/sin permiso al escribir salida/temps → `E_IO`.
* **Aceptación:** interrumpir a mitad no deja `.mp4` parcial ni `.wav`.

### RF-10 Ayuda, versión y verbosidad
* **ID:** RF-10. **Prioridad:** Media.
* `--help` documenta todos los flags con ejemplos; `--version` imprime `denoise 1.0.0 + modelo DFN3 v0.5.6 + ffmpeg <ver>` (fuente única `Cargo.toml [package] version="1.0.0"` leída vía `env!("CARGO_PKG_VERSION")`; D16 cerrado: nombre canónico todo `denoise`); `-v` debug (comando `ffmpeg` exacto, `model-dir`, tiempos por fase, `chunks d/t`, tamaños).
* Niveles: defecto `info` (fases), `-v` `debug` vía `log + env_logger`. Sin nivel silencioso separado: `--json` ya es apto para scripting.
* **Aceptación:** `denoise --help` es suficiente para usar sin leer este doc; `-v` permite reproducir manualmente cada `ffmpeg`.

## 3. Requerimientos no funcionales

### RNF-01 Autocontención
* Todo vive en este repo (`Video-Noise-Remover/` raíz). Sin runtime Python. Stack Rust: `Rust stable 1.88+ (`rust-version="1.88"`, `edition="2021"`, MSRV 1.88 exigido por `ort 2`) + ort 2 pinnado (D36 cerrado 2026-09-13, opción A: `ort = "=2.0.0-rc.13"` con `default-features = false` y `features = ["std", "ndarray", "copy-dylibs", "download-binaries", "tls-rustls"]`, sin `tls-native` ni defaults (D36 cerrado 2026-09-13 + D_b cerrado 2026-09-13: los defaults de `ort` rc.13 incluyen `tls-native` (OpenSSL del sistema); se desactivan con `default-features=false` y se re-declaran `std/ndarray/copy-dylibs` explícitos; TLS 100% rustls, coherente con `reqwest webpki-roots` D_a) (D42 cerrado 2026-09-13: verificado en `crates.io` que `rc.13` es el RC vigente —publicado 2026-07-28, no yanked, sin `2.0.0` estable—; pin fijo hasta `v1.0.0`, se elimina la cláusula «actualizar al RC vigente»)) + ndarray + rustfft + hound + clap 4 + indicatif + reqwest 0.12 (blocking + rustls-tls-webpki-roots; D_a cerrado 2026-09-13: sustituye `rustls-tls-manual-roots` —TLS sin raíces de confianza, rompía github.com— por raíces Mozilla empaquetadas sin OpenSSL, ver RF-06) + sha2 + flate2 + tar + home + which (PATHEXT Win) + ctrlc + sysinfo + anyhow (bin) / thiserror (lib) + serde 1 (+derive) + serde_json + log + env_logger + regex 1` + `rand 0.8` solo `dev-dependency` para fixtures + binario `ffmpeg 6+`. `Cargo.lock` versionado en git; `[profile.release] opt-level=3, strip=true`. Toolchain: `cargo + clippy + rustfmt` (ver `docs/plan.md T0.3`, detalle de features autoritativo). Implementación íntegra en Rust. (D15 + D32 cerrado 2026-09-13: `reqwest blocking`, sin dependencia directa a `tokio` —`tokio` solo transitivo vía `reqwest`—; D17 cerrado: `sysinfo` para chequeo disco ≥50MB en `model-dir`).
* Estilo: `std::path::PathBuf`, `clippy+rustfmt`, `std::process::Command` con argv sin shell, `String::from_utf8_lossy`, ASCII seguro en `pwsh`.
* **Verificación:** `cargo build --release` (con `ort download-binaries`, sin cmake ni runtime del sistema) + `./target/release/denoise --help` funciona con este repo copiado a otra máquina con toolchain Rust + `ffmpeg 6+`.

### RNF-02 Portabilidad
* `Windows 10+ / macOS 13+ / Linux x64`. Rutas con espacios y no-latinas. Hijos `ffmpeg` siempre con `String::from_utf8_lossy`. Sin `NUL` vs `/dev/null` hardcodeado.
* **Verificación:** matriz manual 1 video corto por OS (D19 cerrado: CI solo Win suficiente v1, portabilidad 3 OS manual).

### RNF-03 Rendimiento y recursos
* Memoria constante por chunks `60s/1s overlap` (pico `~300MB` más `ffmpeg`); CPU 1 job secuencial; sin GPU requerida.
* No re-encode de video → tiempo dominado por DFN3 + remux ligero. Telemetría en `-v` (segundos por fase), sin objetivo contractual.

### RNF-04 Fiabilidad numérica
* Constantes DSP (ver `docs/design.md §6`: `SR48000/FFT960/HOP480/ERB32/DF96/ORDER5/LOOKAHEAD2/ALPHA0.99/LSNR-15/35/20` (D37 cerrado 2026-09-13, opción B)). Test dorado obligatorio y bloqueante antes de release con `SI-SDR` en Rust puro en `tests/common/si_sdr.rs` (zero-mean por señal, `eps=1e-8`, `10*log10(||s_target||²/||e||²)`, declarado en cada test vía `#[path = "common/si_sdr.rs"] mod si_sdr;` — D_h cerrado 2026-09-13) y vectores deterministas en `tests/data/` versionados en git (generador `examples/gen_vectors.rs` con `rand StdRng seed 0/1` + Box-Muller manual, ejecutado solo manualmente vía `cargo run --example gen_vectors`, nunca en `cargo test`/CI — D39 cerrado 2026-09-13, opción A; preserva el congelado D14), seno `440Hz 3s` + ruido blanco `SNR 10dB`, `SR 48k` mono `f32`: `voz.wav`, `mezcla10dB.wav`, `referencia_dfn3.wav` D14: generada una vez con el propio port tras validar mejora, luego congelada —D37 invalida refs previas: regenerar una única vez con nuevos umbrales y recongelar—; + vectores largos `voz65s.wav`, `mezcla65s10dB.wav`, `referencia65s_dfn3.wav` (mismo generador `seed 1`, `440Hz 65s` para ejercitar 2 chunks + crossfade 1s); asserts `SI-SDR(denoised,voz)-SI-SDR(mezcla,voz) >=5dB` Y `SI-SDR(denoised,referencia) >=60dB` en ambos pares (3s y 65s); valores `20.8dB` pipeline oficial / `~77dB` paridad bit-exacta solo informativos). Desviación bajo umbrales = bloqueante (D9 cerrado + D30 cerrado 2026-09-13: estricto, 55-59dB también bloquea e impone investigar ventana/`WNORM` sin relajar spec).

### RNF-05 Seguridad
* Sin red salvo descarga modelo desde origen fijo único (`github.com/Rikorose/DeepFilterNet v0.5.6`, sin HuggingFace en v1). Sin ejecución de nombres de archivo como shell (`Command` con argv, sin shell). `-o` fuera del cwd permitido pero se advierte si sobreescribe entrada sin `prefix/suffix`.
* Modelos verificados por tamaño `7983136B >=98%` + `SHA256 C94D91F70911001C946E0FABB4AA9ADC37045F45A03B56008CB0C8244CB63616` registrado en RF-06 y en `docs/design.md §7` (D1 cerrado definitivo 2026-09-13: SHA obligatorio; mismatch → `E_MODEL_MISSING` bloqueante para `v1.0.0`); nunca se ejecuta código descargado salvo `.onnx` vía `ort`.

### RNF-06 Usabilidad y scripting
* Mensajes de error accionables (`qué pasó + qué hacer`). Exit codes estables: `0 ok/skip, 1 ffmpeg/modelo/IO, 2 entrada/salida, 3 cancelado`. `E_IO → 1`: disco lleno, sin permiso escritura, sin espacio en descarga, `-o/--output`/`--output-dir`/`--model-dir` no creables. Batch mixto con fallos `1` y `2` → exit `1` (prioridad `1>2`, `3` siempre gana si hubo cancelación). `--json` estable para `jq`. `--version` sin `ffmpeg` imprime `ffmpeg missing` en lugar de fallar. (D5 cerrado, refinado por D_f: `--dry-run` exit `0` con CLI estructuralmente válida; errores estructurales → exit `2` antes de simular).

### RNF-07 Mantenibilidad y test
* Módulos <300 líneas c/u (D40 cerrado 2026-09-13, opción A: `df.rs` se divide en submódulos `df/{mod,stft,erb,net,overlap}.rs`, cada uno <300), funciones puras donde sea posible (`resolve_output()` en `cli.rs` vía `lib.rs` testeable sin ffmpeg). `tests/test_naming.rs` sin red ni modelo; `test_golden/test_remux` marcados `#[ignore]` (slow).
* Comandos: `cargo test` (rápido, requiere `ffmpeg 6+` real + `FakeProvider` sin red/modelo; D22) y `cargo test -- --ignored` (dorado+e2e). Sin warnings de `clippy`.

### RNF-08 Licencias y atribución
* Código nuevo: `MIT` (`LICENSE` en raíz, `version="1.0.0"` en `Cargo.toml`). Modelo DeepFilterNet3: `MIT © Rikorose/DeepFilterNet` — mantener aviso y enlace en `README.md` y `--version`. `ffmpeg` externo `GPL/LGPL` — no se vende empaquetado sin revisar.

### RNF-09 Documentación
* `README.md` mínimo: instalación, 3 ejemplos idénticos a `docs/design.md §4` (`1 archivo`, `1 archivo con prefix/suffix/bitrate`, `lote --recursive --output-dir --skip-existing --json`), limitaciones (mono, primera pista, secuencial), atribución `MIT © Rikorose/DeepFilterNet`. `--help` idéntico al contrato `docs/design.md §4`. `CHANGELOG.md` por release con versión de `Cargo.toml` (inicial `1.0.0`). `LICENSE` MIT en raíz.

## 4. Matriz flags ↔ RF (resumen)

| Flag | RF |
|---|---|
| `INPUT..., --recursive` | RF-01, RF-02 |
| `-o/--output, --output-dir, --prefix, --suffix` | RF-03 |
| `--overwrite, --skip-existing` | RF-04 |
| (pipeline interno) | RF-05 |
| `--audio-bitrate` | RF-05B |
| `--model-dir` | RF-06 |
| `--ffmpeg-path` | RF-07 |
| `--dry-run, --json, -v` | RF-08, RF-09, RF-10 |

## 5. Casos borde obligatorios

1. Video vertical/teléfono, 4K, `mkv` con múltiples audios → se usa `0:v:0` + primera pista a mono vía `-map 0:a:0` (D3); resto de pistas/subs se pierden (limitación documentada en `--help` y `README.md`).
2. Nombre con espacios/acentos/emoji + ruta >150 caracteres en Win.
3. Video sin audio/solo-video → `E_NO_AUDIO`, imagen renombrada a `.mp4`/solo-audio/corrupto en probe → `E_INVALID_INPUT` (D6), archivo `0B`, `bitrate` fuera de rango.
4. Salida en disco distinto / sin permiso escritura (`E_IO`) / `-o/--output`, `--output-dir`, `--model-dir` a carpeta inexistente (debe crearla, si no creable → `E_IO`, D7) vs `-o` con lote expandido `>1` o `-o/--output` + `--output-dir` juntos (debe fallar `E_INVALID_INPUT`, D12+D20). (D35: `-o` sin `.mp4` auto-añade, no falla).
5. Lote con mezcla de ok + sin-audio + colisión existente + 1 ruta inexistente.
6. Corte `Ctrl+C` durante `extract`, durante `inferencia chunk 3/7`, durante `remux` → exit 3, sin `.part` ni `.wav`.

## 6. Criterios de aceptación de entrega v1 (DoD de ejecución — no bloquean cierre spec/plan, D34 cerrado 2026-09-13)

* [ ] `denoise 1.mp4` y `denoise dir/ --recursive --output-dir out/ --suffix _clean` funcionan offline tras primera descarga.
* [ ] `cargo test` en verde + `cargo test -- --ignored` (`test_golden/test_remux`) en verde en 1 máquina Win.
* [ ] Corrida real `--json` en lote 5 con 1 fallo inducido → exit `!=0` y JSON parseable; `--dry-run` del mismo lote → exit `0`; progreso humano muestra `[i/N]` por video.
* [ ] Repo copiado a otra máquina compila con `cargo build --release` sin dependencias Python.
* [ ] Este doc + `docs/design.md` con contrato idéntico y sin flags fuera de lista.

## 7. Alcance

* Implementación íntegra en Rust según `RNF-01`. Contrato CLI (`docs/design.md §4`), errores, mapeo `pct` y umbrales de `SI-SDR` mandan sin cambios.
