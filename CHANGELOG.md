# Changelog

## [Unreleased]

### Features
- Gate de pausa en la síntesis (`src/df/mod.rs::apply_pause_gate`): VAD solo-salida sobre RMS por marcos de 1440 muestras (30 ms) — voz si `rms >= pico−30 dB`, salida de voz si `rms <= pico−45 dB` durante 3 marcos, hangover de 5 — que atenúa −25 dB las pausas con fundidos raised-cosine de 30 ms, justo antes de cuantizar a PCM16. Por qué: el residuo que el modelo deja en las pausas estaba muy por encima de la voz limpia y hundía SI-SDR, p5, STOI y PESQ; la voz queda multiplicada por 1.0 (bit-idéntica a la síntesis) y no hay ganancia, normalización global ni limiter. Validado 7/7 en la puerta de calidad sin regresiones (`docs/benchmark.md` §5) y con la mejora ratificada después en `tools/quality/baseline.json` (ver `Calidad`); la puerta sigue en verde en la corrida posterior a la ratificación.

### Docs
- Contrato documental actualizado tras el gate de pausa: `docs/design.md` §6 (secuencia con el paso del gate, bit-exactitud pre-gate y estado de los dorados), `docs/specifications.md` (RF-05, RNF-04, §6 y §6.1 con el estado real sin relajar umbrales), `README.md` (características, limitaciones, arquitectura con el paso del gate y estado de los dorados en §9.1; la bit-exactitud de la salida completa queda documentada en `design.md` §6 y RNF-04), `tests/data/README.md` (referencias congeladas: la del 3 s pre-gate, la del 65 s re-congelada tras el gate) y `docs/benchmark.md` §2/§5 (línea base marcada como pre-gate y resultado medido con el gate).
- `README.md`: overhaul de precisión y estructura. Secciones y subsecciones numeradas (`1.`-`11.` y `N.M`) con índice navegable de 25 entradas; nuevas secciones `Limitaciones Conocidas` y `Troubleshooting` (tabla error → exit → causa → solución); subsección de coste del primer uso (modelo ~15 MB, red solo en la primera corrida, ~50 MB libres); contrato CLI exacto (`--output-name`, `--output-dir`, `--json` como JSONL en streaming, `--dry-run`, `--verbose`, `--version`) y ejemplos corregidos.
- `docs/design.md` y `docs/specifications.md`: índices navegables nuevos, con la numeración `§N` conservada por ser contrato citado desde `src/` y los tests. Numeración de §4 normalizada a `1.`-`8.`, referencias cruzadas rotas corregidas y residuos de sintaxis limpiados.
- Documentación alineada con el comportamiento real del código: se retiran afirmaciones que ninguna verificación implementaba (`±0.2s` de duración en RF-05 y `±10%` de bitrate en RF-05B, ambos sustituidos por los umbrales realmente verificados, `±0.5s`), y se corrige la descripción de `--verbose` en RF-10, que atribuía un log de comandos ffmpeg inexistente.
- Eliminados `docs/plan.md` y `verify.ps1`; el fixture manual renombrado a `assets/e2e_vertical_1080x1920_16s.mp4`.
- Corrección del drift documental tras la ratificación de la baseline y el re-congelado de la dorada 65 s: `docs/benchmark.md` (§2 ya no presenta la tabla pre-gate como baseline comprometida, §4 con SNR 0/5 dB, columna RTF de §5 etiquetada como medida de aquella corrida con el rango ratificado 0,69–1,14, §6 marcado como pre-gate, §8 con el conteo real 8→7 clips), `docs/specifications.md` (RNF-03: RTF como objetivo `<=1.0` frente a 1,06–1,19 medidos y telemetría sin desglose por fase; RNF-04/RNF-07 con el estado real de `tests/data/` y de los módulos), `tools/quality/README.md` (cuatro métricas, puerta completa de 9 tolerancias, `run` —no `score`— para `--update-baseline`, SNR 0/5 dB), `tests/data/README.md` y `README.md` §9.1/§10 (generación real de cada vector y números solo en `docs/`), y docstrings de `metrics.py`, `src/df/mod.rs`, `src/pipeline.rs`, `src/models.rs` y `examples/gen_vectors.rs`.

### Tests
- `test_golden_65s`: el log marcaba la mejora con un umbral `mín 5.0 dB` que ese par no asserta (solo verifica paridad); ahora se informa como dato no verificado.
- Paridad de los dorados en verde tras el gate: `test_golden_3s` pasa con 67,77 dB (su referencia 3 s sigue congelada pre-gate: el par es 90 % voz y el gate casi no altera su salida) y `test_golden_65s` vuelve a pasar con 100,00 dB después de re-congelar `tests/data/referencia65s_dpdfnet.wav` con este mismo port sobre el `voz65s_noisy.wav` congelado (`examples/process_wav.rs`, mismo camino `denoise_wav`). El umbral ≥60 dB no se relaja y ningún fixture de voz se toca; `cargo test --release -- --ignored` queda en verde 5/5.

### Calidad
- Ratificación deliberada de `tools/quality/baseline.json` con `run --set tests_data --work out/bench --update-baseline` una vez verificada la mejora del gate (regla de oro de `docs/benchmark.md` §7: no moverla para que un resultado pase, solo para congelar una mejora aprobada); la puerta vuelve a dar «sin regresiones respecto a la linea base» en la corrida posterior.
- `tools/quality/`: benchmark periódico de calidad del DSP con tres métricas complementarias (SI-SDR para ruido eliminado, STOI para voz inteligible, PESQ para naturalidad), harness Rust `examples/process_wav.rs` (misma ruta que `denoise` internamente), manifiesto `clips.json` y puerta de regresión contra `baseline.json` comprometida.
- Batería ampliada: SI-SDR por ventanas con percentil 5 (peor segundo audible, excluye silencio digital y contenido 50 dB bajo el pico), sonoridad LUFS BS.1770 (validada a 0,000 contra teoría), RTF por clip desde el harness y DNSMOS SIG/BAK/OVR sin referencia (vía `torchmetrics`, modelo oficial Microsoft, 2,4 MB). Puerta de 9 métricas con tolerancias justificadas por datos (DNSMOS: desvío 0,000000 en 5 corridas → OVR ±0,1, SIG/BAK ±0,15).
- `docs/metrics.md`: significado de cada métrica explicado sin jerga; `docs/benchmark.md`: registro histórico, con línea base DPDFNet 2026-10-03 (par 3 s a SNR 0 dB: +6,68 dB SI-SDR, STOI 0,838, PESQ 1,196; par 65 s a SNR 10 dB: +2,67 dB).
- `README.md`: nueva subsección `9.3. Benchmark de calidad de audio` y árbol de `§10` actualizado con los ficheros nuevos.
- Batería `tests_data` de 8 a 7 clips: sale `voz_65s` (debajo del mínimo de voz activa, ya descalificado en `docs/benchmark.md` §3); `baseline.json` pierde solo su clave (resto intacto, RTF preservados), WAVs intactos para `test_golden_65s`. Sin cambio DSP. Interino hasta reemplazarlo por un clip largo representativo.
- Puerta a prueba de no finitos en `check_baseline()`: una métrica de puerta (`GATED_LOWER`, `lufs_delta`, `rtf`) con `NaN` o `±inf` — ya sea el valor actual o el de la baseline — es ahora fallo explícito con clip, métrica y valor (antes la comparación daba `False` y la puerta aprobaba en silencio un aprobado fantasma); `None` (métrica no calculada) sigue saltándose como hasta ahora.

## 1.0.0 (2026-09-13)

### Features
- Versión inicial de `denoise` CLI v1.0.0 Rust (DPDFNet ONNX + ort CPU).
- Batch secuencial, mono, primera pista de audio. Video con `-c:v copy`.
- Salida `.mp4` con audio AAC limpio al bitrate pedido.
- Flags: `--output-name`, `--output-dir`, `--prefix`, `--suffix`, `--recursive`, `--overwrite|--skip-existing`, `--audio-bitrate`, `--model-dir`, `--ffmpeg-path`, `--dry-run`, `--json`, `--verbose`, `--version`.
- Modelo DPDFNet (`dpdfnet8_48khz_hr.onnx`, Ceva-IP, Apache 2.0): descarga verificada (SHA256), cacheada en `~/.cache/denoise/models/`. Grafo único stateful, inferencia secuencial sin trocear, salida bit-exacta con sherpa-onnx (1 LSB PCM16, media SI-SDR 14.74 dB en el EvalSet, RTF ≤ 1.0).
- Cancelación con `Ctrl+C` → exit 3, sin archivos parciales.
- Sin normalización artificial: la salida conserva la escala exacta del modelo.
- Sincronización A/V perfecta en contenedores `mov/mp4` con edit lists (`-ignore_editlist 1`).

### Docs
- Especificación completada: `docs/design.md`, `docs/specifications.md`.
- Autocontenido: binario nativo standalone, sin servidor, sin desktop app.
