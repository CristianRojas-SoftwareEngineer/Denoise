# Plan de implementación — `Video-Noise-Remover` v1.0.0 Rust

> Documentos canónicos: `especificaciones.md` (RF/RNF + aceptación) + `nuevo-diseño.md` (arquitectura + DSP + contrato §4).
> Este plan no redefine contratos; solo ordena el trabajo para la implementación Rust directa.
> Repo autocontenido en raíz `./`. Original Python solo lectura en `../NextgenUp@16e01bb` (2026-09-09, `Release v1.2.0`; portar lógica a Rust cuando corresponda, sin ejecutar; tras el port manda el test dorado).
> Decisiones cerradas Ronda 1: `SHA256 bloqueante para v1.0.0 (TBD hasta T2.5, tag bloqueado)` + `origen DSP fijado 16e01bb` + `tests/data sintético: seno 440Hz 3s + ruido blanco SNR 10dB seed 0 SR48k mono f32` + `contrato tests incluye common/si_sdr.rs + test_reporter.rs + data/README.md`. Ronda 2: `toolchain cargo stable 1.75+ edition 2021, #[ignore] slow, clippy+rustfmt` + `receta ffmpeg por OS`. Ronda 3: `Ctrl+C unificado con CREATE_NEW_PROCESS_GROUP exit 3 sin parciales` + `temps junto a salida + -v conserva` + `schema JSONL/version/dry-run congelado`. Ronda 4: `multi-pista solo primera + aviso` + `naming/bin/rename congelados` + `casos borde §5 spec confirmados (ruta larga, disco distinto, -o crea dirs / -o con N>1 falla)`. Pivot: `V1 Rust directo, sin V2; Python solo referencia`. Resto decisiones cerradas en spec: `MIT + 1.0.0 + JSONL {input,output,status,message,pct}+{summary:{ok,failed,skipped}} + tarball 7983136B>=98% + model-dir ~/.cache + ffmpeg 6+ sin ffprobe + SI-SDR Rust en tests/common/si_sdr.rs + solo github.com/Rikorose`.

## 0. Convenciones globales (valen para todas las fases)

* Stack V1 Rust (congelado): `stable 1.75+ edition 2021 + ort + ndarray + rustfft + hound + clap 4 + indicatif + reqwest (rustls) + sha2 + flate2 + tar + home/dirs + ctrlc + anyhow/thiserror + serde_json` + binario `ffmpeg 6+`. Sin Python en runtime.
* Estilo: `PathBuf`, `clippy+rustfmt`, `std::process::Command` argv sin shell, `String::from_utf8_lossy`, ASCII seguro en `pwsh`, flush explícito.
* Versión: `Cargo.toml [package] version="1.0.0"` fuente única vía `env!("CARGO_PKG_VERSION")` (D1-Rust). `LICENSE MIT`, `README.md`, `CHANGELOG.md (1.0.0)` en raíz.
* Comandos:
  * `cargo build --release`
  * `cargo test` (rápido, sin red/modelo/ffmpeg pesado)
  * `cargo test -- --ignored` (dorado + e2e)
  * `cargo run -- --help`
* DoD por tarea: código + test en verde + sin código Python + `clippy` sin warnings + `rustfmt` limpio.
* DoD v1 (de `especificaciones.md §6`): `denoise 1.mp4` y `denoise dir/ --recursive --output-dir out/` offline tras primera descarga; `cargo test` + `cargo test -- --ignored` verdes en Win; lote 5 con 1 fallo verificado en `--dry-run` y `--json` con `[i/N]` visible.

## Fase 0 — Bootstrap repo y entorno (desbloquea todo)

Objetivo: repo Rust compilable vacío que prueba `RNF-01`.

* T0.1 `.gitignore` Rust: `target/`, `*.tmp.*.wav`, `*.part.mp4`.
* T0.2 Crear `LICENSE` MIT, `README.md` mínimo (instalación `cargo build --release` + `ffmpeg 6+`, 3 ejemplos idénticos a `nuevo-diseño.md §4` + limitaciones mono/primera pista/secuencial + atribución `MIT © Rikorose/DeepFilterNet`), `CHANGELOG.md` con entrada `1.0.0`.
* T0.3 Crear `Cargo.toml` mínimo: `[package] name="denoise-videos" version="1.0.0" edition="2021"`, `[[bin]] name="denoise" path="src/main.rs"`, deps `clap 4 + ort + ndarray + rustfft + hound + indicatif + reqwest + sha2 + flate2 + tar + home + ctrlc + anyhow + thiserror + serde_json`. Decisión D1-Rust: fuente única `Cargo.toml`, prohibido duplicar versión en código.
* T0.4 Crear esqueleto `src/main.rs`, `cli.rs`, `pipeline.rs`, `df.rs`, `models.rs`, `ffmpeg_io.rs`, `errors.rs` con `TODO` + `tests/test_naming.rs`, `test_golden.rs`, `test_remux.rs`, `test_errors.rs`, `test_reporter.rs`, `common/si_sdr.rs`, `data/README.md` (contenido real en Fases 1-4).
* T0.5 Verificación: `cargo build --release` + `./target/release/denoise --help` (aunque sea stub) funciona con el repo copiado a otra carpeta con toolchain Rust.

Salida: `cargo test` colecta 0 tests sin error; `RNF-01` verificable.

## Fase 1 — CLI pura sin I/O (lógica testeable sin ffmpeg/red)

Objetivo: cerrar `RF-01/02/03/04 + RF-08(dry-run/reporte puro) + RF-10(help/version)` con `tests/test_naming.rs` en verde. Portar nada del original; todo nuevo Rust.

* T1.1 `cli.rs: clap` exacto del contrato §4: `INPUT... [-o OUT|--output-dir DIR] [--prefix] [--suffix=_denoised] [--recursive] [--overwrite|--skip-existing] [--audio-bitrate=192] [--model-dir] [--ffmpeg-path] [--dry-run] [--json] [-v] [--version]`. `--overwrite/--skip-existing` mutuamente excluyentes. `--audio-bitrate 64-320`, si no → `E_INVALID_INPUT`.
* T1.2 `cli.rs: expandir_entradas()`: archivos `mp4/mov/mkv/webm/avi` case-insensitive + directorios según `--recursive`, orden alfabético determinista, sin duplicados por absoluto normalizado. D6: excluir `--output-dir` si está dentro de `INPUT` + aviso `-v`. Ruta inexistente/extensión mala → registra `failed E_INVALID_INPUT`, no aborta lote.
* T1.3 `cli.rs: resolve_output()` pura: precedencia `1) N==1 + -o exacto (crea dirs) > 2) --output-dir/<prefix><stem><suffix>.mp4 recreando árbol si --recursive > 3) junto a original`. Valida `prefix/suffix [A-Za-z0-9._-]`, no ambos vacíos si in-place. Colisión sin `--overwrite` → `E_OUTPUT_EXISTS`; con `--skip-existing` → `skipped`. D9: `-o` a la propia entrada → warning `stderr`.
* T1.4 `cli.rs: --dry-run` tabla `input → output (skip: motivo)` mismo orden del lote real, sin tocar disco/IA. D4: con `--json` emite `JSONL status="dry-run" pct=0` + `summary`. Sin `--json` humano a `stderr`. `--json` emite `JSONL {input,output,status,message,pct}+{summary:{ok,failed,skipped}}` a `stdout`, humano a `stderr`, sin animación, `pct` según mapeo D4. `--version` → `denoise-videos 1.0.0 + modelo DFN3 v0.5.6 + ffmpeg <ver>` desde `CARGO_PKG_VERSION` (D1-Rust, D9 `ffmpeg missing` si ausente).
* T1.5 `tests/test_naming.rs` (sin ffmpeg/red): 1 archivo, N archivos, `--output-dir`, `prefix/suffix`, `-o` solo `N==1` → error si `N>1`, colisión + `overwrite/skip`, `--recursive` recrea árbol + D6 exclusión output anidado, caracteres inválidos → `E_INVALID_INPUT`, D9 lote vacío → `E_INVALID_INPUT`.

Verificación: `cargo test` verde. Riesgo: ninguno (sin I/O).

## Fase 2 — `ffmpeg_io.rs` + `models.rs` (I/O externo + descarga)

Objetivo: cerrar `RF-06/RF-07 + RNF-05` con `tests/test_errors.rs` parcial (sin DSP).

* T2.1 `ffmpeg_io.rs: find_ffmpeg()` ← portar `../NextgenUp@16e01bb/audio_engine.py:_ffmpeg()` (Python lectura) simplificado: `--ffmpeg-path → PATH (split_paths) → E_FFMPEG_NOT_FOUND`. Exigir `ffmpeg 6+` vía `ffmpeg -version` con D11 regex `ffmpeg version (\d+)\.` major>=6. Sin `ffprobe`. Documentar receta por OS en `README.md` (Win `winget/choco`, macOS `brew`, Linux `apt`).
* T2.2 `ffmpeg_io.rs: has_audio()/probe()` ← portar `../NextgenUp@16e01bb/video_tools.py:_has_audio()` + `audio_engine.py:probe_duration()` (Python lectura): `ffmpeg -hide_banner -i`, `Audio:` en `stderr`, regex `Duration:`. Sin audio → `E_NO_AUDIO`.
* T2.3 `ffmpeg_io.rs: extract_mono48k()/remux_copy()` ← portar `video_tools.py:clean_audio()` rama denoise (Python lectura): `ffmpeg -y -v error -i IN -vn -ac 1 -ar 48000 TMP.in.wav` y `ffmpeg -y -v error -i IN -i TMP.out.wav -map 0:v:0 -map 1:a:0 -c:v copy -c:a aac -b:a <bitrate>k -shortest OUT.mp4`. Todo `Command` argv, `from_utf8_lossy`. Error → `E_FFMPEG_FAILED`, sin parciales.
* T2.4 `models.rs: ensure_models()` ← portar `../NextgenUp@16e01bb/model_store.py:denoise_speech` (Python lectura, solo rama): URL `.../DeepFilterNet3_onnx.tar.gz`, `7983136B>=98% + SHA256 según D2 (interino: solo tamaño+warning si TBD, tras registro SHA obligatorio)`, miembros `tmp/export/{enc,erb_dec,df_dec}.onnx → dfn3_*.onnx`, `User-Agent: Video-Noise-Remover/1.0.0`, `reqwest+rustls`, D11 `timeout 30s/retry 3`, anti `tar-slip`, chequeo disco, `.part + rename atómico`, extraer + borrar `.tar.gz`. `model-dir` defecto `~/.cache/denoise-videos/models` vía `home/dirs`. Fallo → `E_MODEL_MISSING` + URL + ruta manual. Sesiones `ort CPUExecutionProvider` cacheadas + `Mutex`.
* T2.5 Tarea bloqueante para release (cierra `SHA256:TBD` según D2): en primera descarga con tamaño ok (`>=98%`), calcular `SHA256` real del tarball (`Get-FileHash -Algorithm SHA256` en Win / `sha256sum` en Linux/macOS) y sustituir `TBD` en `especificaciones.md RF-06/RNF-05` + `nuevo-diseño.md §7`; desde entonces SHA obligatorio. Sin este registro no hay tag `v1.0.0`.
* T2.6 `tests/test_errors.rs` (parte 1): sin audio → `E_NO_AUDIO`, ffmpeg ausente → `E_FFMPEG_NOT_FOUND`, descarga rota → `E_MODEL_MISSING`, destino existe → `E_OUTPUT_EXISTS/skipped`, `bitrate 9999` → `E_INVALID_INPUT`. Nada parcial en disco.

Verificación: `cargo test` verde con ffmpeg 6+ real pero sin ONNX pesado (mock `ensure_models`).

## Fase 3 — `df.rs` DSP + test dorado (corazón numérico)

Objetivo: cerrar `RF-05 + RNF-04` con `tests/test_golden.rs (#[ignore])` bloqueante. Port exacto Python→Rust, cero decisiones nuevas salvo lenguaje.

* T3.1 `df.rs` portar `../NextgenUp@16e01bb/audio_engine.py:183-346` (Python lectura) valores idénticos: `SR48000/FFT960/HOP480/ERB32/DF96/ORDER5/LOOKAHEAD2/WNORM=1/(FFT²/2HOP)/ALPHA0.99/LSNR-10/30/20/CHUNK60s/OVERLAP1s`, `_erb_widths()`, `_df_constants()` (vorbis + `erb_fb/erb_inv`), framing `pad HOP + cola FFT+LOOKAHEAD*HOP`, `STFT*ventana*WNORM`, features `ERB mean-norm/40 + unit-norm compleja`, inferencia `ort enc/erb_dec/df_dec`, alineación `k+LOOKAHEAD`, `mask@erb_inv + deep-filter taps k-2..k+2 si lsnr<=20 / intacto si >30 / mute si <-10`, `iSTFT*FFT*ventana + overlap-add + recorte HOP:HOP+n + crossfade`. Firma `denoise_wav(in: &Path, out: &Path, progress: &dyn Fn(usize,usize))`, sin `Command`, solo `ndarray+rustfft+ort+hound`. D7: I/O WAV con `hound` PCM16 ↔ `f32`.
* T3.2 `tests/common/si_sdr.rs` (helper): `SI-SDR` Rust puro D11 (zero-mean, `eps=1e-8`, `10*log10(||s_target||²/||e||²)`).
* T3.3 `tests/data/README.md` + generador determinista (`seed 0`): seno `440Hz 3s` + ruido blanco a `SNR 10dB`, `SR 48k` mono `f32`. Guardar `voz.wav`, `mezcla10dB.wav`, `referencia_dfn3.wav` (generada una vez con el port validado contra `../NextgenUp@16e01bb`).
* T3.4 `tests/test_golden.rs (#[ignore])`: `denoise_wav(mezcla)` D3: `mejora >=5dB` y `paridad vs referencia >=60dB` (refs informativas `+5dB` / `20.8dB` / `~77dB`). Desviación = bug bloqueante.

Verificación: `cargo test -- --ignored golden` verde en 1 máquina Win. Riesgo mayor: regresión numérica por offsets/ventana/`f32` — mitigación: no tocar valores, comparar contra `../NextgenUp@16e01bb/` salida.

## Fase 4 — `pipeline.rs` + reporte lote (integración)

Objetivo: cerrar `RF-05/08/09 + RNF-02/03/06` con `tests/test_remux.rs + test_reporter.rs`.

* T4.1 `pipeline.rs: clean_one_video()` ← portar `video_tools.py:clean_audio()` solo rama denoise (Python lectura): `expandir→validar→resolver salida→has_audio?→modelo?→colisión?→[1-5%] extract→[6-80%] df por chunks con callback (d,t)→[81-95%] remux a OUT.part.mp4→rename atómico→[96-99%] D5 verificación ligera (>0B+duración±0.5s+AAC)→limpiar temps→[100%] report (mapeo D4)`. Limpieza garantizada + borrado `.part`; con `-v` conserva temps + muestra comando exacto, `model-dir`, tiempos por fase, `chunks d/t`, tamaños.
* T4.2 Progreso/reporte en `cli.rs`: secuencial v1, un fallo no aborta (salvo `Ctrl+C` D8-Rust: `ctrlc→Child::kill`, borra parcial/temps, `exit 3`, Win `CREATE_NEW_PROCESS_GROUP`). Humano TTY: cabecera `[i/N] in → out` + 1 barra `indicatif` por video a `stderr` + línea `done|failed|skipped MB+s` + resumen `ok/failed/skipped`. Sin TTY: líneas `%` por fase. `--json`: solo `JSONL`, sin animación. Exit D9: `3` si cancelado, si no `1` si hubo fallo `1`, si no `2` si hubo fallo `2`, si no `0`.
* T4.3 `tests/test_remux.rs (#[ignore])`: video 5s barras+tono → mismo `vcodec/res/fps`, duración `±0.2s`, `aac 48k bitrate±10%`, D10 sin re-encode por hash stream/`extradata` (no tamaño fichero).
* T4.4 `tests/test_reporter.rs`: lote 3 simulado verifica orden `[1/3..3/3]`, `JSONL` parseable con `jq`, `summary` correcto, sin animación con `--json`/sin TTY.

Verificación: `cargo test` + `cargo test -- --ignored remux` verdes.

## Fase 5 — E2E v1 + release 1.0.0

Objetivo: cumplir `especificaciones.md §6` y publicar.

* T5.1 Casos borde obligatorios (§5 spec): vertical/4K/`mkv` multi-audio (usa `0:v:0` + primera pista), espacios/acentos/emoji + ruta >150 chars Win, `0B`/imagen renombrada/`bitrate` fuera de rango, disco distinto/sin permiso/`-o` a carpeta inexistente (crearla), lote mixto ok+sin-audio+colisión+inexistente, `Ctrl+C` en extract/inferencia/remux → `exit 3` sin `.part/.wav`.
* T5.2 Manual Win: 1 video corto + lote 5 (`--dry-run` primero, luego real + `--json`), progreso `[i/N]` sin silencio >2s.
* T5.3 `--help` idéntico a contrato §4, `README.md/CHANGELOG.md/LICENSE` finales, `clippy+rustfmt` + `cargo build --release` limpio sin Python.
* T5.4 Tag `v1.0.0`: `cargo test` + `cargo test -- --ignored` verdes, sin código Python en repo (salvo `../NextgenUp` referencia externa).

## Trazabilidad RF → fase/test

| RF | Fase | Test |
|---|---|---|
| RF-01, RF-02, RF-03, RF-04 | 1 | `test_naming` |
| RF-05B | 1+4 | `test_naming` + `test_remux` (bitrate±10%) |
| RF-06 | 2 | `test_errors (E_MODEL_MISSING)` + E2E offline |
| RF-07 | 2 | `test_errors (E_FFMPEG_NOT_FOUND, E_NO_AUDIO)` |
| RF-05 | 3+4 | `test_golden (ignored)` + `test_remux (ignored)` |
| RF-08 | 1+4 | `test_reporter` + lote 5 E2E |
| RF-09 | 4+5 | `test_errors (Ctrl+C simulado)` + manual |
| RF-10 | 1+5 | `--help/--version` manual + E2E |
| RNF-04 | 3 | `test_golden mejora>=5dB + paridad>=60dB (D3)` bloqueante |

## Riesgos principales

1. Regresión numérica DSP Python→Rust (`f32`, `rustfft`, ventanas) → mitigado por Fase 3 bloqueante + valores congelados + vectores `tests/data`.
2. `ffmpeg` Win (`PATH`, espacios, no-latino) + `ort` dylib en Win → mitigado por Fase 2 + matriz manual Win + `cargo build` limpio.
3. `SHA256:TBD` → tarea T2.5 obligatoria antes de release.
4. Alcance: sin `MDX/compress/gif/Tauri/Flask/Python` en v1 (ver §9 diseño).
