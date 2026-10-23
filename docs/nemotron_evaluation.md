# Evaluación y Comparativa: NVIDIA NemotronLabs RE-USE vs DPDFNet

Documento de investigación técnica y empírica en la rama `research/nemotronlabs-re-use`.

---

## 1. Qué se investigó

Se analizó el modelo recién publicado **NVIDIA NemotronLabs RE-USE** (`nvidia/NVIDIA-NemotronLabs-RE-USE`, 22 de octubre de 2026), un modelo de 9,6 millones de parámetros diseñado para *Universal Speech Enhancement* (USE).

### Características arquitectónicas del modelo NVIDIA
- **Arquitectura**: 30 bloques de Time-Frequency Mamba (SEMamba) bi-direccionales + Dense Convolutional Encoder/Decoder.
- **Alcance acústico**: Supresión de ruido aditivo, des-reverberación, corrección de clipping, extensión de ancho de banda (BWE) y remoción de artefactos de códec.
- **Dependencias oficiales**: Requiere Linux, PyTorch, GPU NVIDIA Ampere/Ada Lovelace y los kernels propietarios en CUDA/Triton del paquete `mamba_ssm`. En el script oficial (`inference.py:109`), CPU está deshabilitado explícitamente (`RuntimeError: Currently, CPU mode is not supported`).
- **Naturaleza temporal**: No-causal (bi-direccional en el tiempo, procesa clips completos o bloques de ~30 s con solapamiento).

### Comparación con el diseño de Denoise (DPDFNet)
- **DPDFNet**: Grafo único stateful recurrente (3,7M params, ~15 MB ONNX) que opera de forma causal frame a frame (40 ms de retraso determinista), diseñado para ejecutarse en CPU multiplataforma (Windows/macOS/Linux) sin dependencias externas de GPU ni Python.

---

## 2. Qué se probó

1. **Implementación desacoplada en Pure PyTorch CPU**:
   - Para evaluar el modelo sin depender de hardware GPU ni de los binarios CUDA de `mamba_ssm`, se construyó una implementación pura en PyTorch del bloque de escaneo selectivo Mamba S6 en [`tools/quality/compare_nemotron.py`](../tools/quality/compare_nemotron.py).
   - Se descargaron los pesos oficiales (`model.safetensors`, 38,6 MB) cargando los 1.416 tensores de parámetros con 0 claves faltantes y 0 inesperadas.

2. **Evaluación empírica sobre clip dorado**:
   - Clip de prueba: `tests/data/voz_noisy.wav` (3,00 s, 48 kHz, SNR 0 dB).
   - Referencia limpia: `tests/data/voz_clean.wav`.
   - Se midió velocidad (tiempo de pared y RTF en CPU) y métricas de calidad con el harness del proyecto (`tools/quality/metrics.py`).

### Resultados de la prueba

| Métrica | Entrada (Noisy) | DPDFNet (Producción) | NemotronLabs RE-USE | Diferencia |
|---|:---:|:---:|:---:|:---:|
| **Velocidad CPU (RTF)** | - | **~1,05 – 1,28x** | **46,43x** | DPDFNet ~36x más rápido |
| **Tiempo de cómputo CPU** | - | **~3,8 s** | **139,3 s** (~2m 20s) | DPDFNet en tiempo real |
| **SI-SDR final** | +0,06 dB | **+6,74 dB** | **+5,79 dB** | DPDFNet (+0,95 dB) |
| **Ganancia SI-SDR** | - | **+6,68 dB** | **+5,73 dB** | DPDFNet |
| **Peor segundo (p5)** | - | +3,79 dB | **+4,22 dB** | Nemotron (+0,43 dB) |
| **STOI (Inteligibilidad)** | - | 0,838 | **0,867** | Nemotron (+0,029) |
| **PESQ (Calidad de voz)** | - | 1,196 | **1,548** | **Nemotron (+0,352)** |
| **DNSMOS SIG (Voz)** | - | 3,32 | **3,50** | Nemotron (+0,18) |
| **DNSMOS BAK (Fondo)** | - | 3,97 | **4,05** | Nemotron (+0,08) |
| **DNSMOS OVR (Global)** | - | 3,04 | **3,23** | Nemotron (+0,19) |

---

## 3. Qué se concluyó

1. **Inviabilidad en CPU-only para la CLI de Denoise**:
   - Con un RTF de **46,43x**, procesar 1 minuto de video requiere más de **45 minutos de cómputo en CPU**. La evaluación de 120 pasadas Mamba secuenciales sin aceleración tensorial masiva en GPU es impracticable para una herramienta CLI de propósito general.
   - DPDFNet permanece como la arquitectura correcta para el núcleo del proyecto: es autónoma, corre en CPU a tiempo real (~1,1x RTF) y no requiere entorno Python ni GPU dedicada.

2. **Superioridad acústica perceptual de Nemotron**:
   - Nemotron supera a DPDFNet notablemente en preservación de la voz: **PESQ sube de 1,196 a 1,548** (+0,352) y **STOI sube de 0,838 a 0,867**. La voz conserva su timbre natural sin la coloración "metálica" propia de los modelos convolucionales/GRU pequeños.
   - La estabilidad temporal (`p5 = +4,22 dB`) y la atenuación del fondo (`BAK = 4,05`) demuestran que la arquitectura SEMamba es excelente para restauración de alta fidelidad.

3. **Recomendación futura**:
   - NemotronLabs RE-USE es un candidato idóneo si a futuro se diseña un backend opcional con aceleración GPU (vía CUDA / TensorRT / Triton) para usuarios con tarjetas gráficas dedicadas que busquen calidad de masterización de estudio.
