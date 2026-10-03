# Métricas de calidad de audio

> Este documento explica, sin jerga de sonidista, las métricas con las que se
> evalúa la calidad del audio resultante del proceso de aislación y mejora de
> la voz. Es autocontenido: no requiere saber nada previo sobre audio.
> Convención de rutas: `docs/<fichero>` es relativo a la raíz del repo.

## Índice

1. [Qué hace la herramienta](#1-qué-hace-la-herramienta)
2. [Las métricas principales](#2-las-métricas-principales)
   - [2.1. SI-SDR: ¿cuánto ruido se fue?](#21-si-sdr-cuánto-ruido-se-fue)
   - [2.2. STOI: ¿se entiende lo que dice?](#22-stoi-se-entiende-lo-que-dice)
   - [2.3. PESQ: ¿suena natural o suena a robot?](#23-pesq-suena-natural-o-suena-a-robot)
   - [2.4. DNSMOS: la nota de un humano](#24-dnsmos-la-nota-de-un-humano)
3. [Cómo leerlas juntas](#3-cómo-leerlas-juntas)
4. [Las columnas extra](#4-las-columnas-extra)
5. [Lo que las métricas no dicen](#5-lo-que-las-métricas-no-dicen)
6. [Dónde se calculan](#6-dónde-se-calculan)

## 1. Qué hace la herramienta

Recibe un video con voz + ruido de fondo (metro, calle, ventilador) y devuelve
el video con la voz más limpia. La pregunta que nos hacemos es doble: **¿cuánto
más limpia quedó, y a qué costo?** Ninguna métrica sola responde ambas cosas,
por eso usamos varias.

## 2. Las métricas principales

### 2.1. SI-SDR: ¿cuánto ruido se fue?

Compara la energía de la voz contra la energía de lo que no es voz. Se mide en
decibeles (dB), donde cada ~3 dB es el doble o la mitad de energía:

- **0 dB** = la voz y el ruido suenan igual de fuerte. No se entiende nada.
- **10 dB** = la voz suena unas 3 veces más fuerte que el ruido. Como una
  conversación normal en un bar: se entiende, con ruido de fondo.
- **20 dB** = la voz domina totalmente. Como un estudio de grabación.

Lo que importa no es el valor absoluto sino la **ganancia**: cuántos dB se
ganaron entre la entrada y la salida. Pasar de 0 dB a 7 dB es pasar de "no se
entiende nada" a "se entiende con algo de ruido de fondo". Es una mejora real
y grande.

Su punto ciego: es invariante a escala, o sea que **no ve diferencias de
volumen**, y no penaliza que la voz salga deformada. Por eso no basta sola.

### 2.2. STOI: ¿se entiende lo que dice?

Un número entre 0 y 1. Mide si las palabras son reconocibles, no si suenan
bonito:

- **0,6** = se entiende a medias, hay que adivinar palabras.
- **0,84** = se entiende bien, conversación fluida.
- **1,0** = perfecto.

Es lo que al usuario final más le importa: da igual cuánto ruido se haya
quitado si después no se entiende lo que dicen.

### 2.3. PESQ: ¿suena natural o suena a robot?

Un puntaje de 1 a 4,5 que imita cómo calificaría un oído humano el sonido
(es la norma ITU-T P.862, modo banda ancha):

- **1** = suena a robot roto, voz de ultratumba.
- **2** = aceptable para una llamada, pero se nota procesado.
- **3 o más** = suena natural.

Es la métrica más exigente y la que detecta el defecto típico de los
reductores de ruido: quitar el ruido pero dejar la voz con timbre
metálico o aguado. Técnicamente se evalúa remuestreando a 16 kHz, porque la
norma solo admite 8 o 16 kHz; eso no cambia lo que mide.

### 2.4. DNSMOS: la nota de un humano

Un modelo entrenado con miles de calificaciones de personas reales, que
predice qué nota le pondrían a tu audio. Da tres notas de 1 a 5, y es la
única métrica que **no necesita la grabación limpia**: sirve para puntuar
cualquier video real.

- **SIG** = calidad de la voz (¿suena bien quien habla?).
- **BAK** = calidad del fondo (¿se fue el ruido? mientras más alto, más silencio).
- **OVR** = nota global (lo que un humano diría en general).

Su gracia frente a las otras: separa el veredicto en dos. Si BAK es alto pero
SIG es bajo, el ruido se fue pero la voz quedó dañada — el modelo fue
demasiado agresivo. Ninguna métrica con referencia te dice eso tan directo.

## 3. Cómo leerlas juntas

| Lo que ves | Lo que significa |
|---|---|
| SI-SDR sube **y** STOI/PESQ suben | Todo bien: menos ruido, voz intacta |
| SI-SDR sube pero STOI/PESQ **no se mueven** | El modelo está borrando de más: quita ruido pero deforma la voz |
| PESQ bajo aunque SI-SDR sea alto | Suena a robot. Hay que suavizar el procesado, no quitar más ruido |
| BAK alto pero SIG bajo | El ruido se fue pero la voz quedó dañada: el modelo es agresivo |

La segunda fila es la razón de ser de este documento: durante un tiempo solo
se medía SI-SDR, y un modelo que sobre-suprime mejora SI-SDR mientras empeora
la voz. Sin STOI y PESQ esa regresión es invisible.

## 4. Las columnas extra

Además de las métricas principales, el reporte incluye:

- **Peor segundo (p5)**: el SI-SDR del peor tramo de 1 segundo con voz
  audible. La media puede estar bien y esconder 2 segundos rotos; el p5 los
  delata. Si cae más de 3 dB bajo la media, la herramienta lo marca como aviso.
- **Nivel (dB)**: cuánto cambió el volumen de salida respecto a la entrada.
  ±2 dB es apenas perceptible; más de ±6 dB la herramienta lo marca como aviso.
- **LUFS**: lo mismo que el nivel pero en la unidad que percibe el oído
  (la que usan la televisión y la radio), no en picos. Si la salida conserva
  el volumen percibido, este número apenas se mueve.
- **RTF**: segundos de audio procesados por segundo de pared. Menor que 1 es
  más rápido que tiempo real. Mide cuánto espera el usuario, no la calidad.
- **Retardo (muestras)**: desfase de la salida respecto a la voz real. Si no
  es cero, el audio queda desincronizado con el video, algo que se percibe
  aunque la voz suene bien.
- **Voz %**: qué fracción de la grabación tiene voz real frente a silencio.
  Si es muy baja (menos de ~35 %), la métrica de ese clip no es confiable: es
  como evaluar un examen donde la mayoría de las preguntas están en blanco.

## 5. Lo que las métricas no dicen

Ninguna de ellas reemplaza escuchar el resultado. Miden ruido, inteligibilidad
y naturalidad sobre grabaciones con referencia limpia conocida (salvo DNSMOS,
que no la necesita); no detectan artefactos raros (ecos, cortes, voz que
aparece y desaparece) tan bien como un oído. Ante una duda entre el número y
el oído, manda el oído.

## 6. Dónde se calculan

- `tools/quality/metrics.py`: implementación de todas las métricas.
- `tools/quality/benchmark.py`: ejecuta el DSP sobre un set de clips y genera
  el reporte con la tabla.
- `tools/quality/baseline.json`: línea base comprometida; el benchmark falla
  si alguna métrica cae respecto a ella.
- `tests/common/si_sdr.rs`: SI-SDR en Rust para `test_golden`. Usa la misma
  convención que `metrics.py`, así ambos números son comparables.
- `docs/benchmark.md`: registro histórico de resultados a medida que se
  refina la implementación.
