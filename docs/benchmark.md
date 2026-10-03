# Registro de benchmark de calidad

> Historial de resultados de calidad del DSP a medida que se refina la
> implementación. Qué significa cada métrica se explica en `docs/metrics.md`;
> aquí solo hay números, método y lectura. Convención de rutas:
> `docs/<fichero>` es relativo a la raíz del repo.

## Índice

1. [Método](#1-método)
2. [Línea base: DPDFNet 2026-10-03](#2-línea-base-dpdfnet-2026-10-03)
3. [Lectura de la línea base](#3-lectura-de-la-línea-base)
4. [Limitaciones del set actual](#4-limitaciones-del-set-actual)
5. [Historial](#5-historial)

## 1. Método

```bash
python tools/quality/benchmark.py run --set tests_data --work out/bench
```

El comando ejecuta el DSP de producción sobre cada clip (vía el harness
`examples/process_wav.rs`, que invoca la misma ruta que `denoise` usa
internamente) y puntúa la salida contra la voz limpia real. Las salidas
generadas quedan en `out/bench/` (no se commitean; `out/` está en
`.gitignore`). La línea base comprometida vive en
`tools/quality/baseline.json` y el benchmark falla si alguna métrica cae
respecto a ella.

Comparar dos implementaciones es correr el mismo comando en cada build con
distinto `--work` (y distinto `--model-dir` si usan modelos distintos) y
contrastar las tablas. No hay normalización implícita: todo se mide contra la
voz limpia real, no contra ningún oráculo.

## 2. Línea base: DPDFNet 2026-10-03

Motor: grafo único `dpdfnet8_48khz_hr.onnx` (Ceva-IP, Apache 2.0), pipeline
stateful sin troceado ni normalización de salida.

| Clip | SI-SDR entrada → salida | Ganancia | STOI entrada → salida | PESQ entrada → salida | Nivel Δ | Retardo | Voz % |
|---|---|---|---|---|---|---|---|
| voz, 3 s, ruido a SNR 0 dB | +0,06 → +6,74 | **+6,68** | 0,609 → 0,838 | 1,046 → 1,196 | −2,2 dB | 0 | 90 % |
| voz, 65 s, ruido a SNR 10 dB | +10,0 → +12,67 | **+2,67** | 0,937 → 0,975 | 1,990 → 3,388 | −0,1 dB | 0 | 20 % ⚠️ |

## 3. Lectura de la línea base

- **El DSP funciona.** Desde 0 dB de SNR recupera +6,68 dB con STOI
  acompañando (0,61 → 0,84): antes había que adivinar palabras, ahora se
  entiende. Medido contra grabación real de voz, sin oráculos.
- **PESQ queda bajo en el par de 3 s** (1,20 sobre 4,5): el ruido se fue pero
  la voz conserva timbre procesado. Es el hueco de calidad medido y apunta a
  artefactos de deformación, no a ruido residual.
- **El par de 65 s avisa solo**: 20 % de voz activa, el resto es silencio casi
  digital. Su ganancia global (+2,67) subestima el rendimiento real; la
  métrica restringida a voz da +21,39 dB (ver `report.json` del run). Ese clip
  no sirve como benchmark de calidad hasta que se reemplace.
- **Sin desincronización**: retardo 0 en ambos. El desplazamiento interno de
  1920 muestras del DSP queda compensado y no afecta el A/V.
- **Nivel estable**: −2,2 / −0,1 dB respecto a la entrada, dentro de lo apenas
  perceptible. No hay regresión de volumen por la falta de normalización.

## 4. Limitaciones del set actual

El set `tests_data` es mínimo y heredado: 2 clips, 1 idioma, 2 condiciones de
ruido, y el par largo dominado por silencio. Además el "ruido" de los fixtures
se fabricó re-mezclando una grabación ya ruidosa, no es ruido real aislado.
Cualquier optimización puntuada solo contra este set corre el riesgo de
ajustarse a la medición en vez de a la calidad. El paso pendiente es ampliarlo
(EvalSet DPDFNet, selección por actividad de voz, 3 SNRs × 3 condiciones).

## 5. Historial

| Fecha | Cambio | SI-SDR 3 s | STOI 3 s | PESQ 3 s | SI-SDR 65 s | Notas |
|---|---|---|---|---|---|---|
| 2026-10-03 | Línea base DPDFNet | +6,68 | 0,838 | 1,196 | +2,67 | Primera medición con las tres métricas |

Añadir una fila por cada cambio que toque el DSP, con el comando del §1
re-ejecutado. Si la fila nueva empeora alguna métrica respecto a
`baseline.json`, el propio benchmark lo marca antes de commitear.
