# Diseño — CLI autocontenida `denoise` v1 Rust

> Alcance: única funcionalidad — eliminar ruido de la pista de audio de uno o varios videos.
> Repo autocontenido: este repo (`Denoise/` raíz).
> Documentos canónicos en `docs/`: este diseño + `specifications.md`. En caso de divergencia, el contrato CLI de §4 manda.
> Convención de rutas: `docs/<fichero>` es relativo a la raíz del repo.

## Índice

1. [Objetivo y no-objetivos](#1-objetivo-y-no-objetivos)
2. [Principios de diseño](#2-principios-de-diseño)
3. [Estructura mínima propuesta](#3-estructura-mínima-propuesta)
4. [Contrato CLI (v1)](#4-contrato-cli-v1)
5. [Pipeline por video](#5-pipeline-por-video)
6. [Especificación DSP del módulo `df/` (no cambiar valores)](#6-especificación-dsp-del-módulo-df-no-cambiar-valores)
7. [Módulo `models.rs`: modelo autocontenido](#7-módulo-modelsrs-modelo-autocontenido)
8. [Módulo `ffmpeg_io.rs` y errores](#8-módulo-ffmpeg_iors-y-errores)
9. [Límites v1](#9-límites-v1)
10. [Plan de verificación mínima](#10-plan-de-verificación-mínima)

> Las secciones conservan su numeración `§N` porque son contrato: `src/` y `docs/specifications.md` las citan por número.

## 1. Objetivo y no-objetivos

**Objetivo:** CLI `denoise` que recibe 1..N videos, les quita ruido de fondo/hiss/ambiente de la voz y entrega nuevos `.mp4` con el video intacto.

**No-objetivos explícitos:**

* Sin servidor web, sin desktop app, sin base de datos de tareas, sin polling.
* Sin `Vocal Remover / Mastering / Tag Editor / Compresor / GIF / Upscale / Restauración facial`.
* Sin separación MDX (`Kim_Vocal_2`). Solo `denoise` voz.
* Sin concurrencia masiva. Batch secuencial, determinista.

## 2. Principios de diseño

1. **Autocontenida Rust:** `std` + `ort` 2 pinnado a `=2.0.0-rc.13` con `default-features = false` + `features = ["std","copy-dylibs","download-binaries","tls-rustls"]` (pin fijo) + `rustfft` + `hound` + `clap` 4 + `indicatif` + `reqwest` 0.12 (blocking + rustls-tls-webpki-roots) + `serde` 1 (+derive)/`serde_json` + `which` + `regex` 1 + `sysinfo` + binario `ffmpeg` (externo) + `sha2`/`home`/`ctrlc`/`anyhow` (`thiserror` en lib); `rust-version = "1.88"`, `edition = "2021"`, `Cargo.lock` versionado en git, `[profile.release] opt-level = 3, strip = true`; lista completa en `docs/specifications.md RNF-01`, detalle de features autoritativo en `Cargo.toml`). Sin dependencias externas pesadas ni servidores (`sysinfo` para el chequeo de disco).
2. **No reinventar DSP:** constantes y orden de operaciones de sherpa-onnx/knf se copian exactos según §6 (pasos 1-4; el gate de pausa del paso 6 se añade después de la síntesis y no forma parte de sherpa-onnx). El riesgo es regresión numérica.
3. **Video nunca se re-codifica:** `-c:v copy`. Solo el audio se procesa.
4. **Fallo explícito y limpio:** exit codes, temps siempre borrados, `String::from_utf8_lossy` en Windows.
5. **CLI predecible para batch y scripting:** `--dry-run`, `--json`, `--skip-existing`, lote secuencial.
6. **Implementación legible Rust:** `stable 1.88+ edition 2021`, `PathBuf`, `clippy+rustfmt`, `anyhow` en bin / `thiserror` en lib, `eprintln!` a `stderr` (`--verbose` debug), funciones pequeñas, sin estado global salvo sesiones `ort` cacheadas (`Mutex`).
7. **Testing por pirámide:** unitarias rápidas con `ffmpeg 6+` real sin red/modelo (`cargo test`), doradas DSP bloqueantes + e2e `remux` con `#[ignore]` (slow) vía `cargo test --release -- --ignored`.
8. **Documentación como contrato:** `README.md` (MIT + atribución Apache 2.0 Ceva-IP/DPDFNet) + `--help` + ejemplos idénticos a §4; `CHANGELOG.md` por release (inicial `1.0.0`); `LICENSE` MIT en raíz.

## 3. Estructura mínima propuesta

Todo lo siguiente vive dentro de este repo, en la raíz `./`.

```
./ (Denoise/)
 docs/
  design.md # este documento (arquitectura + pipeline)
  specifications.md # RF/RNF canónicos + aceptación
 LICENSE # MIT (código nuevo)
 README.md # instalación, guía con ejemplos que cubren §4, atribución Apache 2.0 © Ceva-IP/DPDFNet (modelo)
 CHANGELOG.md # por release (inicial 1.0.0)
 Cargo.toml # package/bin `denoise`, version="1.0.0" fuente única, edition 2021, rust-version 1.88, [profile.release]
 Cargo.lock # versionado en git
 src/
 main.rs # bin denoise fino (llama a lib)
 lib.rs # lib denoise, re-exporta cli/pipeline/df/models/ffmpeg_io/errors
 cli.rs # clap, expansión entradas, naming salida, reporte lote
 pipeline.rs # clean_one_video: extract → denoise → remux + verificación
 df/ # DSP DPDFNet + inferencia ort (puro, sin I/O proceso)
 mod.rs # orquestación secuencial + firma denoise_wav
 stft.rs # framing + STFT/iSTFT vorbis + overlap-add (réplica knf)
 net.rs # sesión ort stateful cacheada (Mutex) + inferencia
 models.rs # descarga/cache/verificación de dpdfnet8_48khz_hr.onnx
 ffmpeg_io.rs # localización ffmpeg, has_audio, extract, remux, verify
 errors.rs # E_* + exit codes
 tests/
 test_naming.rs # reglas prefix/suffix/output-dir (--output-name) (sin ffmpeg/red)
 common/si_sdr.rs # helper SI-SDR Rust puro (cada test lo declara: #[path = "common/si_sdr.rs"] mod si_sdr)
 test_golden.rs # voz real + ruido (3s SNR 0dB, 65s SNR 10dB) → mejora SI-SDR + paridad (#[ignore] slow)
 test_remux.rs # duración ±0.5s + hash h264 (stream copy, sin checks de vcodec/res/fps/bitrate) (#[ignore] slow)
 test_errors.rs # E_* + overwrite/skip (Ctrl+C no se simula en tests, solo manual)
 test_reporter.rs # orden [i/N], JSONL parseable, summary ok/failed/skipped
  data/README.md # vectores con voz real del EvalSet + generador (`cargo run --release --example gen_vectors`, solo manual; no corre en `cargo test`)
 data/*.wav # vectores versionados (nunca se regeneran en CI)
  examples/
   gen_vectors.rs # generador con voz real del EvalSet (`cargo run --release --example gen_vectors -- <dir_eval>`, solo manual; no corre en `cargo test`)
```

Infraestructura añadida después de esta propuesta mínima (puerta de calidad `tools/quality/`, `docs/metrics.md`, `docs/benchmark.md`, `assets/`, `examples/process_wav.rs`, `AGENTS.md`): ver el árbol completo en `README.md` §10.

## 4. Contrato CLI (v1)

```text
denoise INPUT... [-o/--output-name NAME] [--output-dir DIR] [--prefix STR] [--suffix STR]
  [--recursive] [--overwrite | --skip-existing]
  [--audio-bitrate KBPS] [--model-dir DIR] [--ffmpeg-path PATH]
  [--dry-run] [--json] [--verbose] [-V/--version]
```

* `INPUT...`: 1..N rutas. Cada una puede ser archivo (`mp4/mov/mkv/webm/avi`) o directorio. Entry-point: bin `denoise` desde `src/main.rs` sobre lib `src/lib.rs` (`cargo run -- ...` equivalente, `version` desde `Cargo.toml`). Los tests de integración importan la lib, nunca el bin.
* `--audio-bitrate`: bitrate AAC de salida en kbps (defecto `192`, rango `64-320`).
* Reglas de salida (precedencia; `--output-name` y `--output-dir` son COMPLEMENTARIOS, no excluyentes):
  1. Si lote expandido==1 y `--output-name NAME`: `DIR/<NAME>.mp4`, donde `DIR` es `--output-dir` si se pasa, sino cwd. Auto-`.mp4` si NAME no termina en `.mp4` case-insensitive (`final` → `final.mp4`). `prefix`/`suffix` se **ignoran** cuando `--output-name` está presente. Con lote expandido>1 → `E_INVALID_INPUT`.
  2. Si solo `--output-dir DIR`: `DIR/<relativo-a-cwd>/<prefix><stem><suffix>.mp4`, recreando subcarpetas si `--recursive` (`dirA/sub/x.mp4 → out/dirA/sub/x_denoised.mp4`; fuera de cwd → solo `stem` + aviso `--verbose`).
  3. Por defecto: junto al original como `<prefix><stem><suffix>.mp4` con `suffix=_denoised`, `prefix=""`.
  4. Si 2+ entradas del lote resuelven al MISMO path de salida (`a.mp4`+`a.mov`→`a_denoised.mp4`), el 2º y siguientes reciben auto-sufijo incremental `_1`, `_2`... (`a_denoised_1.mp4`) con aviso SIEMPRE a `stderr` (no solo `--verbose`); aplica a las reglas 2 y 3. `--dry-run` muestra las salidas desempatadas ya en la tabla.
  5. Colisión sin `--overwrite`: error salvo `--skip-existing` (marca `skipped`, exit 0).
  6. Directorios padre de `--output-name`, `--output-dir` y `--model-dir` se crean siempre; si no creables → `E_IO`.
  7. `prefix/suffix` solo `[A-Za-z0-9._-]`, prohibidos `.` y `..` exactos; no ambos vacíos si salida in-place. `--output-name` que resuelve a la propia entrada sin `prefix/suffix` efectivo → `E_INVALID_INPUT` exit `2` SIEMPRE, incluso con `--overwrite` (endurecido: el warning condicionado se reemplazó por error invariante).
  8. `--output-name` y `--output-dir` son **COMPLEMENTARIOS**: `--output-name` define el nombre base final; `--output-dir` el directorio. Cuando `--output-name` está presente, `prefix`/`suffix` se ignoran. La composición completa se documenta en `docs/specifications.md §2 RF-03` (tabla de decisión).
* Ejemplos:
  * `denoise boda.mp4`
  * `denoise boda.mp4 --prefix pod- --suffix _clean --audio-bitrate 128`
  * `denoise ./crudos/ --recursive --output-dir ./limpios/ --skip-existing --json`
  * `denoise a.mp4 --output-name final --output-dir limpio`

## 5. Pipeline por video

```
expandir → validar extensión/existencia → resolver salida (puras, sin I/O)
  → colisión? → has_audio? → modelo listo?
  → [1-5%] ffmpeg extract mono 48k a TMP.in.wav
 → [6-80%] df::denoise secuencial frame a frame con callback (s actual, s total)
  → [81-95%] ffmpeg remux copy+AAC a OUT.part.mp4 (-t <dur_video> = duración del contenedor) → rename a OUT.mp4 (atómico con reemplazo en POSIX y Win — `MoveFileExW REPLACE_EXISTING`; la no-sobrescritura sin `--overwrite` la garantiza el colisión-check previo, no el rename)
  → [96-99%] verificar OUT existe y >0B + duración ±0.5s + stream Audio presente → limpiar temps → [100%] report {ok,failed,skipped}
```

Responsabilidades estrictas:
* `cli.rs`: parseo `clap`, expansión determinista byte-wise UTF-8, dedup por absoluto lexical contra cwd sin resolver symlinks (case-insensitive solo Win), `resolve_output()` pura, bucle lote secuencial, reporte. No DSP. La expansión excluye `--output-dir` si está dentro de `INPUT` + aviso en `--verbose`, y excluye además `*<suffix>.mp4` vigente en escaneos `--recursive` + aviso en `--verbose`.
* `pipeline.rs`: orquesta un video, traduce progreso a `0-100` según mapeo de §8 de este documento, garantiza limpieza + borrado parcial en error/`Ctrl+C`. Verificación ligera runtime: `>0B + duración ±0.5s + stream Audio presente`.
* `df/`: solo `rustfft+ort`, firma `denoise_wav(in_wav: &Path, out_wav: &Path, progress_cb)` en `df/mod.rs`; `stft.rs` framing+STFT/iSTFT (réplica exacta knf), `net.rs` sesión stateful `ort` cacheada. Sin `Command`, sin `println!` en núcleo. I/O WAV con `hound` PCM16 ↔ `f32` (`/32768.0`, escala `*32768.0` con clip como `soundfile` PCM16).
* `ffmpeg_io.rs`: `find_ffmpeg()` (vía crate `which`, con `PATHEXT` en Win), `probe().has_audio` (campo de `ProbeResult`), `extract_mono48k()`, `remux_copy()`, `verify_output_ligero()`. Todo `Command` argv, `String::from_utf8_lossy`. Probe `ffmpeg -hide_banner -i` + regex `Duration:` = duración del CONTENEDOR, fuente de `<dur_video>` (sin decode extra del stream de video).
* `models.rs`: `ensure_models(model_dir, progress_cb)` tras trait `ModelsProvider` (`FakeProvider` en tests) + verificación tamaño + SHA.
* `errors.rs`: enum `E_*` + `exit_code()` con `thiserror` en lib / `anyhow` en bin (creado primero, usado sin I/O).

Comandos ffmpeg exactos a reimplementar:

```text
# 1. Extracción (mono 48k exigido por DPDFNet: primera pista determinista;
# en contenedores mov/mp4 se añade -ignore_editlist 1 antes de -i para alinear la línea de tiempo física)
ffmpeg -y -v error [-ignore_editlist 1] -i IN -map 0:a:0 -vn -ac 1 -ar 48000 TMP.in.wav

# 2. Remux (video intacto, audio limpio a AAC con bitrate parametrizable)
# `-t <dur_video>` sustituye a
# -shortest; <dur_video> = duración del CONTENEDOR del probe (Duration:), no del stream de video —
# dato que el probe sin ffprobe no expone—. Si el audio limpio es más corto, la cola queda muda;
# si el audio del input es más largo que el video, la salida se alarga a la del contenedor (limitación v1, §9);
# en contenedores mov/mp4 se añade -ignore_editlist 1 para sincronía perfecta 1:1 con el video copiado.
ffmpeg -y -v error [-ignore_editlist 1] -i IN -i TMP.out.wav
  -map 0:v:0 -map 1:a:0 -c:v copy -c:a aac -b:a <audio-bitrate>k -t <dur_video> OUT.mp4
```

Detección de audio: `ffmpeg -hide_banner -i IN` contiene `Audio:`. Sin audio → error `E_NO_AUDIO`, no se genera salida.

Temporales por trabajo: `<out>.tmp.in.wav`, `<out>.tmp.out.wav` junto a la salida. Limpieza garantizada en todos los caminos salvo `--verbose` debug (con `--verbose` se conservan `.wav`/`.part` para inspeccionar). Handler `ctrlc` → `Child::kill` (`ffmpeg`) + cierre de inferencia, borra `.part` + temps, `exit 3` (2º `Ctrl+C` fuerza salida inmediata); la descarga del modelo no es cancelable cooperativamente (`reqwest blocking` no consulta el flag; latencia máx = timeout 60s); hijo `ffmpeg` vía `Command::status()` bloqueante sin `creation_flags`, cancelación vía flag cooperativo `AtomicBool` + limpieza `Drop` (`TempCleaner`), observable único Win/POSIX. Fallos de escritura → `E_IO`.

## 6. Especificación DSP del módulo `df/` (no cambiar valores)

Parámetros fijos:

| Constante | Valor | Origen |
|---|---|---|
| `SR` | 48000, mono `f32` | contrato DPDFNet |
| `FFT/HOP` | 960 / 480, ventana Vorbis | metadatos del ONNX |
| `WNORM` | 1.0 (sin escala: el grafo recibe el espectro crudo) | sherpa-onnx/knf |
| `INV_FFT_SCALE` | `1/960` | `knf::IStft::InverseFFT` |
| `CENTER_CROP` | 480 (`FFT/2`) | `knf::IStft center=true` |
| `ISTFT_HEAD_CROP` | 1920 (`2*FFT`) | `ShiftWaveform` sherpa-onnx |

Secuencia (pasos 1-4: réplica exacta de sherpa-onnx + `knf`, sin trocear; pasos 5-7: posteriores a la síntesis, con el gate de pausa que sherpa-onnx no tiene):

1. Padding `knf::Stft::Pad`: reflect de `FFT/2` por extremo sobre la señal cruda (izquierda excluye `signal[0]`, derecha excluye la última, en orden inverso). Frames `1 + n/HOP`.
2. `STFT = FFT(muestra * ventana Vorbis)` sin `wnorm` → `spec` (`torch.stft(normalized=False)`; verificado contra torch a `7.5e-9`).
3. Inferencia CPU, grafo único stateful: `spec[t] + state → spec_e[t] + state`; estado inicial = ceros + `erb_norm_init`/`spec_norm_init` de los metadatos. Se conservan las `T` salidas (verificado contra onnxruntime a `1e-10`).
4. `iSTFT`: inversa con escala `1/N` + ventana de síntesis + overlap-add (envolvente 1 en la región recortada), `center` (quitar 480 por extremo), descartar 1920 del frente + ceros al final (`ShiftWaveform`), truncar a `n`.
5. Sin normalización ni limiter: la escala es la del modelo y la voz sale multiplicada por 1.0.
6. Gate de pausa `apply_pause_gate` (`src/df/mod.rs`, constantes `GATE_*`): VAD solo-salida sobre RMS de marcos de 1440 muestras (30 ms) — voz si `rms >= pico−30 dB`, salida de voz si `rms <= pico−45 dB` durante 3 marcos, hangover de 5 marcos — que atenúa `−25 dB` las pausas con fundidos raised-cosine de 1440 muestras, justo antes de cuantizar. Es solo atenuación de no-voz: sin ganancia, sin normalización global y sin limiter; la voz queda por 1.0.
7. Escritura PCM16 como `soundfile` (`*32768` con clip).

Criterio de fidelidad: `SI-SDR(denoised,voz)-SI-SDR(mezcla,voz) >=5dB` y `SI-SDR(denoised,referencia_dpdfnet) >=60dB` en el par 3s, paridad `>=60dB` en el par 65s (bloqueante, estricto sin relajación; 55-59dB también bloquea). Vectores deterministas en `tests/data/` versionados en git, generados solo manualmente con `examples/gen_vectors.rs` (`cargo run --release --example gen_vectors -- <dir_eval>` con `Clean/`+`Noisy/` del EvalSet DPDFNet; nunca en `cargo test` ni CI, preserva el congelado): voz real 3s + ruido a `SNR 0dB`, `SR 48k` mono `PCM16` en disco —dato en memoria `f32` vía `i16→f32/32768.0`, mismo camino que el pipeline real— (`voz_clean.wav`, `voz_noisy.wav`, `referencia_dpdfnet.wav` generada una vez con el propio port tras validar mejora, luego congelada, `voz65s_clean.wav`, `voz65s_noisy.wav`, `referencia65s_dpdfnet.wav` — esta última re-congelada el 2026-10-03 tras validar el gate, con `examples/process_wav.rs` sobre `voz65s_noisy.wav`, mismo camino `denoise_wav`, porque las fuentes `Clean/`+`Noisy/` street/subway no están en la máquina) para estabilidad en clips largos. Las 12 ventanas de 15 s `<escena>_snr<N>_{clean,noisy}.wav` (benchmark, sin referencia congelada) se cortaron de las mezclas nativas del EvalSet; detalle en `tests/data/README.md`. `SI-SDR` implementado Rust puro en `tests/common/si_sdr.rs` (zero-mean, `eps=1e-8`; cada test lo declara con `#[path = "common/si_sdr.rs"] mod si_sdr;`). Cualquier desviación bajo umbrales = bug bloqueante. Referencias informativas: paridad 100dB entre corridas, media SI-SDR 14.74 dB en el EvalSet (igual que sherpa-onnx), bit-exactitud 1 LSB PCM16 vs sherpa-onnx — estas dos últimas verificadas **antes** del gate de pausa: con el gate activo la voz sigue bit-idéntica (multiplicada por 1.0), pero la salida completa ya no es bit-exacta porque las pausas se atenúan `−25 dB` tras la síntesis.

Estado de los dorados (2026-10-03, tras el gate de pausa y su re-congelado): `test_golden_3s` **pasa** con paridad 67,77 dB sobre `referencia_dpdfnet.wav`, que sigue congelada del pipeline anterior al gate —el par es 90 % voz y el gate casi no altera su salida—, y `test_golden_65s` **pasa** con paridad 100,00 dB tras re-congelar `referencia65s_dpdfnet.wav` con este mismo port sobre el `voz65s_noisy.wav` congelado. El umbral `>=60dB` **no se relajó**: solo se regeneró la expectativa tras validar la mejora, conforme al criterio de congelado de este §6; ningún fixture de voz (`voz_*`, `voz65s_*`) se tocó y `cargo test --release -- --ignored` queda en verde 5/5.

## 7. Módulo `models.rs`: modelo autocontenido

* Artefacto único: `dpdfnet8_48khz_hr.onnx` (14857107B).
* Origen único: `https://huggingface.co/Ceva-IP/DPDFNet/resolve/main/onnx/dpdfnet8_48khz_hr.onnx` (URL canónica devuelve 200 con 14857107B exactos). Verificación en descarga: tamaño `14857107B >=98%` + `SHA256 7b3afbb260a08fe9af3d16e3bda992971be1e7e951d1dee7c2d235f5c43f5631` obligatorio y mismatch → `E_MODEL_MISSING`; si el ONNX ya existe con tamaño >0B se reutiliza sin re-verificar; registrado en `docs/specifications.md RF-06/RNF-05` + este `§7`. Licencia del modelo: Apache 2.0 (Ceva-IP/DPDFNet).
* Cache: `--model-dir` (defecto `~/.cache/denoise/models/` vía `home_dir()+.cache` + `PathBuf` en Win/macOS/Linux). Directorios padre de `--output-name`/`--output-dir`/`--model-dir` se crean siempre; si no creables → `E_IO`.
* Comportamiento: si falta → descarga con `User-Agent: denoise/1.0.0`, progreso `indicatif`, verificación tamaño + SHA. `timeout 60s + retry 3 con backoff`, chequeo espacio >=50MB libres en disco de `--model-dir` vía `sysinfo` antes de descargar (solo `model-dir`; `OUT/temps` → `E_IO` al fallar escritura), `.part + rename` (en `models.rs` con `remove_file` previo si existe destino; en remux `rename` directo atómico con reemplazo en POSIX/Win). Si la descarga falla por red/modelo (timeout, HTTP, tamaño, SHA) → error `E_MODEL_MISSING` con URL y ruta manual esperada (disco → `E_IO`).
* Sesión de inferencia: `ort` con `CPUExecutionProvider` (binarios vía `download-binaries`), cacheada en `Mutex`. `reqwest blocking` sin dependencia directa a `tokio`; TLS vía `rustls-tls-webpki-roots` — raíces Mozilla empaquetadas, sin OpenSSL del sistema, coherente con `ort tls-rustls`.

## 8. Módulo `ffmpeg_io.rs` y errores

Resolución: `--ffmpeg-path` → crate `which` en `PATH` (con `PATHEXT` en Win) → error `E_FFMPEG_NOT_FOUND`. Requiere `ffmpeg 6+`, verificado con `ffmpeg -version` y regex `ffmpeg version (\d+)\.` (major>=6, `from_utf8_lossy`), con fallback a prefijo `N-` (build git BtbN/gyan, aceptado como válido; si tampoco matchea → `E_FFMPEG_NOT_FOUND` con la línea de versión cruda en el mensaje). Sin `ffprobe`: el probe y el campo `probe().has_audio` se resuelven vía `ffmpeg -hide_banner -i`. Receta por OS en `README.md` §3: Win `winget install Gyan.FFmpeg` / `choco install ffmpeg`, macOS `brew install ffmpeg`, Linux `apt install ffmpeg`.

Taxonomía estable (código → exit):
* `E_INVALID_INPUT` → 2: ruta inexistente, extensión no soportada, `0B`, `bitrate` fuera de `64-320`, `--output-name` con lote expandido `>1` (solo con lote>1 es error; `--output-name` + `--output-dir` son COMPLEMENTARIOS permitidos, no error; errores estructurales de flags → exit `2` en dry-run), `prefix+suffix` ambos vacíos con salida in-place, `--output-name` que resuelve a la propia entrada sin `prefix/suffix` efectivo (`E_INVALID_INPUT` SIEMPRE, incluso con `--overwrite`), directorio sin videos/lote vacío. Solo-audio/corrupto en probe también `E_INVALID_INPUT`; `E_FFMPEG_FAILED` solo en `extract/remux` (`--output-name` sin `.mp4` no es error, se auto-añade).
* `E_OUTPUT_EXISTS` → 2: destino existe sin `--overwrite` ni `--skip-existing`.
* `E_NO_AUDIO` → 2: sin pista `Audio:` (incluye solo-video).
* `E_FFMPEG_NOT_FOUND`, `E_MODEL_MISSING`, `E_FFMPEG_FAILED`, `E_IO` (disco lleno/sin permiso/sin espacio/`--output-name`/`--output-dir`/`--model-dir` no creables) → 1.
* `E_CANCELLED` (`Ctrl+C` cooperativo) → 3.

| Código | Caso | Salida |
|---|---|---|
| 0 | ok / `skipped` con `--skip-existing` | archivo verificado ligero |
| 1 | `E_FFMPEG_NOT_FOUND`, `E_MODEL_MISSING`, `E_FFMPEG_FAILED`, `E_IO` | stderr + no salida parcial |
| 2 | `E_NO_AUDIO`, `E_INVALID_INPUT`, `E_OUTPUT_EXISTS` | nada escrito |
| 3 | `E_CANCELLED` | proceso hijo matado (`Child::kill` vía `Command::status()` bloqueante + flag cooperativo `AtomicBool` + limpieza `Drop`), inferencia abortada, temps y `.part` borrados |

Batch nunca aborta en el primer fallo (salvo `Ctrl+C`): continúa y resume `ok/failed/skipped` con exit: `3` si hubo cancelación, si no `1` si hubo algún fallo `1`, si no `2` si hubo fallos `2`, si no `0`.

Reporte dinámico pero ordenado (sin nuevos flags):
* Humano (defecto, TTY): cabecera `[i/N] in → out`, una barra viva por video a `stderr` (`indicatif`; `hidden()` solo con `--json`), más línea final por video `done|failed|skipped + MB + segundos`. Resumen final siempre visible. Flush explícito, sin emojis, ASCII seguro en `pwsh`.
* Humano sin TTY/CI: mismo comportamiento que con TTY (cabecera `[i/N]` + líneas por fase, sin animación real porque `indicatif` va a `stderr`).
* `--verbose`: añade a `stderr` el detalle del escaneo y nomenclatura (exclusión de `output-dir` anidado, exclusión de `*<suffix>.mp4`, entrada fuera de cwd resuelta con nombre plano) y conserva temps (`.tmp.*.wav`/`.part.mp4`).
* `--json`: desactiva animación; `stdout` = `JSONL` una línea por archivo `{input,output,status,message,pct}` + línea final `{summary:{ok,failed,skipped}}` (`status ∈ {ok,failed,skipped,dry-run}`, `pct` según mapeo `0/1-5/6-80/81-95/96-99/100` —`skipped` SIEMPRE `pct=0`—, `summary` sin `pct`); progreso humano suprimido. Parseable con `jq`. Ver `docs/specifications.md RF-08`. (`--json + --verbose` combinables —debug a `stderr` + conserva `.wav`/`.part`, `stdout` intacto).
* `--dry-run`: tabla `input → output (skip: motivo)` sin escribir/crear nada, mismo orden que el lote real, solo lectura `stat` para colisión; sin `ffmpeg/modelo/has_audio`. Con `--json` emite `JSONL` con `status="dry-run"` y `pct=0`. Sale con `exit 0` si la CLI es estructuralmente válida; los errores estructurales (flags inválidos/combinados, bitrate fuera de rango, prefix+suffix inválidos, lote vacío) fallan antes de simular con `exit 2` y sin reporte, mientras que los casos per-archivo se reportan con `exit 0`. El dry-run simula además la intención de los flags de colisión: destino existente con `--overwrite` → `would overwrite`, sin `--overwrite` → `would fail: E_OUTPUT_EXISTS`, con `--skip-existing` → `would skip`.
* `--version`: imprime `denoise 1.0.0 + modelo DPDFNet + ffmpeg <ver>` desde `Cargo.toml` vía `env!("CARGO_PKG_VERSION")` (sin `ffmpeg` imprime `ffmpeg missing`). Ver `docs/specifications.md RF-10`.

Ejemplo humano:
```text
[2/5] boda.mp4 -> boda_denoised.mp4
 extract 100% | denoise 71% | remux 100%
[2/5] done 24.1MB en 38s
Summary: ok=4 failed=1 skipped=0
```

## 9. Límites v1

* Solo denoise de voz con DPDFNet. Sin servidor, sin app desktop, sin base de datos.
* Sin `MDX`, `compress`, `to_gif`, `Vocal Remover / Mastering / Tag Editor`.
* Batch secuencial. Mono. Primera pista de audio. Video con `-c:v copy`.
* `<dur_video>` del remux = duración del CONTENEDOR (`Duration:` del probe): en inputs donde la pista de audio dura más que el video, la salida se alarga a la duración del contenedor (el video termina antes / último frame extendido); sin decode extra del stream de video en v1.

## 10. Plan de verificación mínima

1. `test_naming` (rápido, sin ffmpeg/red): 1 archivo, N archivos, `--output-dir`, `prefix/suffix`, `--output-name` solo lote==1 → error si lote `>1` (vale dir con 1 video), `--output-name` + `--output-dir` complementarios, colisión + `overwrite/skip`, `--recursive` recrea árbol relativo a cwd (2 dirs mismo `sub/x.mp4` → 2 salidas) + dedup lexical + exclusión output anidado + exclusión `*<suffix>.mp4`, caracteres inválidos + `.`/`..` → `E_INVALID_INPUT`, `--output-name` resolviendo a la propia entrada sin `prefix/suffix` → `E_INVALID_INPUT`.
2. `test_golden` (`#[ignore]` slow): generador Rust (`cargo run --release --example gen_vectors -- <dir_eval>`, voz real del EvalSet DPDFNet): par 3s a `SNR 0dB` + par 65s a `SNR 10dB`, `SR 48k` mono `PCM16` en disco (dato en memoria `f32`) en `tests/data/` → `mejora >=5dB` y `paridad vs referencia >=60dB` en el par 3s, paridad `>=60dB` en el par 65s. Falla si DSP difiere. Estado 2026-10-03: ambos pares en verde (67,77 dB y 100,00 dB tras re-congelar la referencia del par 65 s; ver el estado detallado en §6).
3. `test_remux` (`#[ignore]` slow): fixture `ffmpeg -y -v error -f lavfi -i testsrc=size=640x480:rate=30:duration=5 -f lavfi -i sine=frequency=440:sample_rate=48000:duration=5 -c:v libx264 -pix_fmt yuv420p -c:a aac -b:a 192k fixture.mp4` (sin `-shortest`) → duración `±0.5s` + `sha256` de `ffmpeg -y -v error -i OUT.mp4 -map 0:v:0 -c copy -f h264 -` idéntico al de la entrada (igualdad de hash h264, sin re-encode) + `verify_output_ligero` (prohibido comparar tamaño fichero total; sin checks de `vcodec/res/fps/extradata` ni `bitrate±10%`).
4. `test_errors`: sin audio/solo-video → `E_NO_AUDIO`; solo-audio/corrupto en probe → `E_INVALID_INPUT`; ffmpeg ausente → `E_FFMPEG_NOT_FOUND`; descarga rota → `E_MODEL_MISSING`; destino existe → `E_OUTPUT_EXISTS`/`skipped`; `bitrate 9999` → `E_INVALID_INPUT`; `--output-name` resolviendo a la propia entrada sin `prefix/suffix` → `E_INVALID_INPUT` (incluso con `--overwrite`); disco/sin permiso/`--output-name`/`--output-dir`/`--model-dir` no creables → `E_IO`. Nada parcial en disco.
5. `test_reporter`: lote 3 simulado verifica orden `[1/3..3/3]`, `JSONL` parseable, resumen `ok/failed/skipped`, sin animación con `--json`.
6. Manual: 1 video corto Win + lote 5 videos, `--dry-run` primero (exit `0`), luego real + `--json`. Comandos: `cargo test` y `cargo test --release -- --ignored`.
