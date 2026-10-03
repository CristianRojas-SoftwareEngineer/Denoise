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
6. [Set ampliado a 8 clips (2026-10-03)](#6-set-ampliado-a-8-clips-2026-10-03)
7. [Cómo iterar](#7-cómo-iterar)
8. [Salida del par 65 s del set (2026-10-03)](#8-salida-del-par-65-s-del-set-2026-10-03)

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
stateful sin troceado ni normalización de salida. Estos números son la línea
base comprometida (`tools/quality/baseline.json`), medida **antes** del gate
de pausa: el resultado con gate está en §5.

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

El set `tests_data` tiene 7 clips a SNR 0/5/10 dB, todos con voz española real del
EvalSet DPDFNet: el par heredado de 3 s más 6 ventanas nuevas de 15 s con
>=40 % de voz activa, cortadas de clips largos y remuestreadas de 16 kHz a
48 kHz, usando las mezclas nativas (sin re-escalado). Desde 2026-10-03 sale
del set el par heredado `voz_65s` (ver §8).

Límites restantes: un solo idioma; ventanas cortas (15 s, más el par heredado
de 3 s); el par heredado `voz_65s` salió del set por estar dominado por
silencio y no alcanzar el mínimo de voz activa (ver §8). Ampliar idioma y
duración queda como trabajo futuro.

## 5. Historial

| Fecha | Cambio | SI-SDR 3 s | STOI 3 s | PESQ 3 s | SI-SDR 65 s | Notas |
|---|---|---|---|---|---|---|
| 2026-10-03 | Línea base DPDFNet | +6,68 | 0,838 | 1,196 | +2,67 | Primera medición con las tres métricas |
| 2026-10-03 | Batería ampliada (p5, LUFS, RTF, DNSMOS) | +6,68 | 0,838 | 1,196 | +2,67 | Mismo DSP; la puerta ahora cubre 9 métricas. Detalle abajo |
| 2026-10-03 | Batería 7 clips (sale `voz_65s`) | = | = | = | — | Sin cambio DSP; ver §8 |
| 2026-10-03 | Gate de pausa: −25 dB en pausas, voz ×1.0 | +6,68 | 0,838 | 1,196 | — | 7/7 verde en la puerta, `baseline.json` sin mover. Detalle abajo. `test_golden_65s` en rojo por referencia pre-gate (ver `docs/design.md` §6) |
| 2026-10-03 | Ratificación de `baseline.json` + re-congelado de la dorada 65 s | = | = | = | — | `--update-baseline` tras verificar la mejora; `referencia65s_dpdfnet.wav` regenerada sobre `voz65s_noisy.wav` → `test_golden_65s` en verde con 100,00 dB (umbral sin relajar). Puerta en verde de nuevo en la corrida posterior |

Batería ampliada (mismo DSP, mismos clips):

| Clip | p5 | LUFS Δ | RTF | DNSMOS SIG/BAK/OVR |
|---|---|---|---|---|
| 3 s | +3,8 | −4,2 | 1,09 | 3,33 / 3,94 / 3,03 |
| 65 s | +11,6 | +0,7 | 1,40 | 3,11 / 3,88 / 2,71 |

Lectura: sin daño localizado real (el p5 bajo inicial era silencio digital,
ver §3); sonoridad estable en LUFS salvo −4,2 en el par 3 s (ruido eliminado
pesa en la sonoridad); RTF incluye la carga del modelo en el primer clip.
DNSMOS confirma BAK casi al techo y SIG restaurado. Tolerancias DNSMOS
(OVR ±0,1, SIG/BAK ±0,15) fijadas con desvío 0,000000 en 5 corridas.

Gate de pausa (2026-10-03, `src/df/mod.rs::apply_pause_gate`, set
`tests_data` de 7 clips, comando del §1; números de `out/bench/report.json`
de esa corrida):

| Clip | SI-SDR entrada → salida | Ganancia | p5 | STOI entrada → salida | PESQ entrada → salida | SIG/BAK/OVR |
|---|---|---|---|---|---|---|
| voz_3s | +0,06 → +6,74 | +6,68 | +3,79 | 0,609 → 0,838 | 1,046 → 1,196 | 3,32 / 3,97 / 3,04 |
| pub_snr0 | −1,36 → +9,32 | +10,68 | +5,36 | 0,730 → 0,913 | 1,198 → 2,080 | 3,10 / 4,10 / 2,89 |
| car_snr0 | −1,92 → +11,70 | +13,62 | −3,01 | 0,849 → 0,955 | 1,124 → 2,588 | 3,48 / 4,03 / 3,19 |
| car_snr5 | +2,74 → +13,15 | +10,41 | −2,32 | 0,872 → 0,944 | 1,249 → 2,232 | 3,38 / 4,12 / 3,15 |
| office_snr0 | −2,18 → +8,93 | +11,11 | −23,02 | 0,741 → 0,914 | 1,122 → 2,189 | 2,99 / 3,93 / 2,73 |
| train_snr5 | +3,50 → +15,84 | +12,34 | +11,97 | 0,938 → 0,983 | 1,392 → 2,833 | 3,47 / 4,16 / 3,23 |
| restaurant_snr0 | −1,14 → +7,74 | +8,89 | −15,10 | 0,637 → 0,822 | 1,076 → 1,737 | 2,83 / 4,06 / 2,62 |

| Clip | Nivel Δ | LUFS Δ | RTF | Voz % |
|---|---|---|---|---|
| voz_3s | −2,2 dB | −4,2 | 0,99 | 90 % |
| pub_snr0 | −1,5 dB | −2,3 | 0,91 | 52 % |
| car_snr0 | −1,5 dB | −0,7 | 0,90 | 43 % |
| car_snr5 | −0,6 dB | +1,5 | 1,00 | 46 % |
| office_snr0 | −2,2 dB | −2,3 | 1,02 | 48 % |
| train_snr5 | −0,4 dB | +1,4 | 0,88 | 48 % |
| restaurant_snr0 | −1,1 dB | −2,0 | 0,73 | 47 % |

Lectura: puerta en verde — "sin regresiones" respecto a `baseline.json`,
que en esa corrida **no se movió** (se ratificó después, ver fila y decisión
más abajo), y ninguna métrica de puerta empeora. Mejoras medidas
frente a la línea base: `car_snr0` PESQ 2,472 → 2,588 (+0,116);
`office_snr0` DNSMOS SIG 2,926 → 2,986, BAK 3,869 → 3,927 y
OVR 2,656 → 2,731; `train_snr5` BAK 4,113 → 4,159 y OVR 3,165 → 3,234;
el resto queda dentro del ruido de medición. Nivel acotado (−0,4 a
−2,2 dB), LUFS entre −4,2 y +1,5 y RTF 0,73–1,02: el gate solo atenúa
pausas, la voz sale multiplicada por 1.0 y no hay ganancia, normalización
ni limiter.

Decisiones tomadas el 2026-10-03 tras aprobarlas: la mejora **se ratificó**
en `tools/quality/baseline.json` con `run --set tests_data --work out/bench
--update-baseline` (el §7 exige aprobación para moverla; la puerta volvió a
dar «sin regresiones» en la corrida posterior) y
`tests/data/referencia65s_dpdfnet.wav` **se re-congeló** con el propio port
sobre `voz65s_noisy.wav` sin relajar el umbral `>=60 dB`, de modo que
`test_golden_65s` vuelve a verde con paridad 100,00 dB (ver
`docs/design.md` §6).

## 6. Set ampliado a 8 clips (2026-10-03)

Seis pares nuevos de 15 s (pub/car/office/train/restaurant a SNR 0/5,
>=40 % voz activa, mezclas nativas del EvalSet). Ganancias SI-SDR de
+8,9 a +13,6 dB con STOI acompañando (0,82–0,98) y DNSMOS BAK ~4:

| Clip | SI-SDR | p5 | STOI | PESQ | SIG/BAK/OVR |
|---|---|---|---|---|---|
| pub_snr0 | +10,68 | +5,4 | 0,912 | 2,089 | 3,11 / 4,10 / 2,89 |
| car_snr0 | +13,62 | −3,0 | 0,955 | 2,472 | 3,42 / 3,94 / 3,10 |
| car_snr5 | +10,41 | −2,3 | 0,944 | 2,254 | 3,38 / 4,12 / 3,15 |
| office_snr0 | +11,13 | −23,0 | 0,914 | 2,201 | 2,93 / 3,87 / 2,66 |
| train_snr5 | +12,35 | +12,0 | 0,983 | 2,838 | 3,42 / 4,11 / 3,16 |
| restaurant_snr0 | +8,88 | −15,1 | 0,822 | 1,751 | 2,82 / 4,09 / 2,62 |

Lectura: el p5 encuentra debilidad real en pasajes silenciosos, en ambas
direcciones — fuga de ruido donde la verdad es tenue (salida más fuerte que
la limpia: office, restaurant) y sobre-supresión (salida mucho más baja:
auto). Son los peores segundos audibles, no artefactos de medición, y es
exactamente la clase de defecto que la media escondía. La puerta (tolerancia
1 dB sobre cambios, con DSP determinista) los vigila sin falsos positivos.

Añadir una fila por cada cambio que toque el DSP, con el comando del §1
re-ejecutado. Si la fila nueva empeora alguna métrica respecto a
`baseline.json`, el propio benchmark lo marca antes de commitear.

## 7. Cómo iterar

El loop de mejora es: hipótesis → un cambio → medir → decidir → registrar.

**1. Hipótesis.** Cada iteración responde una pregunta concreta que dice qué
métrica debe moverse:

| Si trabajas en… | Tu dial es… | El resto debe… |
|---|---|---|
| eliminar más ruido | SI-SDR, BAK | no moverse |
| naturalidad de la voz | PESQ, SIG | no moverse |
| volumen consistente | LUFS | no moverse |
| velocidad | RTF | no moverse |

**2. Un cambio.** Uno por iteración en el DSP. Con dos cambios no se puede
atribuir el resultado; con uno, el veredicto es inequívoco.

**3. Medir.** Benchmark para calidad + suite Rust para integración; la
iteración no está evaluada hasta que pasan ambas:

```bash
python tools/quality/benchmark.py run --set tests_data --work out/bench
cargo test --release
```

**4. Decidir.** Puerta en verde + tu dial subió = mejora real (congélala con
`--update-baseline`). Puerta en rojo = regresión: no toques la baseline,
diagnostica con la métrica y el clip que el reporte indica (el p5 dice en qué
segundo mirar; ante la duda manda el oído). Todo igual = hipótesis falsa,
se revierte sin costo.

**5. Registrar.** Cada cambio deja tres rastros: `baseline.json` actualizada
(solo si la mejora fue deliberada y verificada), fila en §5/§6 y commit.

**Regla de oro.** La baseline nunca se mueve para que un resultado pase;
solo para ratificar una mejora que ya pasó. Es manual y deliberada por
diseño: es la única forma de romper este sistema.

## 8. Salida del par 65 s del set (2026-10-03)

`voz_65s` sale de `tests_data`: queda muy por debajo del mínimo de voz
activa que exige `tools/quality/README.md`, el resto es silencio casi
digital y su ruido se fabricó re-mezclando en vez de usar la mezcla nativa.
Ya estaba descalificado en §3 y la propia herramienta lo avisa en cada
corrida. Los veredictos medidos con 8 clips (mezcla, gate, pausas) se
mantienen en el historial tal cual se midieron; no se reescriben.

Cambios: `tools/quality/clips.json` (7 clips), `tools/quality/baseline.json`
(sin la clave `voz_65s`, resto intacto). Sus WAV quedan en `tests/data/`
para `test_golden_65s` (estabilidad en archivos largos). Interino hasta
reemplazarlo por un clip largo representativo (ver §4).
