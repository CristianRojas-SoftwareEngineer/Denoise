# Benchmark de calidad de audio

Herramienta permanente del proyecto para medir la calidad del DSP y detectar
regresiones. Complementa a `test_golden` (Rust): el test es una puerta rápida
sin dependencias, esta herramienta es el análisis completo con tres métricas.

## Por qué existe

Medir solo SI-SDR era insuficiente. Un denoiser que sobre-suprime (quita ruido
de más y deforma la voz) **mejora** SI-SDR y **empeora** la voz resultante, y
ninguna puerta anterior lo detectaba. Las tres métricas juntas lo cubren:

| Métrica | Qué mide | Eje |
|---|---|---|
| SI-SDR | cuánto ruido se eliminó | reducción de ruido |
| STOI | cuánta voz quedó inteligible | aislación de voz |
| PESQ MOS-LQO | calidad perceptual global | distorsión + ruido residual |
| DNSMOS SIG/BAK/OVR | voz / fondo / global sin referencia | las tres, sobre videos reales |

Regla de lectura: si SI-SDR mejora pero STOI/PESQ no, el modelo está
sobre-suprimiendo. Eso es una regresión aunque el SI-SDR suba.

Además se reportan nivel de salida (percentil 99.5, dBFS), desfase respecto a
la verdad (muestras; detecta desincronización A/V) y fracción de voz activa.

## Instalación

```bash
pip install -r tools/quality/requirements.txt
```

## Uso

```bash
# 1. Ejecutar el DSP sobre el set y puntuar (usa el harness de Rust,
#    que invoca la misma ruta que `denoise` internamente):
python tools/quality/benchmark.py run --set tests_data --work out/bench

# 2. Puntuar salidas ya generadas (p. ej. para comparar dos builds):
python tools/quality/benchmark.py score --outputs out/bench

# 3. Actualizar la línea base tras un cambio intencionado:
python tools/quality/benchmark.py score --outputs out/bench --update-baseline
```

`--work` y `--outputs` pueden ser el mismo directorio. `out/` está en
`.gitignore`: los artefactos no se commitean, solo `baseline.json`.

## Comparar dos builds (p. ej. DFN3 vs DPDFNet)

```bash
# build A en su worktree, con su propio model-dir:
python tools/quality/benchmark.py run --root ../denoise-otro \
    --set tests_data --work out/bench-otro --model-dir C:/temp/models-otro
# comparar:
python tools/quality/benchmark.py score --outputs out/bench-otro
```

El `run` de cada build genera su propio reporte; la comparación es manual
entre tablas. No hay factor de normalización implícito: `score` mide contra la
voz limpia real, no contra ningún oráculo.

## Puerta de regresión

`baseline.json` es la línea base comprometida. `score` falla (exit 1) si:

- ganancia SI-SDR cae más de 0.5 dB,
- STOI de salida cae más de 0.02,
- PESQ de salida cae más de 0.05.

Actualizala **solo** cuando el cambio sea intencionado y hayas verificado que la
nueva base es realmente buena, no solo distinta.

## Añadir clips

`clips.json` define los sets. Cada clip necesita `{clean, noisy}` con la misma
tasa de muestreo; la salida la genera `run`. Criterios para que un clip sirva:

- voz real, no tonos sintéticos (un denoiser trata un tono como ruido tonal);
- fracción de voz activa > 35 % (la herramienta avisa si es menor);
- varias SNRs (0/5/10 dB) y condiciones de ruido;
- la referencia limpia debe ser una grabación real, no salida de otro modelo.

El set actual (`tests_data`) es mínimo: el par heredado de 3 s más 6 ventanas
de 15 s. El par de 65 s salió del set el 2026-10-03 (dominado por silencio,
debajo del mínimo de voz activa); reemplazarlo por un clip largo
representativo es el paso pendiente.

## Detalles de implementación

- `metrics.py:si_sdr` reproduce exactamente la convención de
  `tests/common/si_sdr.rs` (zero-mean, `eps = 1e-8`, tope 100.0) para que los
  números del benchmark y del test Rust sean comparables.
- PESQ solo admite 8000/16000 Hz: se remuestrea a 16 kHz. Y la firma real es
  `pesq(fs, ref, deg, mode)` — la frecuencia va **primero**, aunque su
  docstring la liste en otro orden.
- SI-SDR restringido a voz (`si_sdr_gain_speech`, en el JSON del reporte)
  evita que el silencio domine la métrica.
- DNSMOS usa el modelo oficial de Microsoft (`sig_bak_ovr.onnx` P.808 y P.835
  vía `torchmetrics`): primera ejecución lo descarga (~2,4 MB) a
  `~/.torchmetrics/DNSMOS`, después funciona offline. Sin el modelo devuelve
  `n/a` en vez de fallar, igual que STOI/PESQ sin su paquete.
- Nunca uses un nombre de archivo que sombree un módulo stdlib (`struct.py`,
  `wave.py`, …): rompe cualquier script Python que corra en ese directorio.
