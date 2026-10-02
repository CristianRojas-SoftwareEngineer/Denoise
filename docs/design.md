# Diseño — CLI autocontenida `denoise` v1 Rust

> Alcance: única funcionalidad — eliminar ruido de la pista de audio de uno o varios videos.
> Repo autocontenido: este repo (`Video-Noise-Remover/` raíz).
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

1. **Autocontenida Rust:** `std + ort 2 pinnado `=2.0.0-rc.13` con `default-features=false` + `features = ["std","copy-dylibs","download-binaries","tls-rustls"]` (pin fijo) + rustfft + hound + clap 4 + indicatif + reqwest 0.12 (blocking + rustls-tls-webpki-roots) + serde 1 (+derive)/serde_json + log + env_logger + which + regex 1 + sysinfo + ffmpeg` binario (+ `sha2/flate2/tar/home/ctrlc/anyhow(thiserror en lib)`, `rand 0.8` solo dev; `rust-version="1.88"`, `edition="2021"`, `Cargo.lock` versionado en git, `[profile.release] opt-level=3, strip=true`; lista completa en `docs/specifications.md RNF-01`, detalle de features autoritativo en `docs/plan.md T0.3`). Sin dependencias externas pesadas ni servidores. (`sysinfo` para disco).
2. **No reinventar DSP:** constantes y orden de operaciones de sherpa-onnx/knf se copian exactos según §6. El riesgo es regresión numérica.
3. **Video nunca se re-codifica:** `-c:v copy`. Solo el audio se procesa.
4. **Fallo explícito y limpio:** exit codes, temps siempre borrados, `String::from_utf8_lossy` en Windows.
5. **CLI predecible para batch y scripting:** `--dry-run`, `--json`, `--skip-existing`, lote secuencial.
6. **Implementación legible Rust:** `stable 1.88+ edition 2021`, `PathBuf`, `clippy+rustfmt`, `anyhow` en bin / `thiserror` en lib, `log + env_logger` (`info`/`--verbose debug`), funciones pequeñas, sin estado global salvo sesiones `ort` cacheadas (`Mutex`).
7. **Testing por pirámide:** unitarias rápidas con `ffmpeg 6+` real sin red/modelo (`cargo test`, ), doradas DSP bloqueantes + e2e `remux` con `#[ignore]` (slow) vía `cargo test -- --ignored`.
8. **Documentación como contrato:** `README.md` (MIT + atribución Apache 2.0 Ceva-IP/DPDFNet) + `--help` + ejemplos idénticos a §4; `CHANGELOG.md` por release (inicial `1.0.0`); `LICENSE` MIT en raíz.

## 3. Estructura mínima propuesta (todo dentro de este repo, raíz `./`)

```
./ (Video-Noise-Remover/)
 docs/
 design.md # este documento (arquitectura + pipeline)
 specifications.md # RF/RNF canónicos + aceptación
 plan.md # orden de implementación
 LICENSE # MIT (código nuevo)
 README.md # instalación, 3 ejemplos, atribución Apache 2.0 © Ceva-IP/DPDFNet (modelo)
 CHANGELOG.md # por release (inicial 1.0.0)
 Cargo.toml # package/bin `denoise`, version="1.0.0" fuente única, edition 2021, rust-version 1.88, [profile.release] (todo `denoise`)
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
 common/si_sdr.rs # helper SI-SDR Rust puro (cada test lo declara: #[path = "common/si_sdr.rs"] mod si_sdr; D_h)
 test_golden.rs # voz real + ruido (3s SNR 0dB, 65s SNR 10dB) → mejora SI-SDR + paridad (#[ignore] slow)
 test_remux.rs # codec/res/fps/duración/bitrate, sin re-encode (#[ignore] slow)
 test_errors.rs # E_* + overwrite/skip (Ctrl+C no se simula en tests, solo manual T5.1)
 test_reporter.rs # orden [i/N], JSONL parseable, summary ok/failed/skipped
 data/README.md # spec vectores sintéticos + generador determinista (seed 0/1,, parte b: 0=par 3s, 1=par 65s )
 data/*.wav # vectores versionados (nunca se regeneran en CI)
 examples/
  gen_vectors.rs # generador con voz real del EvalSet (`cargo run --release --example gen_vectors -- <dir_eval>`, solo manual; no corre en `cargo test`)
```

## 4. Contrato CLI (v1)

```text
denoise INPUT... [--output-name NAME | --output-dir DIR] [--prefix STR] [--suffix STR]
  [--recursive] [--overwrite | --skip-existing]
  [--audio-bitrate KBPS] [--model-dir DIR] [--ffmpeg-path PATH]
  [--dry-run] [--json] [--verbose] [--version]
```

* `INPUT...`: 1..N rutas. Cada una puede ser archivo (`mp4/mov/mkv/webm/avi`) o directorio. Entry-point: bin `denoise` desde `src/main.rs` sobre lib `src/lib.rs` (`cargo run -- ...` equivalente, `version` desde `Cargo.toml`). Los tests de integración importan la lib, nunca el bin.
* `--audio-bitrate`: bitrate AAC de salida en kbps (defecto `192`, rango `64-320`).
* Reglas de salida (precedencia; `--output-name` y `--output-dir` son COMPLEMENTARIOS:
 1. Si lote expandido==1 y `--output-name NAME`: `DIR/<NAME>.mp4`, donde `DIR` es `--output-dir` si se pasa, sino cwd. Auto-`.mp4` si NAME no termina en `.mp4` case-insensitive (`final` → `final.mp4`). `prefix`/`suffix` se **ignoran** cuando `--output-name` está presente (D_o).: `--output-name` solo con lote==1; lote>1 → `E_INVALID_INPUT`.
 2. Si solo `--output-dir DIR`: `DIR/<relativo-a-cwd>/<prefix><stem><suffix>.mp4`, recreando subcarpetas si `--recursive` (`dirA/sub/x.mp4 → out/dirA/sub/x_denoised.mp4`; fuera de cwd → solo `stem` + aviso `--verbose`).
  3. Por defecto: junto al original como `<prefix><stem><suffix>.mp4` con `suffix=_denoised`, `prefix=""`.
 3b. Si 2+ entradas del lote resuelven al MISMO path de salida (`a.mp4`+`a.mov`→`a_denoised.mp4`), el 2º y siguientes reciben auto-sufijo incremental `_1`, `_2`... (`a_denoised_1.mp4`) con aviso SIEMPRE a `stderr` (no solo `--verbose`); aplica a las reglas 2 y 3. `--dry-run` muestra las salidas desempatadas ya en la tabla.
  4. Colisión sin `--overwrite`: error salvo `--skip-existing` (marca `skipped`, exit 0).
 5. Directorios padre de `--output-name`, `--output-dir` y `--model-dir` se crean siempre; si no creables → `E_IO`.
 6. `prefix/suffix` solo `[A-Za-z0-9._-]`, prohibidos `.` y `..` exactos; no ambos vacíos si salida in-place. `--output-name` que resuelve a la propia entrada sin `prefix/suffix` efectivo → `E_INVALID_INPUT` exit `2` SIEMPRE, incluso con `--overwrite` (endurecido — warning condicionado reemplazado por error invariante—).
  7. `--output-name` y `--output-dir` son **COMPLEMENTARIOS** (D_o): `--output-name` define el nombre base final; `--output-dir` el directorio. Cuando `--output-name` está presente, `prefix`/`suffix` se ignoran. La composición completa se documenta en `docs/specifications.md §5 RF-03` (tabla de decisión).
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
 → [81-95%] ffmpeg remux copy+AAC a OUT.part.mp4 (-t <dur_video> = duración del contenedor según D_d+D_k) → rename a OUT.mp4 (atómico con reemplazo en POSIX y Win — `MoveFileExW REPLACE_EXISTING`, D_j; la no-sobrescritura sin `--overwrite` la garantiza el colisión-check previo, no el rename)
  → [96-99%] verificar OUT existe y >0B + duración ±0.5s + Audio AAC → limpiar temps → [100%] report {ok,failed,skipped}
```

Responsabilidades estrictas:
* `cli.rs`: parseo `clap`, expansión determinista byte-wise UTF-8, dedup por absoluto lexical contra cwd sin resolver symlinks (case-insensitive solo Win), `resolve_output()` pura, bucle lote secuencial, reporte. No DSP. La expansión excluye `--output-dir` si está dentro de `INPUT` + aviso en `--verbose`.  Excluye además `*<suffix>.mp4` vigente en escaneos `--recursive` + aviso en `--verbose`.
* `pipeline.rs`: orquesta un video, traduce progreso a `0-100` según mapeo de §8 de este documento, garantiza limpieza + borrado parcial en error/`Ctrl+C`. Verificación ligera runtime: `>0B + duración ±0.5s + Audio AAC presente`.
* `df/`: solo `rustfft+ort`, firma `denoise_wav(in_wav: &Path, out_wav: &Path, progress_cb)` en `df/mod.rs`; `stft.rs` framing+STFT/iSTFT (réplica exacta knf), `net.rs` sesión stateful `ort` cacheada. Sin `Command`, sin `println!` en núcleo. I/O WAV con `hound` PCM16 ↔ `f32` (`/32768.0`, escala `*32768.0` con clip como `soundfile` PCM16).
* `ffmpeg_io.rs`: `find_ffmpeg()` (vía crate `which`, con `PATHEXT` en Win), `has_audio()`, `extract_mono48k()`, `remux_copy()`, `verify_output_ligero()`. Todo `Command` argv, `String::from_utf8_lossy`. Probe `ffmpeg -hide_banner -i` + regex `Duration:` = duración del CONTENEDOR, fuente de `<dur_video>` (sin decode extra del stream de video).
* `models.rs`: `ensure_models(model_dir, progress_cb)` tras trait `ModelsProvider` (`FakeProvider` en tests) + verificación tamaño + SHA.
* `errors.rs`: enum `E_*` + `exit_code()` con `thiserror` en lib / `anyhow` en bin (creado primero, usado sin I/O).

Comandos ffmpeg exactos a reimplementar:

```text
# 1. Extracción (mono 48k exigido por DPDFNet: primera pista determinista;
# en contenedores mov/mp4 se añade -ignore_editlist 1 antes de -i para alinear la línea de tiempo física)
ffmpeg -y -v error [-ignore_editlist 1] -i IN -map 0:a:0 -vn -ac 1 -ar 48000 TMP.in.wav

# 2. Remux (video intacto, audio limpio a AAC con bitrate parametrizable)
# D_d + D_k -t <dur_video> sustituye a
# -shortest; <dur_video> = duración del CONTENEDOR del probe (Duration:), no del stream de video —
# dato que el probe sin ffprobe no expone—. Si el audio limpio es más corto, la cola queda muda;
# si el audio del input es más largo que el video, la salida se alarga a la del contenedor (limitación v1, §9);
# en contenedores mov/mp4 se añade -ignore_editlist 1 para sincronía perfecta 1:1 con el video copiado.
ffmpeg -y -v error [-ignore_editlist 1] -i IN -i TMP.out.wav
  -map 0:v:0 -map 1:a:0 -c:v copy -c:a aac -b:a <audio-bitrate>k -t <dur_video> OUT.mp4
```

Detección de audio: `ffmpeg -hide_banner -i IN` contiene `Audio:`. Sin audio → error `E_NO_AUDIO`, no se genera salida.

Temporales por trabajo: `<out>.tmp.in.wav`, `<out>.tmp.out.wav` junto a la salida. Limpieza garantizada en todos los caminos salvo `--verbose` debug (con `--verbose` se conservan `.wav`/`.part` para inspeccionar). Handler `ctrlc` → `Child::kill` (`ffmpeg`) + cierre de inferencia, borra `.part` + temps, `exit 3` (2º `Ctrl+C` fuerza salida inmediata); la cancelación también aplica durante la descarga del modelo: flag atómico consultado entre retries + guardia `Drop` que borra el `.part` de la descarga (D_i; en Win hijo con `CREATE_NEW_PROCESS_GROUP`, observable único Win/POSIX. Fallos de escritura → `E_IO`.

## 6. Módulo `df/` — especificación DSP (no cambiar valores)

Parámetros fijos:

| Constante | Valor | Origen |
|---|---|---|
| `SR` | 48000, mono `f32` | contrato DPDFNet |
| `FFT/HOP` | 960 / 480, ventana Vorbis | metadatos del ONNX |
| `WNORM` | 1.0 (sin escala: el grafo recibe el espectro crudo) | sherpa-onnx/knf |
| `INV_FFT_SCALE` | `1/960` | `knf::IStft::InverseFFT` |
| `CENTER_CROP` | 480 (`FFT/2`) | `knf::IStft center=true` |
| `ISTFT_HEAD_CROP` | 1920 (`2*FFT`) | `ShiftWaveform` sherpa-onnx |

Secuencia (réplica exacta de sherpa-onnx + `knf`, sin trocear):

1. Padding `knf::Stft::Pad`: reflect de `FFT/2` por extremo sobre la señal cruda (izquierda excluye `signal[0]`, derecha excluye la última, en orden inverso). Frames `1 + n/HOP`.
2. `STFT = FFT(muestra * ventana Vorbis)` sin `wnorm` → `spec` (`torch.stft(normalized=False)`; verificado contra torch a `7.5e-9`).
3. Inferencia CPU, grafo único stateful: `spec[t] + state → spec_e[t] + state`; estado inicial = ceros + `erb_norm_init`/`spec_norm_init` de los metadatos. Se conservan las `T` salidas (verificado contra onnxruntime a `1e-10`).
4. `iSTFT`: inversa con escala `1/N` + ventana de síntesis + overlap-add (envolvente 1 en la región recortada), `center` (quitar 480 por extremo), descartar 1920 del frente + ceros al final (`ShiftWaveform`), truncar a `n`.
5. Sin normalización ni limiter: la escala es la del modelo. Escritura PCM16 como `soundfile` (`*32768` con clip).

Criterio de fidelidad: `SI-SDR(denoised,voz)-SI-SDR(mezcla,voz) >=5dB` y `SI-SDR(denoised,referencia_dpdfnet) >=60dB` en el par 3s, paridad `>=60dB` en el par 65s (bloqueante, +: estricto sin relajación; 55-59dB también bloquea). Vectores deterministas en `tests/data/` versionados en git, generados solo manualmente con `examples/gen_vectors.rs` (`cargo run --release --example gen_vectors -- <dir_eval>` con `Clean/`+`Noisy/` del EvalSet DPDFNet; nunca en `cargo test` ni CI, preserva el congelado): voz real 3s + ruido a `SNR 0dB`, `SR 48k` mono `PCM16` en disco —dato en memoria `f32` vía `i16→f32/32768.0`, mismo camino que el pipeline real, D_l— (`voz_clean.wav`, `voz_noisy.wav`, `referencia_dpdfnet.wav`: generada una vez con el propio port tras validar mejora, luego congelada;wav`, `voz65s_noisy.wav`, `referencia65s_dpdfnet.wav`) para estabilidad en clips largos (misma regla /). `SI-SDR` implementado Rust puro en `tests/common/si_sdr.rs` (zero-mean, `eps=1e-8`; cada test lo declara con `#[path = "common/si_sdr.rs"] mod si_sdr;`, D_h Cualquier desviación bajo umbrales = bug bloqueante. Referencias informativas: paridad 100dB entre corridas, media SI-SDR 14.74 dB en el EvalSet (igual que sherpa-onnx), bit-exactitud 1 LSB PCM16 vs sherpa-onnx.

## 7. Módulo `models.rs` — modelo autocontenido

* Artefacto único: `dpdfnet8_48khz_hr.onnx` (14857107B).
* Origen único: `https://huggingface.co/Ceva-IP/DPDFNet/resolve/main/onnx/dpdfnet8_48khz_hr.onnx` (URL canónica devuelve 200 con 14857107B exactos). Verificación: tamaño `14857107B >=98%` siempre + `SHA256 7b3afbb260a08fe9af3d16e3bda992971be1e7e951d1dee7c2d235f5c43f5631` obligatorio y mismatch → `E_MODEL_MISSING`; registrado en `docs/specifications.md RF-06/RNF-05` + este `§7`. Licencia del modelo: Apache 2.0 (Ceva-IP/DPDFNet).
* Cache: `--model-dir` (defecto `~/.cache/denoise/models/` vía `home_dir()+.cache` + `PathBuf` en Win/macOS/Linux) (todo `denoise`). Directorios padre de `--output-name`/`--output-dir`/`--model-dir` se crean siempre; si no creables → `E_IO`.
* Comportamiento: si falta → descarga con `User-Agent: denoise/1.0.0` (todo `denoise`), progreso `indicatif`, verificación tamaño + SHA. `timeout 30s + retry 3 con backoff`, chequeo espacio >=50MB libres en disco de `--model-dir` vía `sysinfo` antes de descargar (solo `model-dir`; `OUT/temps` → `E_IO` al fallar escritura), `.part + rename` (rename atómico con reemplazo en POSIX/Win, sin `remove_file` previo — mismo criterio D_j del remux). Si la descarga falla por red/modelo (timeout, HTTP, tamaño, SHA) → error `E_MODEL_MISSING` con URL y ruta manual esperada (disco → `E_IO`). (Sesión `ort` `CPUExecutionProvider` (binarios vía `download-binaries`), cacheada (`Mutex`). `reqwest blocking` sin dependencia directa a `tokio` (+; TLS vía `rustls-tls-webpki-roots` — raíces Mozilla empaquetadas, sin OpenSSL del sistema (D_a; coherente con `ort tls-rustls` ).

## 8. Módulo `ffmpeg_io.rs` + errores

Resolución: `--ffmpeg-path` → crate `which` en `PATH` (con `PATHEXT` en Win) → error `E_FFMPEG_NOT_FOUND`. Requiere `ffmpeg 6+`, verificado con `ffmpeg -version` con regex `ffmpeg version (\d+)\.` (major>=6, `from_utf8_lossy`), con fallback a prefijo `N-` (build git BtbN/gyan, aceptado como válido; si tampoco matchea → `E_FFMPEG_NOT_FOUND` con línea de versión cruda en el mensaje, D_c, sin `ffprobe` (probe y `has_audio()` vía `ffmpeg -hide_banner -i`). Receta por OS en `README.md`: Win `winget install Gyan.FFmpeg` / `choco install ffmpeg`, macOS `brew install ffmpeg`, Linux `apt install ffmpeg`.

Taxonomía estable (código → exit):
* `E_INVALID_INPUT` → 2: ruta inexistente, extensión no soportada, `0B`, `bitrate` fuera de `64-320`, `--output-name` con lote expandido `>1`, `--output-name` con `--output-dir` (complementarios por D_o; errores estructurales de flags → exit `2` en dry-run), `prefix+suffix` ambos vacíos con salida in-place, `--output-name` que resuelve a la propia entrada sin `prefix/suffix` efectivo (`E_INVALID_INPUT` SIEMPRE, incluso con `--overwrite`), directorio sin videos/lote vacío.  Solo-audio/corrupto en probe también `E_INVALID_INPUT`; `E_FFMPEG_FAILED` solo en `extract/remux`. (`--output-name` sin `.mp4` no es error, se auto-añade).
* `E_OUTPUT_EXISTS` → 2: destino existe sin `--overwrite` ni `--skip-existing`.
* `E_NO_AUDIO` → 2: sin pista `Audio:` (incluye solo-video).
* `E_FFMPEG_NOT_FOUND`, `E_MODEL_MISSING`, `E_FFMPEG_FAILED`, `E_IO` (disco lleno/sin permiso/sin espacio/`--output-name`/`--output-dir`/`--model-dir` no creables) → 1.
* `E_CANCELLED` (`Ctrl+C` cooperativo) → 3.

| Código | Caso | Salida |
|---|---|---|
| 0 | ok / `skipped` con `--skip-existing` | archivo verificado ligero |
| 1 | `E_FFMPEG_NOT_FOUND`, `E_MODEL_MISSING`, `E_FFMPEG_FAILED`, `E_IO` | stderr + no salida parcial |
| 2 | `E_NO_AUDIO`, `E_INVALID_INPUT`, `E_OUTPUT_EXISTS` | nada escrito |
| 3 | `E_CANCELLED` | proceso hijo matado (`Child::kill`, Win `CREATE_NEW_PROCESS_GROUP`), inferencia abortada, temps y `.part` borrados |

Batch nunca aborta en el primer fallo (salvo `Ctrl+C`): continúa y resume `ok/failed/skipped` con exit: `3` si hubo cancelación, si no `1` si hubo algún fallo `1`, si no `2` si hubo fallos `2`, si no `0`.

Reporte dinámico pero ordenado (sin nuevos flags):
* Humano (defecto, TTY): cabecera `[i/N] in → out`, una barra viva por video a `stderr` (`indicatif`; `hidden()` con `--json`/sin TTY), más línea final por video `done|failed|skipped + MB + segundos`. Resumen final siempre visible. Flush explícito, sin emojis, ASCII seguro en `pwsh`.
* Humano sin TTY/CI: sin animación (`hidden()`), líneas ` [i/N] name ... 45% msg` cada cambio de fase.
* `--verbose`: `log + env_logger` (`info`/`debug`), añade a `stderr` comando `ffmpeg` exacto, `model-dir`, tiempos por fase, tamaños `in.wav/out.wav`, y conserva temps.
* `--json`: desactiva animación; `stdout` = `JSONL` una línea por archivo `{input,output,status,message,pct}` + línea final `{summary:{ok,failed,skipped}}` (`status ∈ {ok,failed,skipped,dry-run}`, `pct` según mapeo `0/1-5/6-80/81-95/96-99/100` —`skipped` SIEMPRE `pct=0`, D_m—, `summary` sin `pct`); progreso humano suprimido. Parseable con `jq`. Ver `docs/specifications.md RF-08`. (`--json + --verbose` combinables —debug a `stderr` + conserva `.wav`/`.part`, `stdout` intacto).
* `--dry-run`: tabla `input → output (skip: motivo)` sin escribir/crear nada, mismo orden que el lote real, solo lectura `stat` para colisión; sin `ffmpeg/modelo/has_audio` (+ (con `--json` emite `JSONL` con `status="dry-run" pct=0`). (exit `0` con CLI estructuralmente válida; errores estructurales —D_o flags inválidos/combinados, bitrate fuera de rango, prefix+suffix inválidos, lote vacío— fallan antes de simular con exit `2` sin reporte; per-archivo se reporta con exit `0`). (el dry-run SIMULA la intención de los flags de colisión — destino existente con `--overwrite` → `would overwrite`, sin `--overwrite` → `would fail: E_OUTPUT_EXISTS`, con `--skip-existing` → `would skip`; per-archivo, exit `0` según D_f).
* `--version`: imprime `denoise 1.0.0 + modelo DPDFNet + ffmpeg <ver>` desde `Cargo.toml` vía `env!("CARGO_PKG_VERSION")` (todo `denoise`) (sin `ffmpeg` imprime `ffmpeg missing`). Ver `docs/specifications.md RF-10`.

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

1. `test_naming` (rápido, sin ffmpeg/red): 1 archivo, N archivos, `--output-dir`, `prefix/suffix`, `--output-name` solo lote==1 → error si lote `>1` (; vale dir con 1 video), `--output-name` + `--output-dir` complementarios (D_o), colisión + `overwrite/skip`, `--recursive` recrea árbol relativo a cwd (2 dirs mismo `sub/x.mp4` → 2 salidas) + dedup lexical + exclusión output anidado + exclusión `*<suffix>.mp4`, caracteres inválidos + `.`/`..` → `E_INVALID_INPUT`, `--output-name` resolviendo a la propia entrada sin `prefix/suffix` → `E_INVALID_INPUT` (D_p).
2. `test_golden` (`#[ignore]` slow): generador Rust (`cargo run --release --example gen_vectors -- <dir_eval>`, voz real del EvalSet DPDFNet): par 3s a `SNR 0dB` + par 65s a `SNR 10dB`, `SR 48k` mono `PCM16` en disco (dato en memoria `f32`, D_l) en `tests/data/` → `mejora >=5dB` y `paridad vs referencia >=60dB` en el par 3s, paridad `>=60dB` en el par 65s. Falla si DSP difiere.
3. `test_remux` (`#[ignore]` slow): fixture `ffmpeg -y -v error -f lavfi -i testsrc=size=640x480:rate=30:duration=5 -f lavfi -i sine=frequency=440:sample_rate=48000:duration=5 -c:v libx264 -pix_fmt yuv420p -c:a aac -b:a 192k -shortest fixture.mp4` → mismo `vcodec/res/fps`, duración `±0.2s`, `acodec=aac,sr=48000,bitrate±10%`, sin re-encode: `vcodec`/`res`/`fps`/`extradata` iguales + `sha256` de `ffmpeg -y -v error -i OUT.mp4 -map 0:v:0 -c copy -f h264 -` idéntico al de la entrada (prohibido comparar tamaño fichero total).
4. `test_errors`: sin audio/solo-video → `E_NO_AUDIO`; solo-audio/corrupto en probe → `E_INVALID_INPUT`; ffmpeg ausente → `E_FFMPEG_NOT_FOUND`; descarga rota → `E_MODEL_MISSING`; destino existe → `E_OUTPUT_EXISTS`/`skipped`; `bitrate 9999` → `E_INVALID_INPUT`; `--output-name` resolviendo a la propia entrada sin `prefix/suffix` → `E_INVALID_INPUT` (D_p, incluso con `--overwrite`); disco/sin permiso/`--output-name`/`--output-dir`/`--model-dir` no creables → `E_IO`. Nada parcial en disco.
5. `test_reporter`: lote 3 simulado verifica orden `[1/3..3/3]`, `JSONL` parseable, resumen `ok/failed/skipped`, sin animación con `--json` o sin TTY.
6. Manual: 1 video corto Win + lote 5 videos, `--dry-run` primero (exit `0`, ), luego real + `--json`. Comandos: `cargo test` y `cargo test -- --ignored`.
