# Plan de implementación — `Video-Noise-Remover` v1.0.0 Rust

> Documentos canónicos en `docs/`: `especificaciones.md` (RF/RNF + aceptación) + `nuevo-diseño.md` (arquitectura + DSP + contrato `docs/nuevo-diseño.md §4`). Nombres sin prefijo refieren a hermanos en `docs/` (`docs/<fichero>` desde la raíz).
> Este plan no redefine contratos; solo ordena el trabajo para la implementación Rust.
> Repo autocontenido en raíz `./` con docs canónicos en `docs/`. Sin runtime Python.

## 0. Convenciones globales (valen para todas las fases)

* Stack Rust: `stable 1.88+ (`rust-version="1.88"`, `edition="2021"`) + ort 2 (feature download-binaries) + ndarray + rustfft + hound + clap 4 + indicatif + reqwest 0.12 (rustls) + sha2 + flate2 + tar + home + which + ctrlc + anyhow (bin)/thiserror (lib) + serde 1 (+derive) + serde_json + log + env_logger` + `rand 0.8` dev-dep fixtures + binario `ffmpeg 6+`. `Cargo.lock` cometido; `[profile.release] opt-level=3, strip=true`. Sin Python en runtime. `model-dir` defecto `~/.cache` en las 3 OS.
* Estilo: `PathBuf`, `clippy+rustfmt`, `std::process::Command` argv sin shell, `String::from_utf8_lossy`, ASCII seguro en `pwsh`, flush explícito.
* Versión: `Cargo.toml [package] version="1.0.0"` fuente única vía `env!("CARGO_PKG_VERSION")`. `LICENSE MIT`, `README.md`, `CHANGELOG.md (1.0.0)` en raíz.
* Comandos:
  * `cargo build --release`
  * `cargo test` (rápido, sin red/modelo/ffmpeg pesado)
  * `cargo test -- --ignored` (dorado + e2e)
  * `cargo run -- --help`
* DoD por tarea: código + test en verde + sin código Python + `clippy` sin warnings + `rustfmt` limpio.
* DoD v1 (de `docs/especificaciones.md §6`): `denoise 1.mp4` y `denoise dir/ --recursive --output-dir out/` offline tras primera descarga; `cargo test` + `cargo test -- --ignored` verdes en Win; lote 5 con 1 fallo verificado en `--dry-run` y `--json` con `[i/N]` visible.

## Fase 0 — Bootstrap repo y entorno (desbloquea todo)

Objetivo: repo Rust compilable vacío que prueba `RNF-01`.

* T0.1 `.gitignore` Rust: `target/`, `*.tmp.*.wav`, `*.part.mp4`.
* T0.2 Crear `LICENSE` MIT, `README.md` mínimo (instalación `cargo build --release` + `ffmpeg 6+`, 3 ejemplos idénticos a `docs/nuevo-diseño.md §4` + limitaciones mono/primera pista/secuencial + atribución `MIT © Rikorose/DeepFilterNet`), `CHANGELOG.md` con entrada `1.0.0`.
* T0.3 Crear `Cargo.toml` mínimo: `[package] name="denoise-videos" version="1.0.0" edition="2021" rust-version="1.88"`, `[lib] name="denoise_videos" path="src/lib.rs"`, `[[bin]] name="denoise" path="src/main.rs"`, `[profile.release] opt-level=3, strip=true`, deps `clap 4 + ort 2 (feature download-binaries) + ndarray + rustfft + hound + indicatif + reqwest 0.12 + sha2 + flate2 + tar + home + which + ctrlc + anyhow + thiserror + serde 1 (+derive) + serde_json + log + env_logger`, dev-deps `rand 0.8`. Fuente única `Cargo.toml`, prohibido duplicar versión en código.
* T0.4 Crear esqueleto `src/main.rs` (fino), `src/lib.rs` (re-exporta módulos para `tests/`), `cli.rs`, `pipeline.rs`, `df.rs`, `models.rs`, `ffmpeg_io.rs`, `errors.rs` con `TODO` + `tests/test_naming.rs`, `tests/test_golden.rs`, `tests/test_remux.rs`, `tests/test_errors.rs`, `tests/test_reporter.rs`, `tests/common/si_sdr.rs`, `tests/data/README.md` (contenido real en Fases 1-4).
* T0.5 Verificación: `cargo build --release` + `./target/release/denoise --help` (aunque sea stub) funciona con el repo copiado a otra carpeta con toolchain Rust.

Salida: `cargo test` colecta 0 tests sin error; `RNF-01` verificable.

## Fase 1 — CLI pura sin I/O (lógica testeable sin ffmpeg/red)

Objetivo: cerrar `RF-01/02/03/04 + RF-08(dry-run/reporte puro) + RF-10(help/version)` con `tests/test_naming.rs` en verde. Todo nuevo Rust.

* T1.0 `errors.rs` (primero): enum `E_*` (`E_INVALID_INPUT/E_OUTPUT_EXISTS/E_NO_AUDIO/E_FFMPEG_NOT_FOUND/E_MODEL_MISSING/E_FFMPEG_FAILED/E_IO/E_CANCELLED`) + `exit_code()` (`E_IO→1`) + mensajes accionables con `thiserror` (lib) / `anyhow` (bin). Sin I/O, testeable puro.
* T1.1 `cli.rs: clap` exacto del contrato `docs/nuevo-diseño.md §4`: `INPUT... [-o OUT|--output-dir DIR] [--prefix] [--suffix=_denoised] [--recursive] [--overwrite|--skip-existing] [--audio-bitrate=192] [--model-dir] [--ffmpeg-path] [--dry-run] [--json] [-v] [--version]`. `--overwrite/--skip-existing` mutuamente excluyentes. `--audio-bitrate 64-320`, si no → `E_INVALID_INPUT`.
* T1.2 `cli.rs: expandir_entradas()`: archivos `mp4/mov/mkv/webm/avi` case-insensitive + directorios según `--recursive`, orden alfabético determinista, sin duplicados por absoluto normalizado. Excluir `--output-dir` si está dentro de `INPUT` + aviso `-v`. Ruta inexistente/extensión mala → registra `failed E_INVALID_INPUT`, no aborta lote.
* T1.3 `cli.rs: resolve_output()` pura: precedencia `1) N==1 + -o exacto (crea dirs) > 2) --output-dir/<prefix><stem><suffix>.mp4 recreando árbol si --recursive > 3) junto a original`. Valida `prefix/suffix [A-Za-z0-9._-]`, no ambos vacíos si in-place. Colisión sin `--overwrite` → `E_OUTPUT_EXISTS`; con `--skip-existing` → `skipped`. `-o` a la propia entrada → warning `stderr`.
* T1.4 `cli.rs: --dry-run` tabla `input → output (skip: motivo)` mismo orden del lote real, sin tocar disco/IA. Con `--json` emite `JSONL status="dry-run" pct=0` + `summary`. Sin `--json` humano a `stderr`. `--json` emite `JSONL {input,output,status,message,pct}+{summary:{ok,failed,skipped}}` a `stdout`, humano a `stderr`, sin animación, `pct` según mapeo de `docs/nuevo-diseño.md §8`. `--version` → `denoise-videos 1.0.0 + modelo DFN3 v0.5.6 + ffmpeg <ver>` desde `CARGO_PKG_VERSION` (`ffmpeg missing` si ausente).
* T1.5 `tests/test_naming.rs` (sin ffmpeg/red): 1 archivo, N archivos, `--output-dir`, `prefix/suffix`, `-o` solo `N==1` → error si `N>1`, colisión + `overwrite/skip`, `--recursive` recrea árbol + exclusión output anidado, caracteres inválidos → `E_INVALID_INPUT`, lote vacío → `E_INVALID_INPUT`.

Verificación: `cargo test` verde. Riesgo: ninguno (sin I/O).

## Fase 2 — `ffmpeg_io.rs` + `models.rs` (I/O externo + descarga)

Objetivo: cerrar `RF-06/RF-07 + RNF-05` con `tests/test_errors.rs` parcial (sin DSP).

* T2.1 `ffmpeg_io.rs: find_ffmpeg()`: `--ffmpeg-path → crate which en PATH (PATHEXT en Win) → E_FFMPEG_NOT_FOUND`. Exigir `ffmpeg 6+` vía `ffmpeg -version` con regex `ffmpeg version (\d+)\.` major>=6. Sin `ffprobe`. Documentar receta por OS en `README.md` (Win `winget/choco`, macOS `brew`, Linux `apt`).
* T2.2 `ffmpeg_io.rs: has_audio()/probe()`: `ffmpeg -hide_banner -i`, `Audio:` en `stderr`, regex `Duration:`. Sin audio → `E_NO_AUDIO`.
* T2.3 `ffmpeg_io.rs: extract_mono48k()/remux_copy()`: `ffmpeg -y -v error -i IN -vn -ac 1 -ar 48000 TMP.in.wav` y `ffmpeg -y -v error -i IN -i TMP.out.wav -map 0:v:0 -map 1:a:0 -c:v copy -c:a aac -b:a <bitrate>k -shortest OUT.mp4`. Todo `Command` argv, `from_utf8_lossy`. Error → `E_FFMPEG_FAILED`, sin parciales.
* T2.4 `models.rs: ensure_models()` tras trait `ModelsProvider`: URL `.../DeepFilterNet3_onnx.tar.gz`, `7983136B>=98% + SHA256 registrado (mientras esté pendiente: solo tamaño + warning; tras registro SHA obligatorio)`, miembros `tmp/export/{enc,erb_dec,df_dec}.onnx → dfn3_*.onnx`, `User-Agent: Video-Noise-Remover/1.0.0`, `reqwest+rustls`, `timeout 30s/retry 3`, anti `tar-slip`, chequeo disco, `.part + rename` (Win `remove` previo si existe), extraer + borrar `.tar.gz`. `model-dir` defecto `~/.cache/denoise-videos/models` vía `home`. Fallo red/modelo → `E_MODEL_MISSING` + URL + ruta manual; fallo disco → `E_IO`. Sesiones `ort CPUExecutionProvider` cacheadas + `Mutex`.
* T2.5 Tarea bloqueante para release: en primera descarga con tamaño ok (`>=98%`), calcular `SHA256` real del tarball (`Get-FileHash -Algorithm SHA256` en Win / `sha256sum` en Linux/macOS) y sustituir el pendiente en `docs/especificaciones.md RF-06/RNF-05` + `docs/nuevo-diseño.md §7`; desde entonces SHA obligatorio. Sin este registro no hay tag `v1.0.0`.
* T2.6 `tests/test_errors.rs` (parte 1): sin audio → `E_NO_AUDIO`, ffmpeg ausente → `E_FFMPEG_NOT_FOUND`, descarga rota → `E_MODEL_MISSING`, destino existe → `E_OUTPUT_EXISTS/skipped`, `bitrate 9999` → `E_INVALID_INPUT`, disco/sin permiso → `E_IO`. Nada parcial en disco.

Verificación: `cargo test` verde con ffmpeg 6+ real pero sin ONNX pesado (`FakeProvider` del trait `ModelsProvider`, sin red).

## Fase 3 — `df.rs` DSP + test dorado (corazón numérico)

Objetivo: cerrar `RF-05 + RNF-04` con `tests/test_golden.rs (#[ignore])` bloqueante.

* T3.1 `df.rs` valores idénticos: `SR48000/FFT960/HOP480/ERB32/DF96/ORDER5/LOOKAHEAD2/WNORM=1/(FFT²/2HOP)/ALPHA0.99/LSNR-10/30/20/CHUNK60s/OVERLAP1s`, `_erb_widths()`, `_df_constants()` (vorbis + `erb_fb/erb_inv`), framing `pad HOP + cola FFT+LOOKAHEAD*HOP`, `STFT*ventana*WNORM`, features `ERB mean-norm/40 + unit-norm compleja`, inferencia `ort enc/erb_dec/df_dec`, alineación `k+LOOKAHEAD`, `mask@erb_inv + deep-filter taps k-2..k+2 si lsnr<=20 / intacto si >30 / mute si <-10`, `iSTFT*FFT*ventana + overlap-add + recorte HOP:HOP+n + crossfade`. Firma `denoise_wav(in: &Path, out: &Path, progress: &dyn Fn(usize,usize))`, sin `Command`, solo `ndarray+rustfft+ort+hound`. I/O WAV con `hound` PCM16 ↔ `f32`. Chequeo de cancelación cooperativa entre chunks.
* T3.2 `tests/common/si_sdr.rs` (helper): `SI-SDR` Rust puro (zero-mean, `eps=1e-8`, `10*log10(||s_target||²/||e||²)`).
* T3.3 `tests/data/README.md` + generador determinista (`rand StdRng seed_from_u64(0)` + Box-Muller, `hound`): seno `440Hz 3s` + ruido blanco a `SNR 10dB`, `SR 48k` mono `f32`. Guardar `voz.wav`, `mezcla10dB.wav`, `referencia_dfn3.wav` (generada una vez con el port validado).
* T3.4 `tests/test_golden.rs (#[ignore])`: `denoise_wav(mezcla)`: `mejora >=5dB` y `paridad vs referencia >=60dB` (refs informativas `+5dB` / `20.8dB` / `~77dB`). Desviación = bug bloqueante.

Verificación: `cargo test -- --ignored golden` verde en 1 máquina Win. Riesgo mayor: regresión numérica por offsets/ventana/`f32` — mitigación: no tocar valores.

## Fase 4 — `pipeline.rs` + reporte lote (integración)

Objetivo: cerrar `RF-05/08/09 + RNF-02/03/06` con `tests/test_remux.rs + test_reporter.rs`.

* T4.1 `pipeline.rs: clean_one_video()`: `expandir→validar→resolver salida→has_audio?→modelo?→colisión?→[1-5%] extract→[6-80%] df por chunks con callback (d,t)→[81-95%] remux a OUT.part.mp4→rename (POSIX atómico; Win remove previo si --overwrite)→[96-99%] verificación ligera (>0B+duración±0.5s+AAC)→limpiar temps→[100%] report`. Limpieza garantizada + borrado `.part`; con `-v` conserva temps + muestra comando exacto, `model-dir`, tiempos por fase, `chunks d/t`, tamaños.
* T4.2 Progreso/reporte en `cli.rs` (`log + env_logger` `info`/`-v debug`, `indicatif hidden()` con `--json`/sin TTY): secuencial, un fallo no aborta (salvo `Ctrl+C`: `ctrlc→Child::kill`, borra parcial/temps, `exit 3`, 2º `Ctrl+C` fuerza salida, Win `CREATE_NEW_PROCESS_GROUP`). Humano TTY: cabecera `[i/N] in → out` + 1 barra `indicatif` por video a `stderr` + línea `done|failed|skipped MB+s` + resumen `ok/failed/skipped`. Sin TTY: líneas `%` por fase. `--json`: solo `JSONL`, sin animación. Exit: `3` si cancelado, si no `1` si hubo fallo `1`, si no `2` si hubo fallo `2`, si no `0`.
* T4.3 `tests/test_remux.rs (#[ignore])`: fixture `ffmpeg -y -v error -f lavfi -i testsrc=size=640x480:rate=30:duration=5 -f lavfi -i sine=frequency=440:sample_rate=48000:duration=5 -c:v libx264 -pix_fmt yuv420p -c:a aac -b:a 192k -shortest fixture.mp4` → mismo `vcodec/res/fps`, duración `±0.2s`, `aac 48k bitrate±10%`, sin re-encode: `extradata` igual + `sha256` de `ffmpeg -y -v error -i OUT.mp4 -map 0:v:0 -c copy -f h264 -` idéntico al de entrada (no tamaño fichero).
* T4.4 `tests/test_reporter.rs`: lote 3 simulado verifica orden `[1/3..3/3]`, `JSONL` parseable con `jq`, `summary` correcto, sin animación con `--json`/sin TTY.

Verificación: `cargo test` + `cargo test -- --ignored remux` verdes.

## Fase 5 — E2E v1 + release 1.0.0

Objetivo: cumplir `docs/especificaciones.md §6` y publicar.

* T5.1 Casos borde obligatorios (`docs/especificaciones.md §5`): vertical/4K/`mkv` multi-audio (usa `0:v:0` + primera pista), espacios/acentos/emoji + ruta >150 chars Win, `0B`/imagen renombrada/`bitrate` fuera de rango, disco distinto/sin permiso/`-o` a carpeta inexistente (crearla), lote mixto ok+sin-audio+colisión+inexistente, `Ctrl+C` en extract/inferencia/remux → `exit 3` sin `.part/.wav`.
* T5.2 Manual Win: 1 video corto + lote 5 (`--dry-run` primero, luego real + `--json`), progreso `[i/N]` sin silencio >2s.
* T5.3 `--help` idéntico a contrato `docs/nuevo-diseño.md §4`, `README.md/CHANGELOG.md/LICENSE` finales, `clippy+rustfmt` + `cargo build --release` limpio sin Python.
* T5.4 Tag `v1.0.0`: `cargo test` + `cargo test -- --ignored` verdes, sin código Python en repo.

## Trazabilidad RF → fase/test

| RF | Fase | Test |
|---|---|---|
| RF-01, RF-02, RF-03, RF-04 | 1 | `test_naming` |
| RF-05B | 1+4 | `test_naming` + `test_remux` (bitrate±10%) |
| RF-06 | 2 | `test_errors (E_MODEL_MISSING)` + E2E offline |
| RF-07 | 2 | `test_errors (E_FFMPEG_NOT_FOUND, E_NO_AUDIO)` |
| RF-05 | 3+4 | `test_golden (ignored)` + `test_remux (ignored)` |
| RF-08 | 1+4 | `test_reporter` + lote 5 E2E |
| RF-09 | 4+5 | `test_errors (Ctrl+C simulado, E_IO)` + manual |
| RF-10 | 1+5 | `--help/--version` manual + E2E |
| RNF-04 | 3 | `test_golden mejora>=5dB + paridad>=60dB` bloqueante |
| `E_IO` | 2+4 | `test_errors (disco/permiso)` + E2E casos borde |

## Riesgos principales

1. Regresión numérica DSP Python→Rust (`f32`, `rustfft`, ventanas) → mitigado por Fase 3 bloqueante + valores fijos + vectores `tests/data`.
2. `ffmpeg` Win (`PATH`, espacios, no-latino) + `ort` dylib en Win → mitigado por Fase 2 + matriz manual Win + `ort download-binaries` (sin cmake).
3. `SHA256` pendiente → tarea T2.5 obligatoria antes de release.
4. Alcance: sin `MDX/compress/gif` en v1 (ver `docs/nuevo-diseño.md §9`).
