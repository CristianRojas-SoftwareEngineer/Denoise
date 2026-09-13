# Especificaciones — CLI `denoise-videos` v1 (autocontenida en este repo)

> Fuente lógica: pipeline `extract mono 48k → DeepFilterNet3 → remux copy` del proyecto original (`../NextgenUp/`).
> Esta CLI es nueva implementación dedicada y autocontenida en este repo (`Video-Noise-Remover/` raíz). En runtime nada importa de fuera: el código original (`../NextgenUp/app.py`, `../NextgenUp/audio_engine.py`, `../NextgenUp/video_tools.py`, `../NextgenUp/model_store.py`, `../NextgenUp/paths.py`) solo sirve como referencia de lectura para portar/reutilizar la lógica cuando corresponda, sin importarlo.
> Este documento es autocontenido junto a `nuevo-diseño.md` en la raíz del repo; ambos se referencian entre sí como contrato canónico.
> Idioma CLI y mensajes: inglés técnico para flags/ayuda, este documento en español.
> Convención: `INPUT...` = 1..N rutas; `stem` = nombre sin extensión; salida siempre `.mp4`.

## 1. Resumen

CLI offline-first que limpia ruido de 1..N videos, parametrizando entradas, salida, `prefix/suffix`, `audio-bitrate` y lote. Entrega `.mp4` con video idéntico y audio `AAC` limpio al bitrate pedido.

## 2. Requerimientos funcionales

### RF-01 Entradas individuales y múltiples
* **ID:** RF-01. **Prioridad:** Alta.
* Aceptar `INPUT...` con 1..N rutas de archivo `mp4/mov/mkv/webm/avi` (insensible a mayúsculas).
* Rechazar extensiones no soportadas y rutas inexistentes con error `E_INVALID_INPUT`. D9: directorio expandido sin videos → `E_INVALID_INPUT` lote vacío; duplicados por absoluto normalizado.
* **Aceptación:** `denoise a.mp4 b.mov` produce 2 salidas; con 1 ruta mala, procesa las válidas y reporta la mala sin abortar lote.

### RF-02 Entrada por directorio y recursividad
* **ID:** RF-02. **Prioridad:** Alta.
* Si `INPUT` es directorio: expande a videos `mp4/mov/mkv/webm/avi` según `--recursive`.
* Orden determinista (alfabético). Sin duplicados tras expandir (por path absoluto normalizado, case-insensitive en Win).
* D6: si `--output-dir` está dentro de un `INPUT` directorio con `--recursive`, sus contenidos se excluyen del escaneo y se avisa en `-v`; evita loop infinito en re-corridas.
* **Aceptación:** `denoise ./crudos/ --recursive` procesa incluyendo subcarpetas, en orden estable.

### RF-03 Salida parametrizable: archivo, directorio, prefijo, sufijo
* **ID:** RF-03. **Prioridad:** Alta.
* Flags: `-o/--output OUT` (solo si N==1 y entrada es archivo), `--output-dir DIR`, `--prefix STR`, `--suffix STR (defecto `_denoised`)`.
* Precedencia: `1) -o exacto > 2) --output-dir/<prefix><stem><suffix>.mp4 > 3) junto a original`.
* `--recursive` + `--output-dir` recrea árbol relativo. D6 aplica exclusión si `DIR` está dentro de `INPUT`.
* `prefix/suffix` solo caracteres `[A-Za-z0-9._-]`; vacío permitido para uno de los dos, no ambos vacíos si salida es mismo directorio que entrada (evita sobrescribirse). Regla congelada Ronda 4. Remux vía `OUT.part.mp4` + rename atómico `os.replace` (con `--overwrite` reemplaza destino). D7: I/O WAV solo `stdlib wave` PCM16 + `numpy` (`int16→float32 /32768` y vuelta con clip), sin `soundfile/scipy`; `ffmpeg` produce/consume `PCM16 48k mono`.
* `--audio-bitrate` no afecta al nombre (ver RF-05B). D9: `-o` que resuelve a la propia entrada sin `prefix/suffix` efectivo emite warning a `stderr` aunque `--overwrite` lo permita.
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
* Extraer con `ffmpeg -vn -ac 1 -ar 48000`, inferir DeepFilterNet3 3-grafos en CPU, remezclar.
* Video: `-map 0:v:0 -map 1:a:0 -c:v copy -c:a aac -b:a <audio-bitrate>k -shortest`.
* **Aceptación:** salida conserva `codec/res/fps` originales, duración `±0.2s`, audio `aac 48k` al bitrate pedido; mejora audible/SI-SDR en test dorado. Verificación runtime D5 (ligera): `OUT existe >0B + duración ±0.5s vía ffmpeg -i + presencia stream Audio AAC`; verificación estricta `±0.2s/bitrate±10%` solo en `test_remux`.

### RF-05B Bitrate de audio parametrizable
* **ID:** RF-05B. **Prioridad:** Media.
* Flag `--audio-bitrate KBPS` (defecto `192`, rango `64-320`) para `-b:a`.
* **Aceptación:** `denoise boda.mp4 --audio-bitrate 128` genera `aac 128k` verificado con `ffprobe` (±10%); valor fuera de rango → error `E_INVALID_INPUT`.

### RF-06 Modelo autocontenido on-demand
* **ID:** RF-06. **Prioridad:** Alta.
* Artefactos `dfn3_enc.onnx, dfn3_erb_dec.onnx, dfn3_df_dec.onnx` en `--model-dir` (defecto `Path.home()/.cache/denoise-videos/models` en Win/macOS/Linux vía `pathlib`).
* Si faltan → descargar tarball oficial `v0.5.6`, verificar tamaño `7983136B >=98%` + `SHA256` según regla D2: hasta T2.5 (`TBD`) solo se exige tamaño + warning visible `SHA256 pendiente de registro`; tras registrar el hash real en estos docs, `SHA256` pasa a obligatorio y el mismatch es `E_MODEL_MISSING` bloqueante; tag `v1.0.0` bloqueado hasta registrarlo (en primera descarga con tamaño ok se calcula el `SHA256` real y se sustituye en estos docs. Origen fijado `../NextgenUp@16e01bb` 2026-09-09 `model_store.py:denoise_speech`), extraer, borrar `.tar.gz`. Si la descarga falla → `E_MODEL_MISSING` + URL + ruta manual esperada.
* **Aceptación:** primera corrida descarga una vez (~8MB); segunda offline funciona; corrupto/incompleto reintenta con error legible.

### RF-07 Pre-chequeos ffmpeg y audio
* **ID:** RF-07. **Prioridad:** Alta.
* Resolver ffmpeg vía `--ffmpeg-path` o `PATH`; si ausente → `E_FFMPEG_NOT_FOUND`. Requiere `ffmpeg 6+` (verificado con `ffmpeg -version`), sin `ffprobe`.
* Receta instalación documentada en `README.md`: Win `winget install Gyan.FFmpeg` / `choco install ffmpeg`, macOS `brew install ffmpeg`, Linux `apt install ffmpeg` (verificar `ffmpeg -version` muestra `6+`).
* `has_audio` y duración vía `ffmpeg -hide_banner -i` (heredado de `../NextgenUp@16e01bb/audio_engine.py` + `../NextgenUp@16e01bb/video_tools.py`); sin audio → `E_NO_AUDIO`, sin salida.
* **Aceptación:** video sin audio, ffmpeg ausente y entrada corrupta reportan códigos distintos y no dejan parciales.

### RF-08 Lote robusto, dry-run, resumen JSON/humano
* **ID:** RF-08. **Prioridad:** Media.
* Procesamiento secuencial v1; un fallo no aborta lote (salvo `Ctrl+C`). Resumen final `ok=N failed=N skipped=N`.
* `--dry-run`: solo lista `input → output` sin tocar disco IA, en el mismo orden del lote real. Con `--dry-run --json` emite la misma tabla en `JSONL` con `status="dry-run"` y `pct=0`, sin crear nada.
* Humano: cabecera `[i/N]`, una barra viva por video a `stderr`, línea final por video con `MB + segundos`, resumen final. Sin TTY: líneas de porcentaje sin animación. Orden estricto, `flush=True`. Mapeo pct D4 congelado: `0 inicio/colisión-check, 1-5 extract, 6-80 denoise por chunks (d/t), 81-95 remux, 96-99 verificar+limpiar, 100 ok/failed/skipped`.
* `--json`: desactiva animación; `stdout` en `JSONL` una línea por archivo `{input,output,status,message,pct}` + línea final `{summary:{ok,failed,skipped}}` (contrato congelado Ronda 3 + D4: `status ∈ {ok,failed,skipped,dry-run}`, `pct 0-100` según mapeo, `summary` sin `pct`). Sin `--json`: salida humana a `stderr`.
* **Aceptación:** lote 5 con 1 corrupto → 4 ok + 1 failed, exit `!=0`, JSON parseable; en modo humano el proceso muestra avance por video sin quedarse silencioso >2s.

### RF-09 Temporales, cancelación y limpieza
* **ID:** RF-09. **Prioridad:** Media.
* Temps `<out>.tmp.in/out.wav` junto a la salida. Borrado en `finally`; con `-v` se conservan para debug.
* `Ctrl+C` D8: manejador unificado `KeyboardInterrupt` → `Popen.terminate/kill` al hijo `ffmpeg`/inferencia, borra parcial y temps, exit 3. En Win crear hijo con `CREATE_NEW_PROCESS_GROUP` para terminación limpia; en POSIX grupo por defecto. Comportamiento observable único Win/POSIX.
* **Aceptación:** interrumpir a mitad no deja `.mp4` parcial ni `.wav`.

### RF-10 Ayuda, versión y verbosidad
* **ID:** RF-10. **Prioridad:** Media.
* `--help` documenta todos los flags con ejemplos; `--version` imprime `denoise-videos 1.0.0 + modelo DFN3 v0.5.6 + ffmpeg <ver>` — formato exacto congelado Ronda 3 — (`__version__="1.0.0"` en `src/denoise_videos/__init__.py` como fuente única, `pyproject.toml` usa `dynamic=["version"]` con `tool.setuptools.dynamic version attr="denoise_videos.__version__"`, Decisión D1); `-v` debug (comando `ffmpeg` exacto, `model-dir`, tiempos por fase, `chunks d/t`, tamaños).
* Niveles: defecto `info` (fases), `-v` `debug`. Sin nivel silencioso separado: `--json` ya es apto para scripting.
* **Aceptación:** `denoise --help` es suficiente para usar sin leer este doc; `-v` permite reproducir manualmente cada `ffmpeg`.

## 3. Requerimientos no funcionales

### RNF-01 Autocontención
* Todo vive en este repo (`Video-Noise-Remover/` raíz). Cero imports fuera del repo; en runtime nada se importa de `../NextgenUp@16e01bb/` (solo lectura como referencia para portar). Stack V1 (congelado): `Python>=3.10 + numpy>=1.26 + onnxruntime>=1.20 + tqdm>=4.66 + certifi` + binario `ffmpeg 6+`. Toolchain mínimo estándar: `setuptools>=61`, `requires-python>=3.10`, `pytest marker slow`, `ruff+black` defaults (ver `plan.md T0.3`). Hoja de ruta decidida: V1 se implementa íntegramente en Python; V2 migrará de forma íntegra a Rust (ver `§7`, `nuevo-diseño.md §11`, `plan.md Fase 6`).
* Estilo: `type hints`, `pathlib`, `ruff+black`, `argv` lista sin `shell=True`, `UTF-8 errors=replace`.
* **Verificación:** `pip install -r requirements.txt` + `python -m denoise_videos` funciona con este repo copiado a otra máquina.

### RNF-02 Portabilidad
* `Windows 10+ / macOS 13+ / Linux x64`. Rutas con espacios y no-latinas. Subprocess siempre `UTF-8 errors=replace`. Sin `NUL` vs `/dev/null` hardcodeado.
* **Verificación:** matriz manual 1 video corto por OS.

### RNF-03 Rendimiento y recursos
* Memoria constante por chunks `60s/1s overlap` (pico `~300MB` más `ffmpeg`); CPU 1 job secuencial v1; sin GPU requerida.
* No re-encode de video → tiempo dominado por DFN3 + remux ligero. Telemetría en `-v` (segundos por fase), sin objetivo contractual.

### RNF-04 Fiabilidad numérica
* Constantes DSP congeladas (ver `nuevo-diseño.md §6`: `SR48000/FFT960/HOP480/ERB32/DF96/ORDER5/LOOKAHEAD2/ALPHA0.99/LSNR-10/30/20`). Test dorado obligatorio y bloqueante antes de release con `SI-SDR` numpy puro en `tests/si_sdr.py` (D11: zero-mean por señal, `eps=1e-8`, `10*log10(||s_target||²/||e||²)`) y vectores deterministas en `tests/data/` (seno `440Hz 3s` + ruido blanco `SNR 10dB`, `seed 0`, `SR 48k` mono `float32`: `voz.wav`, `mezcla10dB.wav`, `referencia_dfn3.wav`; Decisión D3: asserts `SI-SDR(denoised,voz)-SI-SDR(mezcla,voz) >=5dB` Y `SI-SDR(denoised,referencia) >=60dB`; valores `20.8dB` pipeline oficial / `~77dB` paridad bit-exacta solo informativos). Desviación bajo umbrales = bloqueante. Origen DSP fijado `../NextgenUp@16e01bb` (`audio_engine.py:183-346`); tras el port manda el test dorado.

### RNF-05 Seguridad
* Sin red salvo descarga modelo desde origen fijo único (`github.com/Rikorose/DeepFilterNet v0.5.6`, sin HuggingFace en v1). Sin ejecución de nombres de archivo como shell (lista argv, no `shell=True`). `-o` fuera del cwd permitido pero se advierte si sobreescribe entrada sin `prefix/suffix`.
* Modelos verificados por tamaño `7983136B >=98%` + `SHA256` según D2 (interino solo tamaño+warning, tras registro obligatorio bloqueante para `v1.0.0`); nunca se ejecuta código descargado salvo `.onnx` vía `onnxruntime`.

### RNF-06 Usabilidad y scripting
* Mensajes de error accionables (`qué pasó + qué hacer`). Exit codes estables: `0 ok/skip, 1 ffmpeg/modelo, 2 entrada/salida, 3 cancelado`. D9: batch mixto con fallos `1` y `2` → exit `1` (prioridad `1>2`, `3` siempre gana si hubo cancelación). `--json` estable para `jq`. D9: `--version` sin `ffmpeg` imprime `ffmpeg missing` en lugar de fallar.

### RNF-07 Mantenibilidad y test
* Módulos <300 líneas c/u, funciones puras donde sea posible (`resolve_output()` en `cli.py` testeable sin ffmpeg). `tests/test_naming.py` sin red ni modelo; `test_golden/test_remux` marcados `slow`.
* Comandos: `pytest -q -m "not slow"` (rápido) y `pytest -q -m slow` (dorado+e2e). Docstrings estilo Google en funciones públicas.

### RNF-08 Licencias y atribución
* Código nuevo: `MIT` (`LICENSE` en raíz, `__version__="1.0.0"`). Modelo DeepFilterNet3: `MIT © Rikorose/DeepFilterNet` — mantener aviso y enlace en `README.md` y `--version`. `ffmpeg` externo `GPL/LGPL` — no se vende empaquetado sin revisar.

### RNF-09 Documentación
* `README.md` mínimo: instalación, 3 ejemplos (`1 archivo`, `lote --output-dir`, `--dry-run+--json`), limitaciones v1 (mono, primera pista, secuencial), atribución `MIT © Rikorose/DeepFilterNet`. `--help` idéntico al contrato §4 de `nuevo-diseño.md`. `CHANGELOG.md` por release con versión de `src/denoise_videos/__init__.py:__version__` (inicial `1.0.0`). `LICENSE` MIT en raíz.

## 4. Matriz flags ↔ RF (resumen)

| Flag | RF |
|---|---|
| `INPUT..., --recursive` | RF-01, RF-02 |
| `-o, --output-dir, --prefix, --suffix` | RF-03 |
| `--overwrite, --skip-existing` | RF-04 |
| (pipeline interno) | RF-05 |
| `--audio-bitrate` | RF-05B |
| `--model-dir` | RF-06 |
| `--ffmpeg-path` | RF-07 |
| `--dry-run, --json, -v` | RF-08, RF-09, RF-10 |

## 5. Casos borde obligatorios

1. Video vertical/teléfono, 4K, `mkv` con múltiples audios → se usa `0:v:0` + primera pista a mono; resto de pistas/subs se pierden (limitación v1 confirmada Ronda 4, documentada en `--help` y `README.md`).
2. Nombre con espacios/acentos/emoji + ruta >150 caracteres en Win.
3. Video sin audio, imagen renombrada a `.mp4`, archivo `0B`, `bitrate` fuera de rango.
4. Salida en disco distinto / sin permiso escritura / `-o` a carpeta inexistente (debe crearla) vs `-o` con `N>1` (debe fallar).
5. Lote con mezcla de ok + sin-audio + colisión existente + 1 ruta inexistente.
6. Corte `Ctrl+C` durante `extract`, durante `inferencia chunk 3/7`, durante `remux` → exit 3, sin `.part` ni `.wav`.

## 6. Criterios de aceptación de entrega v1

* [ ] `denoise 1.mp4` y `denoise dir/ --recursive --output-dir out/ --suffix _clean` funcionan offline tras primera descarga.
* [ ] `pytest -q -m "not slow"` en verde + `test_golden/test_remux` en verde en 1 máquina Win.
* [ ] `--dry-run` y `--json` verificados en lote 5 con 1 fallo inducido; progreso humano muestra `[i/N]` por video.
* [ ] Sin imports fuera de este repo, `requirements.txt` mínimo instala limpio en venv vacío.
* [ ] Este doc + `nuevo-diseño.md` con contrato idéntico y sin flags fuera de lista.

## 7. Hoja de ruta V1 → V2 (decisión congelada)

* **V1 (alcance de estos docs):** implementación íntegra en Python según `RNF-01`. Contrato CLI (`§4` de `nuevo-diseño.md`), errores, `pct` D4 y umbrales D3 mandan.
* **V2 (futura, fuera de DoD v1):** migración íntegra a Rust del mismo contrato sin cambios observables: mismo CLI, mismos `exit codes`, mismo `JSONL`, mismos umbrales dorados D3 y misma verificación ligera D5. Objetivo: binario único (`clap + ort + hound + indicatif + reqwest`), reutilizando el ecosistema oficial `DeepFilterNet` en Rust donde corresponda. Ninguna tarea V2 bloquea el tag `v1.0.0`.
