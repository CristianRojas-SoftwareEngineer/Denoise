# Diseño — CLI autocontenida `denoise` v1 Rust

> Alcance: única funcionalidad — eliminar ruido de la pista de audio de uno o varios videos.
> Repo autocontenido: este repo (`Video-Noise-Remover/` raíz). Sin runtime Python.
> Documentos canónicos en `docs/`: este diseño + `specifications.md`. En caso de divergencia, el contrato CLI de §4 manda.
> Convención de rutas: `docs/<fichero>` es relativo a la raíz del repo.

## 1. Objetivo y no-objetivos

**Objetivo:** CLI `denoise` que recibe 1..N videos, les quita ruido de fondo/hiss/ambiente de la voz y entrega nuevos `.mp4` con el video intacto.

**No-objetivos explícitos:**

* Sin servidor web, sin desktop app, sin base de datos de tareas, sin polling.
* Sin `Vocal Remover / Mastering / Tag Editor / Compresor / GIF / Upscale / Restauración facial`.
* Sin separación MDX (`Kim_Vocal_2`). Solo `denoise` voz.
* Sin concurrencia masiva. Batch secuencial, determinista.

## 2. Principios de diseño

1. **Autocontenida Rust:** `std + ort 2 pinnado con `tls-rustls` (D36 cerrado 2026-09-13, opción A; D42 cerrado 2026-09-13: pin fijo `=2.0.0-rc.13`, RC vigente verificado, sin «actualizar») + ndarray + rustfft + hound + clap 4 + indicatif + reqwest 0.12 (blocking + rustls-tls-manual-roots) + serde 1 (+derive)/serde_json + log + env_logger + which + regex 1 + sysinfo + ffmpeg` binario (+ `sha2/flate2/tar/home/ctrlc/anyhow(thiserror en lib)`, `rand 0.8` solo dev; `rust-version="1.88"`, `edition="2021"`, `Cargo.lock` versionado en git, `[profile.release] opt-level=3, strip=true`; lista completa en `docs/specifications.md RNF-01`, detalle de features autoritativo en `docs/plan.md T0.3`). Sin `Python/torch/librosa/Flask/pillow/opencv`. (D17: `sysinfo` para disco).
2. **No reinventar DSP:** constantes y orden de operaciones de DeepFilterNet3 se copian exactos según §6. El riesgo es regresión numérica.
3. **Video nunca se re-codifica:** `-c:v copy`. Solo el audio se procesa.
4. **Fallo explícito y limpio:** exit codes, temps siempre borrados, `String::from_utf8_lossy` en Windows.
5. **CLI predecible para batch y scripting:** `--dry-run`, `--json`, `--skip-existing`, lote secuencial.
6. **Implementación legible Rust:** `stable 1.88+ edition 2021`, `PathBuf`, `clippy+rustfmt`, `anyhow` en bin / `thiserror` en lib, `log + env_logger` (`info`/`-v debug`), funciones pequeñas, sin estado global salvo sesiones `ort` cacheadas (`Mutex`).
7. **Testing por pirámide:** unitarias rápidas con `ffmpeg 6+` real sin red/modelo (`cargo test`, D22), doradas DSP bloqueantes + e2e `remux` con `#[ignore]` (slow) vía `cargo test -- --ignored`.
8. **Documentación como contrato:** `README.md` (MIT + atribución DFN3) + `--help` + ejemplos idénticos a §4; `CHANGELOG.md` por release (inicial `1.0.0`); `LICENSE` MIT en raíz.

## 3. Estructura mínima propuesta (todo dentro de este repo, raíz `./`)

```
./  (Video-Noise-Remover/)
  docs/
    design.md            # este documento (arquitectura + pipeline)
    specifications.md        # RF/RNF canónicos + aceptación
    plan.md                    # orden de implementación
  LICENSE                  # MIT (código nuevo)
  README.md                  # instalación, 3 ejemplos, atribución MIT © Rikorose/DeepFilterNet
  CHANGELOG.md               # por release (inicial 1.0.0)
  Cargo.toml                 # package/bin `denoise`, version="1.0.0" fuente única, edition 2021, rust-version 1.88, [profile.release] (D16: todo `denoise`)
  Cargo.lock                 # versionado en git
  src/
    main.rs                  # bin denoise fino (llama a lib)
    lib.rs                   # lib denoise, re-exporta cli/pipeline/df/models/ffmpeg_io/errors
    cli.rs                   # clap, expansión entradas, naming salida, reporte lote
    pipeline.rs              # clean_one_video(): extract → denoise → remux + verificación
    df/                       # DSP DeepFilterNet3 + inferencia ort (puro, sin I/O proceso) — D40 opción A
      mod.rs                 # orquestación por chunks + firma denoise_wav() + cancelación cooperativa
      stft.rs                # framing streaming + STFT/iSTFT vorbis + WNORM + overlap-add
      erb.rs                 # _erb_widths() + _df_constants() + erb_fb/erb_inv + features mean-norm
      net.rs                 # sesiones ort enc/erb_dec/df_dec cacheadas (Mutex) + inferencia
      overlap.rs             # crossfade lineal entre chunks 60s/1s + recorte HOP:HOP+n
    models.rs                # descarga/cache/verificación de dfn3_*.onnx
    ffmpeg_io.rs             # localización ffmpeg, has_audio, extract, remux, verify
    errors.rs                # E_* + exit codes
  tests/
    test_naming.rs           # reglas prefix/suffix/output-dir/-o (sin ffmpeg/red)
    common/si_sdr.rs         # helper SI-SDR Rust puro
    test_golden.rs           # tono 10dB ruido blanco → mejora SI-SDR (#[ignore] slow)
    test_remux.rs            # codec/res/fps/duración/bitrate, sin re-encode (#[ignore] slow)
    test_errors.rs           # E_* + overwrite/skip (D13: Ctrl+C no se simula en tests, solo manual T5.1)
    test_reporter.rs         # orden [i/N], JSONL parseable, summary ok/failed/skipped
    data/README.md           # spec vectores sintéticos + generador determinista (seed 0/1, D45 cerrado 2026-09-13, parte b: 0=par 3s, 1=par 65s D30)
    data/*.wav               # vectores versionados (D39: nunca se regeneran en CI)
  examples/
    gen_vectors.rs           # generador determinista seed 0/1 (D39 cerrado 2026-09-13, opción A: `cargo run --example gen_vectors`, solo manual; no corre en `cargo test`)
```

## 4. Contrato CLI (v1)

```text
denoise INPUT... [-o/--output OUT | --output-dir DIR] [--prefix STR] [--suffix STR]
  [--recursive] [--overwrite | --skip-existing]
  [--audio-bitrate KBPS] [--model-dir DIR] [--ffmpeg-path PATH]
  [--dry-run] [--json] [-v] [--version]
```

* `INPUT...`: 1..N rutas. Cada una puede ser archivo (`mp4/mov/mkv/webm/avi`) o directorio. Entry-point: bin `denoise` desde `src/main.rs` sobre lib `src/lib.rs` (`cargo run -- ...` equivalente, `version` desde `Cargo.toml`). Los tests de integración importan la lib, nunca el bin.
* `--audio-bitrate`: bitrate AAC de salida en kbps (defecto `192`, rango `64-320`).
* Reglas de salida (precedencia, alternativas excluyentes por D12):
  1. Si lote expandido==1 y `-o/--output OUT`: ese archivo exacto con auto-`.mp4` (D10: forma canónica `-o/--output`; D20: vale `INPUT` archivo o directorio que expande a 1 video; D35 cerrado 2026-09-13 revisado: si no termina en `.mp4` case-insensitive se añade automáticamente, ej. `final` → `final.mp4`).
  2. Si `--output-dir DIR`: `DIR/<relativo-a-cwd>/<prefix><stem><suffix>.mp4`, recreando subcarpetas si `--recursive` (D27 cerrado 2026-09-13, opción A: `dirA/sub/x.mp4 → out/dirA/sub/x_denoised.mp4`; fuera de cwd → solo `stem` + aviso `-v`).
  3. Por defecto: junto al original como `<stem><suffix>.mp4` con `suffix=_denoised`, `prefix=""`.
  4. (D12 cerrado 2026-09-13; D26 cerrado 2026-09-13: renumerado 4-7) `-o/--output` y `--output-dir` mutuamente excluyentes vía `clap(conflicts_with)`; combinación → `E_INVALID_INPUT` exit `2`, sin escribir disco.
  5. Colisión sin `--overwrite`: error salvo `--skip-existing` (marca `skipped`, exit 0).
  6. (D7 cerrado) Directorios padre de `-o/--output`, `--output-dir` y `--model-dir` se crean siempre; si no creables → `E_IO`.
  7. `prefix/suffix` solo `[A-Za-z0-9._-]`, prohibidos `.` y `..` exactos; no ambos vacíos si salida in-place. `-o/--output` que resuelve a la propia entrada sin `prefix/suffix` efectivo → warning a `stderr` (D33 cerrado 2026-09-13).
* Ejemplos:
  * `denoise boda.mp4`
  * `denoise boda.mp4 --prefix pod- --suffix _clean --audio-bitrate 128`
  * `denoise ./crudos/ --recursive --output-dir ./limpios/ --skip-existing --json`

## 5. Pipeline por video

```
expandir → validar extensión/existencia → resolver salida (puras, sin I/O)
  → colisión? → has_audio? → modelo listo?
  → [1-5%] ffmpeg extract mono 48k a TMP.in.wav
  → [6-80%] df::denoise() por chunks 60s/1s con callback (d,t)
  → [81-95%] ffmpeg remux copy+AAC a OUT.part.mp4 → rename a OUT.mp4 (POSIX atómico con reemplazo; Win con `--overwrite` remove previo + rename, sin overwrite falla si existe)
  → [96-99%] verificar OUT existe y >0B + duración ±0.5s + Audio AAC → limpiar temps → [100%] report {ok,failed,skipped}
```

Responsabilidades estrictas:
* `cli.rs`: parseo `clap`, expansión determinista byte-wise UTF-8 (D21), dedup por absoluto lexical contra cwd sin resolver symlinks (D28 cerrado 2026-09-13; case-insensitive solo Win), `resolve_output()` pura, bucle lote secuencial, reporte. No DSP. La expansión excluye `--output-dir` si está dentro de `INPUT` + aviso en `-v`. (D4 cerrado) Excluye además `*<suffix>.mp4` vigente en escaneos `--recursive` + aviso en `-v`.
* `pipeline.rs`: orquesta un video, traduce progreso a `0-100` según mapeo de §8 de este documento, garantiza limpieza + borrado parcial en error/`Ctrl+C`. Verificación ligera runtime: `>0B + duración ±0.5s + Audio AAC presente`.
* `df/` (D40 cerrado 2026-09-13, opción A: submódulos, cada uno <300 líneas): solo `ndarray+rustfft+ort`, firma `denoise_wav(in_wav: &Path, out_wav: &Path, progress_cb)` en `df/mod.rs`; `stft.rs` framing+STFT/iSTFT+WNORM, `erb.rs` constantes+features, `net.rs` sesiones `ort` cacheadas, `overlap.rs` crossfade+recorte. Sin `Command`, sin `println!` en núcleo. I/O WAV con `hound` PCM16 ↔ `f32` (`/32768.0`, clip antes de `i16`).
* `ffmpeg_io.rs`: `find_ffmpeg()` (vía crate `which`, con `PATHEXT` en Win), `has_audio()`, `extract_mono48k()`, `remux_copy()`, `verify_output_ligero()`. Todo `Command` argv, `String::from_utf8_lossy`.
* `models.rs`: `ensure_models(model_dir, progress_cb)` tras trait `ModelsProvider` (`FakeProvider` en tests) + verificación tamaño + SHA.
* `errors.rs`: enum `E_*` + `exit_code()` con `thiserror` en lib / `anyhow` en bin (creado primero, usado sin I/O).

Comandos ffmpeg exactos a reimplementar:

```text
# 1. Extracción (mono 48k exigido por DFN3, D3 cerrado: primera pista determinista)
ffmpeg -y -v error -i IN -map 0:a:0 -vn -ac 1 -ar 48000 TMP.in.wav

# 2. Remux (video intacto, audio limpio a AAC con bitrate parametrizable)
ffmpeg -y -v error -i IN -i TMP.out.wav
  -map 0:v:0 -map 1:a:0 -c:v copy -c:a aac -b:a <audio-bitrate>k -shortest OUT.mp4
```

Detección de audio: `ffmpeg -hide_banner -i IN` contiene `Audio:`. Sin audio → error `E_NO_AUDIO`, no se genera salida.

Temporales por trabajo: `<out>.tmp.in.wav`, `<out>.tmp.out.wav` junto a la salida. Limpieza garantizada en todos los caminos salvo `-v` debug (D31 cerrado 2026-09-13: con `-v` se conservan `.wav`/`.part` para inspeccionar). Handler `ctrlc` → `Child::kill` (`ffmpeg`) + aborto cooperativo de `ort` entre chunks (latencia máx 1 chunk en curso), borra `.part` + temps, `exit 3` (2º `Ctrl+C` fuerza salida inmediata); en Win hijo con `CREATE_NEW_PROCESS_GROUP`, observable único Win/POSIX. Fallos de escritura → `E_IO`.

## 6. Módulo `df/` — especificación DSP (no cambiar valores)

Parámetros fijos:

| Constante | Valor | Origen |
|---|---|---|
| `SR` | 48000, mono `f32` | contrato DFN3 |
| `FFT/HOP` | 960 / 480, ventana vorbis | libDF v0.5.6 |
| `WNORM` | `1/(FFT*FFT/(2*HOP))` | escala análisis |
| `NB_ERB/NB_DF/ORDER` | 32 / 96 / 5 | export oficial |
| `LOOKAHEAD` | 2 frames | alineación salida |
| `ALPHA` | 0.99 | media móvil features |
| `MIN/MAX_ERB/MAX_DF` | -15 / 35 / 20 dB | gating LSNR (D37 cerrado 2026-09-13, opción B: MIN/MAX desde `config.ini` oficial `lsnr_min=-15/lsnr_max=35`; MAX_DF 20 se mantiene como umbral DF interno) |
| `CHUNK/OVERLAP` | 60s / 1s, crossfade lineal | memoria constante |

Secuencia por chunk:

1. Framing streaming `frame t = [(t-1)*hop,(t+1)*hop)` + pad inicial `HOP` + cola `FFT+LOOKAHEAD*HOP`.
2. `STFT * ventana vorbis * WNORM` → `spec`.
3. Features: `ERB log-power mean-norm /40` + `unit-norm compleja` con `alpha=0.99`.
4. Inferencia CPU 3 grafos: `enc(feat_erb,feat_spec) → emb,e0..e3,c0,lsnr`; `erb_dec(emb,e3,e2,e1,e0) → mask`; `df_dec(emb,c0) → coefs`.
5. Alineación `k+LOOKAHEAD`, `out = spec*(mask@erb_inv)`; deep-filter taps `k-2..k+2` en bins `0..96` si `lsnr<=20`; si `lsnr>35` intacto; si `lsnr<-15` mute (D37 opción B).
6. `iSTFT * FFT * ventana`, overlap-add, recorte `HOP:HOP+n`, crossfade entre chunks.

Criterio de fidelidad: `SI-SDR(denoised,voz)-SI-SDR(mezcla,voz) >=5dB` y `SI-SDR(denoised,referencia_dfn3) >=60dB` (bloqueante, D9 cerrado + D30 cerrado 2026-09-13: estricto sin relajación; 55-59dB también bloquea). Vectores deterministas en `tests/data/` versionados en git, generados solo manualmente con `examples/gen_vectors.rs` (D39 cerrado 2026-09-13, opción A: `cargo run --example gen_vectors`; nunca en `cargo test` ni CI, preserva el congelado D14): seno `440Hz 3s` + ruido blanco `SNR 10dB`, `seed 0`, `SR 48k` mono `f32` (`voz.wav`, `mezcla10dB.wav`, `referencia_dfn3.wav` D14: generada una vez con el propio port tras validar mejora, luego congelada; D37 invalida refs previas: regenerar una única vez con nuevos umbrales D37 y recongelar) + par largo `440Hz 65s seed 1` (`voz65s.wav`, `mezcla65s10dB.wav`, `referencia65s_dfn3.wav`) para 2 chunks + crossfade (misma regla D14/D37). `SI-SDR` implementado Rust puro en `tests/common/si_sdr.rs` (zero-mean, `eps=1e-8`). Cualquier desviación bajo umbrales = bug bloqueante. Referencias informativas: `~77dB` paridad, `20.8dB` pipeline oficial.

## 7. Módulo `models.rs` — modelo autocontenido

* Artefactos: `dfn3_enc.onnx`, `dfn3_erb_dec.onnx`, `dfn3_df_dec.onnx` (~8MB total, tarball `7983136B`).
* Origen único: `https://github.com/Rikorose/DeepFilterNet/raw/v0.5.6/models/DeepFilterNet3_onnx.tar.gz` (D2 cerrado-verificado 2026-09-13 en `docs/plan.md T2.0`: URL canónica devuelve 200 con 7983136B exactos; no se canoniza `releases/download`), miembros `tmp/export/{enc,erb_dec,df_dec}.onnx` (+`config.ini` presente ignorado). Verificación: tamaño `7983136B >=98%` siempre + `SHA256 C94D91F70911001C946E0FABB4AA9ADC37045F45A03B56008CB0C8244CB63616` obligatorio y mismatch → `E_MODEL_MISSING`; SHA verificado 2026-09-13 vía `Invoke-WebRequest` + `Get-FileHash`/`tar -tzf` y registrado en `docs/specifications.md RF-06/RNF-05` + este `§7` (D1 cerrado definitivo, fin de la política interina).
* Cache: `--model-dir` (defecto `~/.cache/denoise/models/` vía `home_dir()+.cache` + `PathBuf` en Win/macOS/Linux) (D8 cerrado; D16: todo `denoise`). (D7 cerrado) Directorios padre de `-o/--output`/`--output-dir`/`--model-dir` se crean siempre; si no creables → `E_IO`.
* Comportamiento: si faltan → descarga con `User-Agent: denoise/1.0.0` (D16: todo `denoise`), progreso `indicatif`, verificación tamaño + SHA, extracción `tar.gz`, borrado archivo. `timeout 30s + retry 3 con backoff`, anti `tar-slip` (solo miembros `tmp/export/{enc,erb_dec,df_dec}.onnx`, rechaza `..`/absolutos), chequeo espacio >=50MB libres en disco de `--model-dir` vía `sysinfo` antes de descargar (D17 + D29 cerrado 2026-09-13: solo `model-dir`; `OUT/temps` → `E_IO` al fallar escritura), `.part + rename` (Win `remove` previo si existe). Si la descarga falla por red/modelo (timeout, HTTP, tamaño, SHA, tar-slip) → error `E_MODEL_MISSING` con URL y ruta manual esperada (disco → `E_IO`). (D24 cerrado 2026-09-13). Sesiones `ort` `CPUExecutionProvider` (binarios vía `download-binaries`), cacheadas (`Mutex`), un lock de inferencia. `reqwest blocking` sin dependencia directa a `tokio` (D15 + D32 cerrado 2026-09-13).

## 8. Módulo `ffmpeg_io.rs` + errores

Resolución: `--ffmpeg-path` → crate `which` en `PATH` (con `PATHEXT` en Win) → error `E_FFMPEG_NOT_FOUND`. Requiere `ffmpeg 6+`, verificado con `ffmpeg -version` con regex `ffmpeg version (\d+)\.` (major>=6, `from_utf8_lossy`), sin `ffprobe` (probe y `has_audio` vía `ffmpeg -hide_banner -i`). Receta por OS en `README.md`: Win `winget install Gyan.FFmpeg` / `choco install ffmpeg`, macOS `brew install ffmpeg`, Linux `apt install ffmpeg`.

Taxonomía estable (código → exit):
* `E_INVALID_INPUT` → 2: ruta inexistente, extensión no soportada, `0B`, `bitrate` fuera de `64-320`, `-o` con lote expandido `>1` (D20), `-o/--output` + `--output-dir` juntos (D12), `prefix+suffix` ambos vacíos con salida in-place, directorio sin videos/lote vacío. (D6 cerrado) Solo-audio/corrupto en probe también `E_INVALID_INPUT`; `E_FFMPEG_FAILED` solo en `extract/remux`. (D35: `-o` sin `.mp4` no es error, se auto-añade).
* `E_OUTPUT_EXISTS` → 2: destino existe sin `--overwrite` ni `--skip-existing`.
* `E_NO_AUDIO` → 2: sin pista `Audio:` (D6 cerrado: incluye solo-video).
* `E_FFMPEG_NOT_FOUND`, `E_MODEL_MISSING`, `E_FFMPEG_FAILED`, `E_IO` (disco lleno/sin permiso/sin espacio/`-o/--output`/`--output-dir`/`--model-dir` no creables) → 1.
* `E_CANCELLED` (`Ctrl+C` cooperativo entre chunks) → 3.

| Código | Caso | Salida |
|---|---|---|
| 0 | ok / `skipped` con `--skip-existing` | archivo verificado ligero |
| 1 | `E_FFMPEG_NOT_FOUND`, `E_MODEL_MISSING`, `E_FFMPEG_FAILED`, `E_IO` | stderr + no salida parcial |
| 2 | `E_NO_AUDIO`, `E_INVALID_INPUT`, `E_OUTPUT_EXISTS` | nada escrito |
| 3 | `E_CANCELLED` | proceso hijo matado (`Child::kill`, Win `CREATE_NEW_PROCESS_GROUP`), inferencia abortada entre chunks, temps y `.part` borrados |

Batch nunca aborta en el primer fallo (salvo `Ctrl+C`): continúa y resume `ok/failed/skipped` con exit: `3` si hubo cancelación, si no `1` si hubo algún fallo `1`, si no `2` si hubo fallos `2`, si no `0`.

Reporte dinámico pero ordenado (sin nuevos flags):
* Humano (defecto, TTY): cabecera `[i/N] in → out`, una barra viva por video a `stderr` (`indicatif`, `unit=chunk`; `hidden()` con `--json`/sin TTY), más línea final por video `done|failed|skipped + MB + segundos`. Resumen final siempre visible. Flush explícito, sin emojis, ASCII seguro en `pwsh`.
* Humano sin TTY/CI: sin animación (`hidden()`), líneas ` [i/N] name ... 45% msg` cada cambio de fase.
* `-v`: `log + env_logger` (`info`/`debug`), añade a `stderr` comando `ffmpeg` exacto, `model-dir`, tiempos por fase, `chunks d/t`, tamaños `in.wav/out.wav`, y conserva temps.
* `--json`: desactiva animación; `stdout` = `JSONL` una línea por archivo `{input,output,status,message,pct}` + línea final `{summary:{ok,failed,skipped}}` (`status ∈ {ok,failed,skipped,dry-run}`, `pct` según mapeo `0/1-5/6-80/81-95/96-99/100`, `summary` sin `pct`); progreso humano suprimido. Parseable con `jq`. Ver `docs/specifications.md RF-08`. (D38 cerrado 2026-09-13, opción A: `--json + -v` combinables —debug a `stderr` + conserva `.wav`/`.part`, `stdout` intacto).
* `--dry-run`: tabla `input → output (skip: motivo)` sin escribir/crear nada, mismo orden que el lote real, solo lectura `stat` para colisión; sin `ffmpeg/modelo/has_audio` (D23 + D25 cerrado 2026-09-13) (con `--json` emite `JSONL` con `status="dry-run" pct=0`). (D5 cerrado) Exit `0` siempre en `--dry-run`.
* `--version`: imprime `denoise 1.0.0 + modelo DFN3 v0.5.6 + ffmpeg <ver>` desde `Cargo.toml` vía `env!("CARGO_PKG_VERSION")` (D16: todo `denoise`) (sin `ffmpeg` imprime `ffmpeg missing`). Ver `docs/specifications.md RF-10`.

Ejemplo humano:
```text
[2/5] boda.mp4 -> boda_denoised.mp4
  extract 100% | denoise chunk 5/7 71% | remux 100%
[2/5] done 24.1MB en 38s
Summary: ok=4 failed=1 skipped=0
```

## 9. Límites v1

* Solo denoise de voz con DeepFilterNet3. Sin servidor, sin app desktop, sin base de datos.
* Sin `MDX`, `compress`, `to_gif`, `Vocal Remover / Mastering / Tag Editor`.
* Batch secuencial. Mono. Primera pista de audio. Video con `-c:v copy`.

## 10. Plan de verificación mínima

1. `test_naming` (rápido, sin ffmpeg/red): 1 archivo, N archivos, `--output-dir`, `prefix/suffix`, `-o/--output` solo lote==1 → error si lote `>1` (D20; vale dir con 1 video), colisión + `overwrite/skip`, `--recursive` recrea árbol relativo a cwd (D27: 2 dirs mismo `sub/x.mp4` → 2 salidas) + dedup lexical D28 + exclusión output anidado + exclusión `*<suffix>.mp4` (D4), caracteres inválidos + `.`/`..` (D33) → `E_INVALID_INPUT`.
2. `test_golden` (`#[ignore]` slow): generador Rust (`rand StdRng seed 0/1` + Box-Muller, `hound`): seno `440Hz 3s` y `65s` + ruido blanco `SNR 10dB`, `SR 48k` mono `f32` en `tests/data/` → `mejora >=5dB` y `paridad vs referencia >=60dB` en ambos pares (D30). Falla si DSP difiere.
3. `test_remux` (`#[ignore]` slow): fixture `ffmpeg -y -v error -f lavfi -i testsrc=size=640x480:rate=30:duration=5 -f lavfi -i sine=frequency=440:sample_rate=48000:duration=5 -c:v libx264 -pix_fmt yuv420p -c:a aac -b:a 192k -shortest fixture.mp4` → mismo `vcodec/res/fps`, duración `±0.2s`, `acodec=aac,sr=48000,bitrate±10%`, sin re-encode: `vcodec`/`res`/`fps`/`extradata` iguales + `sha256` de `ffmpeg -y -v error -i OUT.mp4 -map 0:v:0 -c copy -f h264 -` idéntico al de la entrada (prohibido comparar tamaño fichero total).
4. `test_errors`: sin audio/solo-video → `E_NO_AUDIO`; solo-audio/corrupto en probe → `E_INVALID_INPUT` (D6); ffmpeg ausente → `E_FFMPEG_NOT_FOUND`; descarga rota → `E_MODEL_MISSING`; destino existe → `E_OUTPUT_EXISTS`/`skipped`; `bitrate 9999` → `E_INVALID_INPUT`; disco/sin permiso/`-o/--output`/`--output-dir`/`--model-dir` no creables → `E_IO` (D7). Nada parcial en disco.
5. `test_reporter`: lote 3 simulado verifica orden `[1/3..3/3]`, `JSONL` parseable, resumen `ok/failed/skipped`, sin animación con `--json` o sin TTY.
6. Manual: 1 video corto Win + lote 5 videos, `--dry-run` primero (exit `0`, D5), luego real + `--json`. Comandos: `cargo test` y `cargo test -- --ignored`.
