"""Metricas de calidad de audio para denoise.

Contrato: `docs/specifications.md §6`, `docs/design.md §6`.
Golangpe: `si_sdr` reproduce exactamente la convencion de
`tests/common/si_sdr.rs` para que los numeros del benchmark y los del test
de regresion Rust sean comparables (mismo zero-mean, mismo `eps = 1e-8`, mismo
tope de100.0 para paridad).

Las tres metricas cubren ejes distintos y none sustituye a las otras:

- `si_sdr`: cuanto ruido se eliminó. Invariante a escala, asi que NO ve
  diferencias de volumen ni de ganancia.
- `stoi`: integridad/intelligibilidad de la voz. Penaliza la deformacion.
- `pesq`: MOS-LQO (P.862.2). Penaliza la deformacion y el ruido residual.
  Solo admite 8000 o 16000 Hz, por lo que se evalua sobre la senal remuestreada.

Medir solo SI-SDR es el error que motivó esta herramienta: un denoiser que
sobre-suprime (quita ruido de mas y deforma la voz) mejora SI-SDR y empeora
STOI/PESQ. Las tres juntas son necesarias para detectar esa regresión.
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field

import numpy as np

# ---------------------------------------------------------------- SI-SDR


def si_sdr(ref: np.ndarray, est: np.ndarray, eps: float = 1e-8) -> float:
    """SI-SDR entre la señal limpia `ref` y la estimada `est`.

    Misma convención que `tests/common/si_sdr.rs`:
      `SI-SDR = 10 · log10( ||x · ŝ||² / ||x̂ - ŝ||² )` con `ŝ = (x·x̂ / ||x||²) · x`,
      tras restar la media a ambas señales.

    Returns:
        dB. `100.0` para paridad práctica, `-inf` si alguna señal es nula.
    """
    n = min(len(ref), len(est))
    x = np.asarray(ref[:n], dtype=np.float64)
    y = np.asarray(est[:n], dtype=np.float64)

    x = x - x.mean()
    y = y - y.mean()

    norm_x_sq = float(np.dot(x, x))
    norm_y_sq = float(np.dot(y, y))

    if norm_x_sq < eps or norm_y_sq < eps:
        return -math.inf

    numerator = float(np.dot(x, y)) ** 2 / norm_x_sq
    denominator = norm_y_sq - numerator

    if denominator <= eps:
        return 100.0
    if numerator <= eps:
        return -math.inf

    return 10.0 * math.log10(numerator / denominator)


# ------------------------------------------------------- actividad de voz


def speech_mask(
    x: np.ndarray, sr: int, frame_ms: float = 30.0, drop_db: float = 35.0
) -> np.ndarray:
    """Máscara de frames con voz, por energía relativa al pico.

    Los fixtures contienen mucha porción de silencio casi digital (el par de
    65 s es >80 % silencio). Medir sobre la señal completa deja que el silencio
    domine el resultado, así que las métricas de reducción de ruido se
    calculan además restringidas a voz.

    Returns:
        `bool` array de longitud `num_frames`.
    """
    frame = max(1, int(sr * frame_ms / 1000.0))
    num = len(x) // frame
    if num == 0:
        return np.zeros(0, dtype=bool)

    trimmed = x[: num * frame].reshape(num, frame)
    rms_db = 10.0 * np.log10(np.maximum(np.mean(trimmed**2, axis=1), 1e-20))
    peak_db = float(np.max(rms_db))
    return rms_db >= peak_db - drop_db


def si_sdr_speech_only(ref: np.ndarray, est: np.ndarray, sr: int) -> float:
    """SI-SDR restringido a los frames donde la señal limpia tiene voz."""
    frame = max(1, int(sr * 30.0 / 1000.0))
    n = min(len(ref), len(est)) // frame
    if n == 0:
        return -math.inf

    mask = speech_mask(ref, sr)
    if not mask.any():
        return -math.inf

    r = ref[: n * frame].reshape(n, frame)[mask].ravel()
    e = est[: n * frame].reshape(n, frame)[mask].ravel()
    return si_sdr(r, e)


# ------------------------------------------------------------- STOI / PESQ


def stoi_score(ref: np.ndarray, est: np.ndarray, sr: int) -> float | None:
    """STOI (0..1). Mide integridad de la voz;oce null si no está instalado."""
    try:
        from pystoi import stoi as _stoi
    except ImportError:
        return None
    try:
        n = min(len(ref), len(est))
        return float(_stoi(ref[:n], est[:n], sr, extended=False))
    except Exception:
        return None


def _resample(x: np.ndarray, sr: int, target: int) -> np.ndarray:
    try:
        import scipy.signal as _ss

        return _ss.resample_poly(x, target, sr)
    except ImportError:
        g = np.gcd(sr, target)
        idx = np.arange(0, len(x) * (target // g), target // g)
        return np.interp(idx, np.arange(len(x)) * (sr // g), x)


def pesq_score(ref: np.ndarray, est: np.ndarray, sr: int) -> float | None:
    """PESQ MOS-LQO (P.862.2, wideband). Devuelve `None` si no está instalado.

    PITFALL: la firma real es `pesq(fs, ref, deg, mode)` — la frecuencia va
    PRIMERO, aunque el docstring la liste en otro orden. Además solo acepta
    8000 o 16000 Hz, así que se remuestrea a 16 kHz.
    """
    try:
        from pesq import pesq as _pesq
    except ImportError:
        return None

    target = 16000
    if sr != target:
        ref = _resample(ref, sr, target)
        est = _resample(est, sr, target)
    n = min(len(ref), len(est))
    try:
        return float(_pesq(target, ref[:n], est[:n], "wb"))
    except Exception:
        return None


# --------------------------------------------------------------- niveles


def level_p999(x: np.ndarray) -> float:
    """Nivel en dBFS del percentil 99.5 de |amplitud| (robusto a picos aislados)."""
    p = float(np.percentile(np.abs(x), 99.5))
    return 20.0 * math.log10(max(p, 1e-12))


def lag_samples(ref: np.ndarray, est: np.ndarray) -> int:
    """Desfase de `est` respecto de `ref` en muestras, por correlación cruzada.

    Negativo = `est` va retrasado. Detecta desincronización A/V, que el usuario
    percibe aunque la calidad de voz sea buena.
    """
    n = min(len(ref), len(est))
    size = 1 << int(math.ceil(math.log2(2 * n)))
    a = np.asarray(est[:n], dtype=np.float64)
    b = np.asarray(ref[:n], dtype=np.float64)
    a = a - a.mean()
    b = b - b.mean()
    if not np.any(a) or not np.any(b):
        return 0
    cc = np.fft.irfft(np.fft.rfft(a, size) * np.conj(np.fft.rfft(b, size)), size)
    cc = np.concatenate((cc[-(n - 1):], cc[:n]))
    return int(np.argmax(cc) - (n - 1))


# ------------------------------------------------------------- reporte


@dataclass
class ClipScore:
    name: str
    sr: int
    seconds: float
    si_sdr_in: float
    si_sdr_out: float
    si_sdr_gain: float
    si_sdr_gain_speech: float
    stoi_in: float | None
    stoi_out: float | None
    pesq_in: float | None
    pesq_out: float | None
    level_in: float
    level_out: float
    level_delta: float
    lag: int
    active_fraction: float
    warnings: list[str] = field(default_factory=list)

    def as_dict(self) -> dict:
        return {
            k: (None if isinstance(v, float) and math.isinf(v) else v)
            for k, v in self.__dict__.items()
        }


def score_clip(name: str, sr: int, clean: np.ndarray, noisy: np.ndarray, out: np.ndarray) -> ClipScore:
    """Calcula todas las métricas para un triplete (limpio, entrada, salida)."""
    n = min(len(clean), len(noisy), len(out))
    clean, noisy, out = clean[:n], noisy[:n], out[:n]

    s_in, s_out = si_sdr(clean, noisy), si_sdr(clean, out)
    speech = si_sdr_speech_only(clean, out, sr)
    if math.isinf(speech):
        speech = s_out - s_in
    lvl_in, lvl_out = level_p999(noisy), level_p999(out)
    n_frames = len(clean) // max(1, int(sr * 0.030))
    frac = float(speech_mask(clean, sr).mean()) if n_frames else 0.0

    warnings: list[str] = []
    lag = lag_samples(clean, out)
    if abs(lag) > int(sr * 0.020):
        warnings.append(f"desfase de {lag} muestras ({lag / sr * 1000:+.1f} ms)")
    if abs(lvl_out - lvl_in) > 6.0:
        warnings.append(f"nivel {lvl_out - lvl_in:+.1f} dB respecto a la entrada")
    if frac < 0.35:
        warnings.append(f"solo {frac * 100:.0f} % de voz activa: métrica poco representativa")
    if s_out < s_in:
        warnings.append("la salida empeora el SI-SDR global")

    return ClipScore(
        name=name,
        sr=sr,
        seconds=round(n / sr, 2),
        si_sdr_in=round(s_in, 2),
        si_sdr_out=round(s_out, 2),
        si_sdr_gain=round(s_out - s_in, 2),
        si_sdr_gain_speech=round(speech, 2),
        stoi_in=stoi_score(clean, noisy, sr),
        stoi_out=stoi_score(clean, out, sr),
        pesq_in=pesq_score(clean, noisy, sr),
        pesq_out=pesq_score(clean, out, sr),
        level_in=round(lvl_in, 2),
        level_out=round(lvl_out, 2),
        level_delta=round(lvl_out - lvl_in, 2),
        lag=lag,
        active_fraction=round(frac, 3),
        warnings=warnings,
    )