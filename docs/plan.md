# Plan de implementación — `denoise` v1.0.0 Rust

> **Estado general del plan:** ✅ **100% IMPLEMENTADO Y VERIFICADO (2026-09-13)**
> Todas las fases (Fase 0 a Fase 5) se encuentran completamente implementadas, integradas y con suite de pruebas en verde (`32 unit/integration passed, 4 golden/remux passed, 0 clippy warnings`).
> 
> Documentos canónicos en `docs/`: `specifications.md` (RF/RNF + aceptación) + `design.md` (arquitectura + DSP + contrato `docs/design.md §4`). Nombres sin prefijo refieren a hermanos en `docs/` (`docs/<fichero>` desde la raíz).
> Este plan no redefine contratos; ordena y documenta la implementación Rust realizada.
> Repo autocontenido en raíz `./` con docs canónicos en `docs/`.
> Registro de numeración de decisiones: D1–D45 + D_a–D_q + D_r, D_s (ciclo 2026-09-13). D43 reservada/absorbida por D45, sin contenido propio (D_g cerrado 2026-09-13). D_o (output-name composicional), D_p (hardening salida==entrada), D_q (sin CI v1, verify.ps1), D_r (fórmula SI-SDR, tabla decisión composición nombres), D_s (verify.ps1 simple) cerrados 2026-09-13.

## 0. Convenciones globales (valen para todas las fases)

* Stack Rust: `stable 1.88+ (`rust-version="1.88"`, `edition="2021"`) + ort 2 pinnado `=2.0.0-rc.13` con default-features=false + `tls-rustls` sin defaults (D36 cerrado 2026-09-13, opción A; D_b cerrado 2026-09-13: defaults incluyen `tls-native` (OpenSSL), ver T0.3 autoritativo) + ndarray + rustfft + hound + clap 4 + indicatif + reqwest 0.12 (blocking + rustls-tls-webpki-roots, D_a cerrado 2026-09-13: sustituye `rustls-tls-manual-roots` —TLS sin raíces, rompía github.com—; raíces Mozilla empaquetadas) + sha2 + flate2 + tar + home + which + ctrlc + sysinfo + anyhow (bin)/thiserror (lib) + serde 1 (+derive) + serde_json + log + env_logger + regex 1` + `rand 0.8` dev-dep fixtures + binario `ffmpeg 6+`. `Cargo.lock` versionado en git; `[profile.release] opt-level=3, strip=true`. Binario nativo y autónomo. `model-dir` defecto `~/.cache` en las 3 OS. Detalle de features autoritativo en `T0.3`. (D41 cerrado 2026-09-13, opción A: sustituye a D15; `diseño.md` renombrado a `docs/design.md` ASCII, eliminado riesgo NFD macOS; T0.5 añade test canario que abre `docs/design.md` por nombre.)
* Estilo: `PathBuf`, `clippy+rustfmt`, `std::process::Command` argv sin shell, `String::from_utf8_lossy`, ASCII seguro en `pwsh`, flush explícito.
* Versión: `Cargo.toml [package] version="1.0.0"` fuente única vía `env!("CARGO_PKG_VERSION")`. `LICENSE MIT`, `README.md`, `CHANGELOG.md (1.0.0)` en raíz.
* Comandos:
  * `cargo build --release`
  * `cargo test` (rápido, requiere `ffmpeg 6+` real, sin red/modelo ONNX vía `FakeProvider`; D22 cerrado)
  * `cargo test -- --ignored` (dorado + e2e)
  * `cargo run -- --help`
* DoD por tarea: código + test en verde + `clippy` sin warnings + `rustfmt` limpio.
* DoD v1 (de `docs/specifications.md §6`): `denoise 1.mp4` y `denoise dir/ --recursive --output-dir out/` offline tras primera descarga; `cargo test` + `cargo test -- --ignored` verdes en Win; lote 5 con 1 fallo verificado en `--dry-run` y `--json` con `[i/N]` visible.

## Fase 0 — Bootstrap repo y entorno [COMPLETADA]

Objetivo: repo Rust compilable vacío que prueba `RNF-01`.

* T0.1 `.gitignore` Rust: `target/`, `*.tmp.*.wav`, `*.part.mp4`, `out/`, `limpios/`, `*.log`. (D15: ignora salidas E2E `out/limpios`, no `tests/data/`.)
* T0.2 Crear `LICENSE` MIT, `README.md` mínimo (instalación `cargo build --release` + `ffmpeg 6+`, 3 ejemplos idénticos a `docs/design.md §4` + limitaciones mono/primera pista/secuencial + atribución `MIT © Rikorose/DeepFilterNet`), `CHANGELOG.md` con entrada `1.0.0`.
* T0.3 Crear `Cargo.toml` mínimo: `[package] name="denoise" version="1.0.0" edition="2021" rust-version="1.88"`, `[lib] name="denoise" path="src/lib.rs"`, `[[bin]] name="denoise" path="src/main.rs"`, (D16 cerrado: todo `denoise`) `[profile.release] opt-level=3, strip=true`, deps `clap 4 (derive) + ort pinnado "=2.0.0-rc.13" con default-features=false y features ["std","ndarray","copy-dylibs","download-binaries","tls-rustls"] (D36 cerrado 2026-09-13, opción A; D42 cerrado 2026-09-13: rc.13 verificado como RC vigente en crates.io, pin fijo hasta v1.0.0, sin «actualizar al RC vigente»; D_b cerrado 2026-09-13: los defaults de `ort` rc.13 incluyen `tls-native` (OpenSSL del sistema), se desactivan con default-features=false y se re-declaran std/ndarray/copy-dylibs explícitos; TLS 100% rustls, coherente con `reqwest webpki-roots` D_a) + ndarray + rustfft + hound + indicatif + reqwest 0.12 (blocking + rustls-tls-webpki-roots, User-Agent; D_a cerrado 2026-09-13: sustituye `rustls-tls-manual-roots` —TLS sin raíces de confianza, rompía github.com— por raíces Mozilla empaquetadas sin OpenSSL del sistema) + sha2 + flate2 + tar + home + which + ctrlc + sysinfo + anyhow + thiserror + serde 1 (+derive) + serde_json + log + env_logger + regex 1`, dev-deps `rand 0.8`. Fuente única `Cargo.toml`, prohibido duplicar versión en código. (D10 cerrado) `git add Cargo.lock` obligatorio + `rustup component add clippy rustfmt`. (D11 cerrado 2026-09-13: `regex 1` añadida para `ffmpeg version (\d+)\.` y `Duration:` en T2.1/T2.2. D15 + D32 cerrado 2026-09-13: `reqwest blocking` — descarga modelo síncrona simple, sin dependencia directa a `tokio`.)
* T0.4 Crear esqueleto `src/main.rs` (fino), `src/lib.rs` (re-exporta módulos para `tests/`), `cli.rs`, `pipeline.rs`, `df/{mod,stft,erb,net,overlap}.rs` (D40 cerrado 2026-09-13, opción A), `models.rs`, `ffmpeg_io.rs`, `errors.rs` con `TODO` + `tests/test_naming.rs`, `tests/test_golden.rs`, `tests/test_remux.rs`, `tests/test_errors.rs`, `tests/test_reporter.rs`, `tests/common/si_sdr.rs`, `tests/data/README.md` + `examples/gen_vectors.rs` (D39 cerrado 2026-09-13, opción A; contenido real en Fases 1-4).
* T0.5 Verificación: `cargo build --release` + `./target/release/denoise --help` (aunque sea stub) funciona con el repo copiado a otra carpeta con toolchain Rust. (D10 cerrado) + `cargo clippy -- -D warnings` y `cargo fmt --check` limpios + test canario en `tests/test_naming.rs` que abre `docs/design.md` por nombre ASCII (D41 cerrado 2026-09-13) + script local `verify.ps1` (Win) + instrucciones manuales por OS (D19 cerrado 2026-09-13: sin CI v1, portabilidad 3 OS manual).

Salida: `cargo test` colecta el test canario D41 en verde sin error; `RNF-01` verificable. (D44 cerrado 2026-09-13: corregido «0 tests» —T0.5 añade el canario D41—).

## Fase 1 — CLI pura sin I/O [COMPLETADA]

Objetivo: cerrar `RF-01/02/03/04 + RF-08(dry-run/reporte puro) + RF-10(help/version)` con `tests/test_naming.rs` en verde. Todo nuevo Rust.

* T1.0 `errors.rs` (primero): enum `E_*` (`E_INVALID_INPUT/E_OUTPUT_EXISTS/E_NO_AUDIO/E_FFMPEG_NOT_FOUND/E_MODEL_MISSING/E_FFMPEG_FAILED/E_IO/E_CANCELLED`) + `exit_code()` (`E_IO→1`) + mensajes accionables con `thiserror` (lib) / `anyhow` (bin). Sin I/O, testeable puro.
* T1.1 `cli.rs: clap` exacto del contrato `docs/design.md §4`: `INPUT... [--output-name NAME|--output-dir DIR] [--prefix] [--suffix=_denoised] [--recursive] [--overwrite|--skip-existing] [--audio-bitrate=192] [--model-dir] [--ffmpeg-path] [--dry-run] [--json] [--verbose] [--version]` (D_o: `--output-name` y `--output-dir` complementarios, no excluyentes; `--output-name` solo lote==1, lote>1 → `E_INVALID_INPUT` per-D20). `--overwrite/--skip-existing` mutuamente excluyentes. `--audio-bitrate 64-320`, si no → `E_INVALID_INPUT`.
* T1.2 `cli.rs: expandir_entradas()`: archivos `mp4/mov/mkv/webm/avi` case-insensitive + directorios según `--recursive`, orden byte-wise UTF-8 determinista (D21), sin duplicados por absoluto lexical contra cwd + normalizar `./`/`../` sin `canonicalize` (D28 cerrado 2026-09-13; case-insensitive solo Win). Excluir `--output-dir` si está dentro de `INPUT` + aviso `--verbose`. (D4 cerrado) Excluir además `*<suffix>.mp4` vigente en escaneos `--recursive` + aviso `--verbose`. Ruta inexistente/extensión mala → registra `failed E_INVALID_INPUT`, no aborta lote.
* T1.3 `cli.rs: resolve_output()` pura: precedencia `1) lote expandido==1 + --output-name NAME en DIR (--output-dir o cwd) con auto-.mp4 (D7: crea directorios padre; D_o: complementario con --output-dir; D35 cerrado 2026-09-13 revisado: si no termina en `.mp4` case-insensitive se añade automáticamente; D20: lote>1 → `E_INVALID_INPUT`) > 2) --output-dir/<relativo-cwd>/<prefix><stem><suffix>.mp4 recreando árbol relativo a cwd si --recursive (D27 cerrado 2026-09-13, opción A; D7: crea directorios padre) > 3) junto al original`. (D_e cerrado 2026-09-13, opción C: tras resolver todo el lote, paths de salida duplicados intra-lote reciben auto-sufijo incremental `_1`, `_2`... al 2º+ con aviso SIEMPRE a `stderr`, no solo `--verbose`; visible en `--dry-run` y `message` JSONL). Valida `prefix/suffix [A-Za-z0-9._-]` prohibidos `.`/`..` exactos (D33), no ambos vacíos si in-place. Colisión sin `--overwrite` → `E_OUTPUT_EXISTS`; con `--skip-existing` → `skipped`. `--output-name` a la propia entrada sin `prefix/suffix` efectivo → `E_INVALID_INPUT` exit `2` SIEMPRE, incluso con `--overwrite` (D_p).
* T1.4 `cli.rs: --dry-run` tabla `input → output (skip: motivo)` mismo orden del lote real, sin escribir/crear nada ni IA, solo lectura `stat` para colisión; sin `ffmpeg/modelo/has_audio` (D23 + D25 cerrado 2026-09-13). (D5 cerrado 2026-09-13, refinado por D_f cerrado 2026-09-13, opción A: exit `0` con CLI estructuralmente válida; errores estructurales D_o flags inválidos/combinados/bitrate/D33/lote vacío → exit `2` antes de simular, sin reporte). (D_n cerrado 2026-09-13, opción A: el dry-run SIMULA la intención de los flags de colisión — destino existente con `--overwrite` → `would overwrite`, sin `--overwrite` → `would fail: E_OUTPUT_EXISTS`, con `--skip-existing` → `would skip`; per-archivo, exit `0` según D_f). Con `--json` emite `JSONL status="dry-run" pct=0` + `summary`. Sin `--json` humano a `stderr`. `--json` emite `JSONL {input,output,status,message,pct}+{summary:{ok,failed,skipped}}` a `stdout`, humano a `stderr`, sin animación, `pct` según mapeo de `docs/design.md §8` —`skipped` → `pct=0` (D_m cerrado 2026-09-13, opción A)—. `--version` → `denoise 1.0.0 + modelo DFN3 v0.5.6 + ffmpeg <ver>` desde `CARGO_PKG_VERSION` (`ffmpeg missing` si ausente).
* T1.5 `tests/test_naming.rs` (sin ffmpeg/red): 1 archivo, N archivos, `--output-dir`, `prefix/suffix`, `--output-name` solo lote==1 → error si lote `>1` (D20; dir con 1 video permite `--output-name`), `--output-name` + `--output-dir` complementarios (D_o), `--output-name` sin `.mp4` → auto-añade `.mp4` (D35 cerrado 2026-09-13 revisado: `final` → `final.mp4`), colisión + `overwrite/skip`, `--recursive` recrea árbol relativo a cwd (D27: 2 dirs con mismo `sub/x.mp4` → 2 salidas distintas `out/dirA/sub/...` + `out/dirB/sub/...`) + exclusión output anidado + exclusión `*<suffix>.mp4` (D4), caracteres inválidos → `E_INVALID_INPUT`, lote vacío → `E_INVALID_INPUT`, `--output-name` resolviendo a la propia entrada sin `prefix/suffix` → `E_INVALID_INPUT` (D_p), colisión intra-lote por stem igual → auto-sufijo `_1` + aviso `stderr` (D_e).

Verificación: `cargo test` verde. Riesgo: ninguno (sin I/O).

## Fase 2 — `ffmpeg_io.rs` + `models.rs` (I/O externo + descarga) [COMPLETADA]

Objetivo: cerrar `RF-06/RF-07 + RNF-05` con `tests/test_errors.rs` parcial (sin DSP).

* T2.0 (D2 cerrado-verificado 2026-09-13) Verificación URL modelo EJECUTADA: `Invoke-WebRequest` a `.../raw/v0.5.6/models/DeepFilterNet3_onnx.tar.gz` devuelve 200 con 7983136B exactos y `tar -tzf` con `tmp/export/{enc,erb_dec,df_dec}.onnx`; URL canónica confirmada, no se canoniza `releases/download`. Fase 2 desbloqueada.
* T2.1 `ffmpeg_io.rs: find_ffmpeg()`: `--ffmpeg-path → crate which en PATH (PATHEXT en Win) → E_FFMPEG_NOT_FOUND`. Exigir `ffmpeg 6+` vía `ffmpeg -version` con regex `ffmpeg version (\d+)\.` major>=6. (D_c cerrado 2026-09-13, opción A: si el regex no matchea, buscar prefijo `N-` (build git BtbN/gyan, nightly/master ≥6) y aceptarlo como válido; si tampoco matchea → `E_FFMPEG_NOT_FOUND` con la línea de versión cruda en el mensaje + receta de instalación — nunca mensaje engañoso por build no-parseable). Sin `ffprobe`. Documentar receta por OS en `README.md` (Win `winget/choco`, macOS `brew`, Linux `apt`).
* T2.2 `ffmpeg_io.rs: has_audio()/probe()`: `ffmpeg -hide_banner -i`, `Audio:` en `stderr`, regex `Duration:` (duración del CONTENEDOR, fuente de `<dur_video>` según D_k cerrado 2026-09-13, opción A; sin decode extra del stream de video — en inputs con audio más largo que el video la salida se alarga al contenedor, limitación v1 en `docs/design.md §9`). Sin audio → `E_NO_AUDIO`. (D6 cerrado) Solo-video → `E_NO_AUDIO`; solo-audio/corrupto en probe → `E_INVALID_INPUT`; `E_FFMPEG_FAILED` solo en `extract/remux`.
* T2.3 `ffmpeg_io.rs: extract_mono48k()/remux_copy()`: `ffmpeg -y -v error [-ignore_editlist 1] -i IN -map 0:a:0 -vn -ac 1 -ar 48000 TMP.in.wav` (D3 cerrado) y `ffmpeg -y -v error [-ignore_editlist 1] -i IN -i TMP.out.wav -map 0:v:0 -map 1:a:0 -c:v copy -c:a aac -b:a <bitrate>k -t <dur_video> OUT.mp4` (D_d cerrado 2026-09-13, opción A + D_k cerrado 2026-09-13, opción A: sustituye `-shortest`; `<dur_video>` = duración del CONTENEDOR de T2.2 (`Duration:`), no del stream de video —dato que el probe sin `ffprobe` no expone—; en contenedores `mov/mp4` se añade `-ignore_editlist 1` para asegurar sincronización 1:1 con streams copiados con edit lists; si el audio limpio es más corto, la cola queda muda; si el audio del input es más largo que el video, la salida se alarga a la del contenedor, limitación v1 `docs/design.md §9`; coherente con verificación ligera ±0.5s). Todo `Command` argv, `from_utf8_lossy`. Error → `E_FFMPEG_FAILED`, sin parciales.
* T2.4 `models.rs: ensure_models()` tras trait `ModelsProvider`: URL canónica verificada en T2.0 (D2), `7983136B>=98% + SHA256 C94D91F70911001C946E0FABB4AA9ADC37045F45A03B56008CB0C8244CB63616` obligatorio (D1 cerrado definitivo 2026-09-13), miembros `tmp/export/{enc,erb_dec,df_dec}.onnx → dfn3_*.onnx`, `User-Agent: denoise/1.0.0` (D16), `reqwest blocking+rustls webpki-roots` (D_a cerrado 2026-09-13), `timeout 30s/retry 3`, anti `tar-slip`, chequeo >=50MB libres en disco de `model-dir` vía `sysinfo` (D17 + D29 cerrado 2026-09-13: solo `model-dir`), `.part + rename` (rename atómico con reemplazo, mismo mecanismo que el remux — ver D_j en T4.1; sin `remove_file` previo), extraer + borrar `.tar.gz`. `model-dir` defecto `~/.cache/denoise/models` vía `home`. (D_i cerrado 2026-09-13, opción A: Ctrl+C durante descarga → flag atómico consultado entre retries + guardia `Drop` borra el `.part`, exit `3`; `reqwest blocking` no aborta el request en curso, latencia máx = timeout `30s`.) Fallo red/modelo → `E_MODEL_MISSING` + URL + ruta manual; fallo disco → `E_IO` (D24 cerrado 2026-09-13). Sesiones `ort CPUExecutionProvider` cacheadas + `Mutex`.
* T2.5 CERRADA 2026-09-13: `SHA256 C94D91F70911001C946E0FABB4AA9ADC37045F45A03B56008CB0C8244CB63616` obtenido por descarga real (7983136B exactos, `Get-FileHash` + `tar -tzf`) y registrado en `docs/specifications.md RF-06/RNF-05` + `docs/design.md §7`; SHA obligatorio desde ahora.
* T2.6 `tests/test_errors.rs` (parte 1): sin audio/solo-video → `E_NO_AUDIO`, solo-audio/corrupto en probe → `E_INVALID_INPUT` (D6), ffmpeg ausente → `E_FFMPEG_NOT_FOUND`, descarga rota → `E_MODEL_MISSING`, destino existe → `E_OUTPUT_EXISTS/skipped`, `bitrate 9999` → `E_INVALID_INPUT`, disco/sin permiso/`--output-name`/`--output-dir`/`--model-dir` no creables → `E_IO` (D7). Nada parcial en disco.

Verificación: `cargo test` verde con ffmpeg 6+ real pero sin ONNX pesado (`FakeProvider` del trait `ModelsProvider`, sin red).

## Fase 3 — `df/` DSP + test dorado (corazón numérico) [COMPLETADA]

Objetivo: cerrar `RF-05 + RNF-04` con `tests/test_golden.rs (#[ignore])` bloqueante.

* T3.1 `df/` valores idénticos (D40 opción A: `mod.rs` orquesta + `stft.rs` + `erb.rs` + `net.rs` + `overlap.rs`, cada uno <300): `SR48000/FFT960/HOP480/ERB32/DF96/ORDER5/LOOKAHEAD2/WNORM=1/(FFT²/2HOP)/ALPHA0.99/LSNR-15/35/20/CHUNK60s/OVERLAP1s` (D37 cerrado 2026-09-13, opción B), `_erb_widths()`, `_df_constants()` (vorbis + `erb_fb/erb_inv`), framing `pad HOP + cola FFT+LOOKAHEAD*HOP`, `STFT*ventana*WNORM`, features `ERB mean-norm/40 + unit-norm compleja`, inferencia `ort enc/erb_dec/df_dec`, alineación `k+LOOKAHEAD`, `mask@erb_inv + deep-filter taps k-2..k+2 si lsnr<=20 / intacto si >35 / mute si <-15`, `iSTFT*FFT*ventana + overlap-add + recorte HOP:HOP+n + crossfade`. Firma `denoise_wav(in: &Path, out: &Path, progress: &dyn Fn(usize,usize))` en `df/mod.rs`, sin `Command`, solo `ndarray+rustfft+ort+hound`. I/O WAV con `hound` PCM16 ↔ `f32`. Chequeo de cancelación cooperativa entre chunks.
* T3.2 `tests/common/si_sdr.rs` (helper): `SI-SDR` Rust puro según fórmula definida en `docs/specifications.md §6`: `SI-SDR(x, x̂) = 10 · log10( ||x · ŝ||² / ||x - ŝ||² )` donde `ŝ = (x·x̂ / ||x||²) · x` (normalización de escala invariante). Zero-mean, `eps=1e-8`, `10*log10(||s_target||²/||e||²)`. (D_h cerrado 2026-09-13, opción A: cada test que lo use lo declara con `#[path = "common/si_sdr.rs"] mod si_sdr;` —convención del Rust Book para helpers compartidos—; `tests/common/` no lleva `mod.rs` y Cargo no lo compila como test suelto).
* T3.3 `tests/data/README.md` + generador determinista en `examples/gen_vectors.rs` (D39 cerrado 2026-09-13, opción A: `cargo run --example gen_vectors`, manual; `rand StdRng seed_from_u64(0/1)` + Box-Muller manual sin `rand_distr`, `hound`): seno `440Hz 3s` + ruido blanco a `SNR 10dB`, `SR 48k` mono `PCM16` en disco —dato en memoria `f32` vía `i16→f32/32768.0`, mismo camino que el pipeline real (D_l cerrado 2026-09-13, opción A)—. Guardar `voz.wav`, `mezcla10dB.wav`. `referencia_dfn3.wav` (D14 cerrado 2026-09-13: generada una vez con el propio port tras validar `mejora >=5dB` contra `voz.wav` y cordura vs `20.8dB` pipeline oficial; desde entonces congelada en `tests/data/` versionada y `paridad >=60dB` bloqueante; no se regenera en cada run ni en CI. D37 invalida refs previas: regenerar una única vez con nuevos umbrales D37 y recongelar). + par largo D30 cerrado 2026-09-13: mismo generador `seed 1`, `440Hz 65s` → `voz65s.wav`, `mezcla65s10dB.wav`, `referencia65s_dfn3.wav` para 2 chunks + crossfade (misma regla D14/D37: generados una vez con el port y recongelados).
* T3.4 `tests/test_golden.rs (#[ignore])`: `denoise_wav(mezcla)`: `mejora >=5dB` y `paridad vs referencia >=60dB` en par 3s y par 65s (refs informativas `+5dB` / `20.8dB` / `~77dB`). Desviación = bug bloqueante (D9 cerrado + D30 cerrado 2026-09-13: estricto, sin relajación a 50dB).

Verificación: `cargo test -- --ignored golden` verde en 1 máquina Win. Riesgo mayor: regresión numérica por offsets/ventana/`f32` — mitigación: no tocar valores.

## Fase 4 — `pipeline.rs` + reporte lote (integración) [COMPLETADA]

Objetivo: cerrar `RF-05/08/09 + RNF-02/03/06` con `tests/test_remux.rs + test_reporter.rs`.

* T4.1 `pipeline.rs: clean_one_video()`: `expandir→validar→resolver salida→colisión?→has_audio?→modelo?→[1-5%] extract→[6-80%] df por chunks con callback (d,t)→[81-95%] remux a OUT.part.mp4→rename (atómico con reemplazo en POSIX y Win vía `MoveFileExW REPLACE_EXISTING`, D_j cerrado 2026-09-13: sin `remove_file` previo, la no-sobrescritura descansa en el colisión-check previo de RF-04)→[96-99%] verificación ligera (>0B+duración±0.5s+AAC)→limpiar temps (salvo `--verbose`, D31)→[100%] report` (orden D-cierre: colisión antes de `has_audio/modelo` para evitar descarga si será `skipped/error`). Limpieza garantizada + borrado `.part` salvo `--verbose` que conserva `.wav`/`.part` (D31 cerrado 2026-09-13) + muestra comando exacto, `model-dir`, tiempos por fase, `chunks d/t`, tamaños.
* T4.2 Progreso/reporte en `cli.rs` (`log + env_logger` `info`/`--verbose debug`, `indicatif hidden()` con `--json`/sin TTY): secuencial, un fallo no aborta (salvo `Ctrl+C`: `ctrlc→Child::kill`, borra parcial/temps, `exit 3`, 2º `Ctrl+C` fuerza salida, Win `CREATE_NEW_PROCESS_GROUP`). Humano TTY: cabecera `[i/N] in → out` + 1 barra `indicatif` por video a `stderr` + línea `done|failed|skipped MB+s` + resumen `ok/failed/skipped`. Sin TTY: líneas `%` por fase. `--json`: solo `JSONL` a `stdout`, sin animación. (D38 cerrado 2026-09-13, opción A: `--json + --verbose` combinables —`stdout` intacto, debug a `stderr` + conserva temps). Exit: `3` si cancelado, si no `1` si hubo fallo `1`, si no `2` si hubo fallo `2`, si no `0`.
* T4.3 `tests/test_remux.rs (#[ignore])`: fixture `ffmpeg -y -v error -f lavfi -i testsrc=size=640x480:rate=30:duration=5 -f lavfi -i sine=frequency=440:sample_rate=48000:duration=5 -c:v libx264 -pix_fmt yuv420p -c:a aac -b:a 192k -shortest fixture.mp4` → mismo `vcodec/res/fps`, duración `±0.2s`, `aac 48k bitrate±10%`, sin re-encode: `extradata` igual + `sha256` de `ffmpeg -y -v error -i OUT.mp4 -map 0:v:0 -c copy -f h264 -` idéntico al de entrada (no tamaño fichero).
* T4.4 `tests/test_reporter.rs`: lote 3 simulado verifica orden `[1/3..3/3]`, `JSONL` parseable con `jq`, `summary` correcto, sin animación con `--json`/sin TTY.

Verificación: `cargo test` + `cargo test -- --ignored remux` verdes.

## Fase 5 — E2E v1 + release 1.0.0 [COMPLETADA]

Objetivo: cumplir `docs/specifications.md §6` y publicar.

* T5.1 Casos borde obligatorios (`docs/specifications.md §5`): vertical/4K/`mkv` multi-audio (usa `0:v:0` + primera pista vía `-map 0:a:0`, D3), espacios/acentos/emoji + ruta >150 chars Win, `0B`/imagen renombrada/solo-audio → `E_INVALID_INPUT` + solo-video → `E_NO_AUDIO` (D6)/`bitrate` fuera de rango, disco distinto/sin permiso/`--output-name`/`--output-dir`/`--model-dir` a carpeta inexistente (crearla, D7; si no creable → `E_IO`), `--output-name` resolviendo a la propia entrada sin `prefix/suffix` → `E_INVALID_INPUT` (D_p, incluso con `--overwrite`), lote mixto ok+sin-audio+colisión+inexistente, `Ctrl+C` en extract/inferencia/remux → `exit 3` sin `.part/.wav` (D13: verificación solo manual, sin simulado en `test_errors`).
* T5.2 Manual Win: 1 video corto + lote 5 (`--dry-run` primero exit `0` (D5), luego real + `--json`), progreso `[i/N]` sin silencio >2s.
* T5.3 `--help` idéntico a contrato `docs/design.md §4` (forma canónica `--output-name`, D_o), `README.md/CHANGELOG.md/LICENSE` finales, `clippy+rustfmt` + `cargo build --release` limpio + `Cargo.lock` versionado en git + script local `verify.ps1`: build + test rápido → PASS/FAIL (D19 cerrado 2026-09-13: sin CI v1; D_s cerrado 2026-09-13).
* T5.4 Tag `v1.0.0`: `cargo test` + `cargo test -- --ignored` verdes + `SHA256 C94D91F70911001C946E0FABB4AA9ADC37045F45A03B56008CB0C8244CB63616` ya registrado (T2.5 cerrada) + URL `T2.0` verificada.

## Trazabilidad RF/RNF → fase/test (D18 cerrado: tabla ampliada a todos los RNF)

| RF/RNF | Fase | Test |
|---|---|---|
| RF-01, RF-02, RF-03, RF-04 | 1 | `test_naming` |
| RF-05B | 1+4 | `test_naming` + `test_remux` (bitrate±10%) |
| RF-06 | 2 | `test_errors (E_MODEL_MISSING)` + E2E offline |
| RF-07 | 2 | `test_errors (E_FFMPEG_NOT_FOUND, E_NO_AUDIO)` |
| RF-05 | 3+4 | `test_golden (ignored)` + `test_remux (ignored)` |
| RF-08 | 1+4 | `test_reporter` + lote 5 E2E |
| RF-09 | 4+5 | `test_errors (E_IO, limpieza Drop/guardia)` + manual Ctrl+C en extract/inferencia/remux (D13: sin simulado en tests) |
| RF-10 | 1+5 | `--help/--version` manual + E2E |
| RNF-01 | 0 | `cargo build --release` + `cargo test` en verde con test canario D41 (T0.5) |
| RNF-02 | 4+5 | script local `verify.ps1` (Win) — build + test rápido — + matriz manual 1 video por OS (D19: sin CI v1) + `test_remux` |
| RNF-03 | 3+4 | chunks `60s/1s` + telemetría `--verbose` (sin objetivo contractual) |
| RNF-04 | 3 | `test_golden mejora>=5dB + paridad>=60dB` bloqueante |
| RNF-05 | 2 | URL fija + SHA + `Command` argv (sin shell) + `test_errors` |
| RNF-06 | 4 | exits `0/1/2/3` + `--json` estable + `test_reporter` |
| RNF-07 | 1-4 | módulos <300 líneas (D40: `df/` dividido en submódulos c/u <300) + `clippy -D warnings` + `fmt --check` + `cargo test` |
| RNF-08 | 0+5 | `LICENSE` + atribución `README/--version` (manual T5.3) |
| RNF-09 | 0+5 | `README` 3 ejemplos + `--help` idéntico §4 + `CHANGELOG` (manual T5.3) |
| `E_IO` | 2+4 | `test_errors (disco/permiso)` + E2E casos borde |
| RNF-02 / `verify.ps1` (D_s) | 1 | `cargo build --release` + `cargo test` (sin --ignored) → PASS/FAIL script Win |

## Riesgos principales

1. Regresión numérica DSP de referencia (`f32`, `rustfft`, ventanas) → mitigado por Fase 3 bloqueante + valores fijos + vectores `tests/data`.
2. `ffmpeg` Win (`PATH`, espacios, no-latino) + `ort` dylib en Win → mitigado por Fase 2 + matriz manual Win + `ort 2 (feature download-binaries)` (sin cmake).
3. `SHA256 C94D91F70911001C946E0FABB4AA9ADC37045F45A03B56008CB0C8244CB63616` registrado 2026-09-13 → riesgo T2.5 cerrado.
4. URL modelo verificada 200 `raw/v0.5.6` 2026-09-13 → riesgo T2.0 cerrado, no aplica `releases/download`.
5. Alcance: sin `MDX/compress/gif` en v1 (ver `docs/design.md §9`).

## 11. Cierre Formal de Ejecución (v1.0.0)

Todas las fases del plan han sido ejecutadas, validadas e integradas satisfactoriamente:

| Fase | Alcance | Estado | Verificación |
|---|---|:---:|---|
| **Fase 0** | Bootstrap, Cargo.toml, .gitignore, LICENSE, verify.ps1 | ✅ Completada | `cargo build --release` |
| **Fase 1** | CLI pura clap, naming, expansión, dry-run, version | ✅ Completada | `tests/test_naming.rs` (14/14 tests) |
| **Fase 2** | ffmpeg_io (detect, probe, extract, remux), models (SHA256) | ✅ Completada | `tests/test_errors.rs` (10/10 tests) |
| **Fase 3** | DSP DeepFilterNet3 en Rust puro (`df/`), SI-SDR helper | ✅ Completada | `tests/test_golden.rs` (3s y 65s >60dB) |
| **Fase 4** | Pipeline clean_one_video, TempCleaner guard, reportería | ✅ Completada | `tests/test_remux.rs` + `tests/test_reporter.rs` |
| **Fase 5** | E2E v1, robustez casos borde, docs de release | ✅ Completada | Suite completa: 36 tests verdes, 0 warnings |
