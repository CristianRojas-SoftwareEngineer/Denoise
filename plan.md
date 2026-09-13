# Plan de implementación — `Video-Noise-Remover` v1.0.0

> Documentos canónicos: `especificaciones.md` (RF/RNF + aceptación) + `nuevo-diseño.md` (arquitectura + DSP + contrato §4).
> Este plan no redefine contratos; solo ordena el trabajo para desbloquear la reimplementación.
> Repo autocontenido en raíz `./`. Original solo lectura en `../NextgenUp@16e01bb` (2026-09-09, `Release v1.2.0`; portar cuando corresponda, sin importar; tras el port manda el test dorado).
> Decisiones cerradas Ronda 1: `SHA256 bloqueante para v1.0.0 (TBD hasta T2.5, tag bloqueado)` + `origen DSP fijado 16e01bb` + `tests/data sintético: seno 440Hz 3s + ruido blanco SNR 10dB seed 0 SR48k mono` + `contrato tests incluye si_sdr.py + test_reporter.py + data/README.md`. Ronda 2: `toolchain mínimo estándar (setuptools>=61, py3.10, slow marker, ruff+black defaults)` + `receta ffmpeg por OS`. Ronda 3: `Ctrl+C único Win/POSIX exit 3 sin parciales` + `temps junto a salida + -v conserva` + `schema JSONL/version/dry-run congelado`. Ronda 4: `multi-pista solo primera + aviso` + `naming/entry/rename congelados` + `casos borde §5 spec confirmados (ruta larga, disco distinto, -o crea dirs / -o con N>1 falla)`. Resto decisiones cerradas en spec: `MIT + 1.0.0 + JSONL {input,output,status,message,pct}+{summary:{ok,failed,skipped}} + tarball 7983136B>=98% + model-dir Path.home()/.cache + ffmpeg 6+ sin ffprobe + SI-SDR numpy en tests/si_sdr.py + solo github.com/Rikorose`.

## 0. Convenciones globales (valen para todas las fases)

* Stack V1 (congelado): `Python>=3.10 + numpy>=1.26 + onnxruntime>=1.20 + tqdm>=4.66 + certifi` + binario `ffmpeg 6+`. Sin `torch/librosa/Flask/pillow/opencv`. Hoja de ruta: V1 Python íntegro; V2 migración íntegra a Rust (ver `Fase 6`, `especificaciones.md §7`, `nuevo-diseño.md §11`).
* Estilo: `type hints`, `pathlib`, `ruff+black`, `argv` lista sin `shell=True`, `UTF-8 errors=replace`, ASCII seguro en `pwsh`, `flush=True`.
* Versión: `src/denoise_videos/__init__.py:__version__="1.0.0"` fuente única. `LICENSE MIT`, `README.md`, `CHANGELOG.md (1.0.0)` en raíz.
* Comandos:
  * `pip install -r requirements.txt`
  * `pytest -q -m "not slow"` (rápido, sin red/modelo/ffmpeg pesado)
  * `pytest -q -m slow` (dorado + e2e)
  * `python -m denoise_videos --help`
* DoD por tarea: código + test en verde + sin imports fuera del repo + `ruff+black` limpio.
* DoD v1 (de `especificaciones.md §6`): `denoise 1.mp4` y `denoise dir/ --recursive --output-dir out/` offline tras primera descarga; `not slow` + `test_golden/test_remux` verdes en Win; lote 5 con 1 fallo verificado en `--dry-run` y `--json` con `[i/N]` visible.

## Fase 0 — Bootstrap repo y entorno (desbloquea todo)

Objetivo: repo instalable vacío que prueba el contrato de autocontención `RNF-01`.

* T0.1 `git init` en `Video-Noise-Remover/` (si no existe), `.gitignore`: `__pycache__/`, `.venv/`, `*.tmp.*.wav`, `*.part.mp4`, `.pytest_cache/`.
* T0.2 Crear `LICENSE` MIT, `README.md` mínimo (instalación + 3 ejemplos idénticos a `nuevo-diseño.md §4` + limitaciones mono/primera pista/secuencial + atribución `MIT © Rikorose/DeepFilterNet`), `CHANGELOG.md` con entrada `1.0.0`.
* T0.3 Crear `requirements.txt` (`numpy>=1.26, onnxruntime>=1.20, tqdm>=4.66, certifi`) + `pyproject.toml` mínimo estándar: `build-system requires setuptools>=61`, `[project] name=denoise-videos dynamic=["version"] requires-python>=3.10` con `tool.setuptools.dynamic version attr="denoise_videos.__version__"`, `entry-point: denoise = "denoise_videos.cli:main"`, `tool.pytest.ini_options markers slow`, `ruff+black` sin config extra (defaults). Decisión D1: fuente única `src/denoise_videos/__init__.py:__version__`, prohibido duplicar `version=` fija en `pyproject.toml`.
* T0.4 Crear esqueleto `src/denoise_videos/__init__.py (__version__="1.0.0")`, `__main__.py (python -m denoise_videos)`, `cli.py`, `pipeline.py`, `dfn3.py`, `models.py`, `ffmpeg_io.py` con `TODO` + docstrings Google, y `tests/__init__.py` vacío + stubs `test_naming.py`, `test_golden.py`, `test_remux.py`, `test_errors.py`, `test_reporter.py`, `si_sdr.py`, `data/README.md` (contenido real en Fases 1-4).
* T0.5 Verificación: `pip install -r requirements.txt` en venv vacío + `python -m denoise_videos --help` (aunque sea stub) funciona con el repo copiado a otra carpeta.

Salida: `pytest -q -m "not slow"` colecta 0 tests sin error; `RNF-01` verificable.

## Fase 1 — CLI pura sin I/O (lógica testeable sin ffmpeg/red)

Objetivo: cerrar `RF-01/02/03/04 + RF-08(dry-run/reporte puro) + RF-10(help/version)` con `tests/test_naming.py` en verde. Portar nada del original; todo nuevo.

* T1.1 `cli.py: argparse` exacto del contrato §4: `INPUT... [-o OUT|--output-dir DIR] [--prefix] [--suffix=_denoised] [--recursive] [--overwrite|--skip-existing] [--audio-bitrate=192] [--model-dir] [--ffmpeg-path] [--dry-run] [--json] [-v] [--version]`. `--overwrite/--skip-existing` mutuamente excluyentes. `--audio-bitrate 64-320`, si no → `E_INVALID_INPUT`.
* T1.2 `cli.py: expandir_entradas()`: archivos `mp4/mov/mkv/webm/avi` case-insensitive + directorios según `--recursive`, orden alfabético determinista, sin duplicados por absoluto normalizado. D6: excluir `--output-dir` si está dentro de `INPUT` + aviso `-v`. Ruta inexistente/extensión mala → registra `failed E_INVALID_INPUT`, no aborta lote.
* T1.3 `cli.py: resolve_output()` pura: precedencia `1) N==1 + -o exacto (crea dirs) > 2) --output-dir/<prefix><stem><suffix>.mp4 recreando árbol si --recursive > 3) junto a original`. Valida `prefix/suffix [A-Za-z0-9._-]`, no ambos vacíos si in-place. Colisión sin `--overwrite` → `E_OUTPUT_EXISTS`; con `--skip-existing` → `skipped`.
* T1.4 `cli.py: --dry-run` tabla `input → output (skip: motivo)` mismo orden del lote real, sin tocar disco/IA. D4: con `--json` emite `JSONL status="dry-run" pct=0` + `summary`. Sin `--json` humano a `stderr`. `--json` emite `JSONL {input,output,status,message,pct}+{summary:{ok,failed,skipped}}` a `stdout`, humano a `stderr`, sin animación, `pct` según mapeo D4. `--version` → `denoise-videos 1.0.0 + modelo DFN3 v0.5.6 + ffmpeg <ver>` desde `__version__` (D1).
* T1.5 `tests/test_naming.py` (sin ffmpeg/red): 1 archivo, N archivos, `--output-dir`, `prefix/suffix`, `-o` solo `N==1` → error si `N>1`, colisión + `overwrite/skip`, `--recursive` recrea árbol + D6 exclusión output anidado, caracteres inválidos → `E_INVALID_INPUT`.

Verificación: `pytest -q -m "not slow"` verde. Riesgo: ninguno (sin I/O).

## Fase 2 — `ffmpeg_io.py` + `models.py` (I/O externo + descarga)

Objetivo: cerrar `RF-06/RF-07 + RNF-05` con `tests/test_errors.py` parcial (sin DSP).

* T2.1 `ffmpeg_io.py: find_ffmpeg()` ← portar `../NextgenUp@16e01bb/audio_engine.py:_ffmpeg()` simplificado: `--ffmpeg-path → PATH (shutil.which) → E_FFMPEG_NOT_FOUND`. Exigir `ffmpeg 6+` vía `ffmpeg -version` con D11 regex `ffmpeg version (\d+)\.` major>=6. Sin `ffprobe`, sin `BUNDLE_DIR/EXE_DIR/src-tauri`. Documentar receta por OS en `README.md` (Win `winget/choco`, macOS `brew`, Linux `apt`).
* T2.2 `ffmpeg_io.py: has_audio()/probe()` ← portar `../NextgenUp@16e01bb/video_tools.py:_has_audio()` + `audio_engine.py:probe_duration()`: `ffmpeg -hide_banner -i`, `Audio:` en `stderr`, regex `Duration:`. Sin audio → `E_NO_AUDIO`.
* T2.3 `ffmpeg_io.py: extract_mono48k()/remux_copy()` ← portar `video_tools.py:clean_audio()` rama denoise: `ffmpeg -y -v error -i IN -vn -ac 1 -ar 48000 TMP.in.wav` y `ffmpeg -y -v error -i IN -i TMP.out.wav -map 0:v:0 -map 1:a:0 -c:v copy -c:a aac -b:a <bitrate>k -shortest OUT.mp4`. Todo `argv` lista, `errors=replace`. Error → `E_FFMPEG_FAILED`, sin parciales.
* T2.4 `models.py: ensure_models()` ← portar `../NextgenUp@16e01bb/model_store.py:denoise_speech` (solo rama): URL `.../DeepFilterNet3_onnx.tar.gz`, `7983136B>=98% + SHA256 según D2 (interino: solo tamaño+warning si TBD, tras registro SHA obligatorio)`, miembros `tmp/export/{enc,erb_dec,df_dec}.onnx → dfn3_*.onnx`, `User-Agent: Video-Noise-Remover/1.0.0`, `certifi`, D11 `timeout 30s/retry 3`, anti `tar-slip`, chequeo disco, `.part + os.replace`, extraer + borrar `.tar.gz`. `model-dir` defecto `Path.home()/.cache/denoise-videos/models` vía `pathlib`. Fallo → `E_MODEL_MISSING` + URL + ruta manual. Sesiones `CPUExecutionProvider` cacheadas + lock.
* T2.5 Tarea bloqueante para release (cierra `SHA256:TBD` según D2): en primera descarga con tamaño ok (`>=98%`), calcular `SHA256` real del tarball (`Get-FileHash -Algorithm SHA256` en Win / `sha256sum` en Linux/macOS) y sustituir `TBD` en `especificaciones.md RF-06/RNF-05` + `nuevo-diseño.md §7`; desde entonces SHA obligatorio. Sin este registro no hay tag `v1.0.0`.
* T2.6 `tests/test_errors.py` (parte 1): sin audio → `E_NO_AUDIO`, ffmpeg ausente → `E_FFMPEG_NOT_FOUND`, descarga rota → `E_MODEL_MISSING`, destino existe → `E_OUTPUT_EXISTS/skipped`, `bitrate 9999` → `E_INVALID_INPUT`. Nada parcial en disco.

Verificación: `pytest -q -m "not slow"` verde con ffmpeg 6+ real pero sin ONNX pesado (mock `ensure_models`).

## Fase 3 — `dfn3.py` DSP + test dorado (corazón numérico)

Objetivo: cerrar `RF-05 + RNF-04` con `tests/test_golden.py (slow)` bloqueante. Port exacto, cero decisiones nuevas.

* T3.1 `dfn3.py` portar `../NextgenUp@16e01bb/audio_engine.py:183-346` valores idénticos: `SR48000/FFT960/HOP480/ERB32/DF96/ORDER5/LOOKAHEAD2/WNORM=1/(FFT²/2HOP)/ALPHA0.99/LSNR-10/30/20/CHUNK60s/OVERLAP1s`, `_erb_widths()`, `_df_constants()` (vorbis + `erb_fb/erb_inv`), framing `pad HOP + cola FFT+LOOKAHEAD*HOP`, `STFT*ventana*WNORM`, features `ERB mean-norm/40 + unit-norm compleja`, inferencia `enc/erb_dec/df_dec`, alineación `k+LOOKAHEAD`, `mask@erb_inv + deep-filter taps k-2..k+2 si lsnr<=20 / intacto si >30 / mute si <-10`, `iSTFT*FFT*ventana + overlap-add + recorte HOP:HOP+n + crossfade`. Firma `denoise_wav(in_wav,out_wav,progress_cb)`, sin `subprocess/print`, solo `numpy+onnxruntime`. D7: I/O WAV con `stdlib wave` PCM16 ↔ `float32` en `ffmpeg_io.py`/helpers, no en núcleo DSP.
* T3.2 `tests/si_sdr.py` (helper, no dependencia externa): `SI-SDR` numpy puro D11 (zero-mean, `eps=1e-8`, `10*log10(||s_target||²/||e||²)`).
* T3.3 `tests/data/README.md` + generador determinista (`seed 0`): seno `440Hz 3s` + ruido blanco a `SNR 10dB`, `SR 48k` mono `float32`. Guardar `voz.wav`, `mezcla10dB.wav`, `referencia_dfn3.wav` (generada una vez con el port validado contra `../NextgenUp@16e01bb`).
* T3.4 `tests/test_golden.py (slow)`: `denoise_wav(mezcla)` D3: `mejora >=5dB` y `paridad vs referencia >=60dB` (refs informativas `+5dB` / `20.8dB` / `~77dB`). Desviación = bug bloqueante.

Verificación: `pytest -q -m slow -k golden` verde en 1 máquina Win. Riesgo mayor: regresión numérica por offsets/ventana — mitigación: no tocar valores, comparar contra `../NextgenUp@16e01bb/` salida.

## Fase 4 — `pipeline.py` + reporte lote (integración)

Objetivo: cerrar `RF-05/08/09 + RNF-02/03/06` con `tests/test_remux.py + test_reporter`.

* T4.1 `pipeline.py: clean_one_video()` ← portar `video_tools.py:clean_audio()` solo rama denoise: `expandir→validar→resolver salida→has_audio?→modelo?→colisión?→[1-5%] extract→[6-80%] dfn3 por chunks con callback (d,t)→[81-95%] remux a OUT.part.mp4→rename atómico→[96-99%] D5 verificación ligera (>0B+duración±0.5s+AAC)→limpiar temps→[100%] report (mapeo D4)`. `finally` borra `<out>.tmp.in/out.wav` + `.part`; con `-v` conserva temps + muestra comando exacto, `model-dir`, tiempos por fase, `chunks d/t`, tamaños.
* T4.2 Progreso/reporte en `cli.py`: secuencial v1, un fallo no aborta (salvo `Ctrl+C` D8: `KeyboardInterrupt→terminate/kill`, borra parcial/temps, `exit 3`, Win `CREATE_NEW_PROCESS_GROUP`). Humano TTY: cabecera `[i/N] in → out` + 1 barra `tqdm` por video a `stderr` + línea `done|failed|skipped MB+s` + resumen `ok/failed/skipped`. Sin TTY: líneas `%` por fase. `--json`: solo `JSONL`, sin animación. Exit D9: `3` si cancelado, si no `1` si hubo fallo `1`, si no `2` si hubo fallo `2`, si no `0`.
* T4.3 `tests/test_remux.py (slow)`: video 5s barras+tono → mismo `vcodec/res/fps`, duración `±0.2s`, `aac 48k bitrate±10%`, D10 sin re-encode por hash stream/`extradata` (no tamaño fichero).
* T4.4 `tests/test_reporter`: lote 3 simulado verifica orden `[1/3..3/3]`, `JSONL` parseable con `jq`, `summary` correcto, sin animación con `--json`/sin TTY.

Verificación: `pytest -q -m "not slow"` + `pytest -q -m slow -k "remux or reporter"` verdes.

## Fase 5 — E2E v1 + release 1.0.0

Objetivo: cumplir `especificaciones.md §6` y publicar.

* T5.1 Casos borde obligatorios (§5 spec): vertical/4K/`mkv` multi-audio (usa `0:v:0` + primera pista), espacios/acentos/emoji + ruta >150 chars Win, `0B`/imagen renombrada/`bitrate` fuera de rango, disco distinto/sin permiso/`-o` a carpeta inexistente (crearla), lote mixto ok+sin-audio+colisión+inexistente, `Ctrl+C` en extract/inferencia/remux → `exit 3` sin `.part/.wav`.
* T5.2 Manual Win: 1 video corto + lote 5 (`--dry-run` primero, luego real + `--json`), progreso `[i/N]` sin silencio >2s.
* T5.3 `--help` idéntico a contrato §4, `README.md/CHANGELOG.md/LICENSE` finales, `ruff+black` + `pip install -r requirements.txt` limpio en venv vacío.
* T5.4 Tag `v1.0.0`: `pytest -q -m "not slow"` + `pytest -q -m slow` verdes, sin imports fuera del repo.

## Trazabilidad RF → fase/test

| RF | Fase | Test |
|---|---|---|
| RF-01, RF-02, RF-03, RF-04 | 1 | `test_naming` |
| RF-05B | 1+4 | `test_naming` + `test_remux` (bitrate±10%) |
| RF-06 | 2 | `test_errors (E_MODEL_MISSING)` + E2E offline |
| RF-07 | 2 | `test_errors (E_FFMPEG_NOT_FOUND, E_NO_AUDIO)` |
| RF-05 | 3+4 | `test_golden (slow)` + `test_remux (slow)` |
| RF-08 | 1+4 | `test_reporter` + lote 5 E2E |
| RF-09 | 4+5 | `test_errors (Ctrl+C simulado)` + manual |
| RF-10 | 1+5 | `--help/--version` manual + E2E |
| RNF-04 | 3 | `test_golden mejora>=5dB + paridad>=60dB (D3)` bloqueante |

## Riesgos principales

1. Regresión numérica DSP → mitigado por Fase 3 bloqueante + valores congelados.
2. `ffmpeg` Win (`PATH`, espacios, no-latino) → mitigado por Fase 2 + matriz manual Win.
3. `SHA256:TBD` → tarea T2.5 obligatoria antes de release.
4. Alcance: sin `MDX/compress/gif/Tauri/Flask` en v1 (ver §9 diseño).

## Fase 6 — V2 Rust (futura, no bloquea v1.0.0)

Objetivo: migración íntegra a Rust del mismo contrato V1 sin cambios observables.

* T6.1 Reimplementar `cli/pipeline/dfn3/models/ffmpeg_io` con `clap 4 + ort + hound + ndarray/rustfft + indicatif + reqwest + sha2 + flate2/tar`, manteniendo `§4` CLI, exits D9, `pct/JSONL` D4 y verificación D5.
* T6.2 Puerta de paridad: reutilizar `tests/data/` (`voz/mezcla/referencia`) y exigir D3 (`>=5dB`, `>=60dB`) + `cargo test`; evaluar crates oficiales `DeepFilterNet` (`libDF`) en vez de re-portar DSP.
* T6.3 Distribución: binario único por OS + `ffmpeg 6+` externo, `clippy+rustfmt` limpios. Ninguna T6 bloquea el tag `v1.0.0`.
