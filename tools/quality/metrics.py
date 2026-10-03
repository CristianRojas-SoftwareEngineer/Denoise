"""Metricas de calidad de audio para denoise.

Contrato: `docs/specifications.md §6`, `docs/design.md §6`.
Golpe: `si_sdr` reproduce exactamente la convencion de
`tests/common/si_sdr.rs` para que los numeros del benchmark y los del test
de regresion Rust sean comparables (mismo zero-mean, mismo `eps = 1e-8`, mismo
tope de100.0 para paridad).

Las cuatro metricas principales cubren ejes distintos y ninguna sustituye a las otras:

- `si_sdr`: cuanto ruido se eliminó. Invariante a escala, asi que NO ve
  diferencias de volumen ni de ganancia.
- `stoi`: integridad/intelligibilidad de la voz. Penaliza la deformacion.
- `pesq`: MOS-LQO (P.862.2). Penaliza la deformacion y el ruido residual.
  Solo admite 8000 o 16000 Hz, por lo que se evalua sobre la senal remuestreada.
- `dnsmos` (SIG/BAK/OVR): nota de humano sin referencia sobre voz, fondo y global.

Columnas de diagnóstico: `si_sdr_p5` (peor segundo audible), nivel, LUFS y RTF.

Medir solo SI-SDR es el error que motivó esta herramienta: un denoiser que
sobre-suprime (quita ruido de mas y deforma la voz) mejora SI-SDR y empeora
STOI/PESQ/DNSMOS. Las cuatro juntas son necesarias para detectar esa regresión.
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


# ------------------------------------------- SI-SDR por ventanas (peor caso)


def si_sdr_windowed(
    ref: np.ndarray,
    est: np.ndarray,
    sr: int,
    win_s: float = 1.0,
    eps: float = 1e-8,
    floor_drop_db: float = 50.0,
) -> np.ndarray:
    """Un SI-SDR por ventana de `win_s` segundos.

    Las medias globales esconden daño localizado (2 s rotos en 60 s buenos
    apenas mueven el promedio). Se excluyen (`nan`) dos clases de ventana:
    referencia casi nula (`eps`) y referencia más de `floor_drop_db` por
    debajo del pico del clip — contenido 50 dB bajo el pico es inaudible en
    cualquier condición real, y puntuarlo contaminaría el percentil con el
    fixture en vez de con el daño. Misma convención que `si_sdr` en el resto
    (100.0 paridad, -inf daño total).
    """
    win = max(1, int(sr * win_s))
    n = min(len(ref), len(est)) // win
    vals = np.full(n, np.nan)
    if n == 0:
        return vals

    r = np.asarray(ref[: n * win], dtype=np.float64).reshape(n, win)
    e = np.asarray(est[: n * win], dtype=np.float64).reshape(n, win)
    r = r - r.mean(axis=1, keepdims=True)
    e = e - e.mean(axis=1, keepdims=True)

    nr = np.einsum("ij,ij->i", r, r)
    ne = np.einsum("ij,ij->i", e, e)
    rms_db = 10.0 * np.log10(np.maximum(nr / win, 1e-20))
    valid = (nr >= eps) & (ne >= eps) & (rms_db >= rms_db.max() - floor_drop_db)
    if not valid.any():
        return vals

    d = np.einsum("ij,ij->i", r[valid], e[valid])
    num = d**2 / nr[valid]
    den = ne[valid] - num

    v = np.full(int(valid.sum()), np.nan)
    v[den <= eps] = 100.0
    good = (den > eps) & (num > eps)
    v[good] = 10.0 * np.log10(num[good] / den[good])
    v[(den > eps) & (num <= eps)] = -np.inf
    vals[valid] = v
    return vals


def si_sdr_p5(ref: np.ndarray, est: np.ndarray, sr: int, win_s: float = 1.0) -> float:
    """Percentil 5 del SI-SDR por ventanas: cómo suena el peor tramo."""
    vals = si_sdr_windowed(ref, est, sr, win_s)
    vals = vals[~np.isnan(vals)]
    if len(vals) == 0:
        return -math.inf
    return float(np.percentile(vals, 5))


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


# ------------------------------------------------- DNSMOS (sin referencia)


def dnsmos_scores(x: np.ndarray, sr: int) -> dict | None:
    """DNSMOS P.808 + P.835 sin referencia: `{p808, sig, bak, ovr}`.

    La única métrica que no necesita la voz limpia: permite puntuar videos
    reales sin grabación de referencia. `sig` = calidad de la voz, `bak` =
    supresión del fondo, `ovr` = global. Vía torchmetrics (descarga el modelo
    oficial de Microsoft a `~/.torchmetrics/DNSMOS` en el primer uso).
    Devuelve `None` si no está instalado o falla la descarga.
    """
    try:
        import torch
        from torchmetrics.functional.audio.dnsmos import (
            deep_noise_suppression_mean_opinion_score as _dns,
        )
    except ImportError:
        return None
    try:
        t = torch.from_numpy(np.asarray(x, dtype=np.float32))
        v = _dns(t, int(sr), False)
        return {k: float(v[i]) for i, k in enumerate(("p808", "sig", "bak", "ovr"))}
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


# -------------------------------------------------- sonoridad (LUFS)


# Coeficientes del K-weighting BS.1770-4 a 48 kHz (tablas de la norma).
# Etapa 1: pre-filtro high-shelf. Etapa 2: pasa-altos RLB.
_K_PRE_B = (1.53512485958697, -2.69169618940638, 1.19839281085285)
_K_PRE_A = (1.0, -1.69065929318241, 0.73248077421585)
_K_RLB_B = (1.0, -2.0, 1.0)
_K_RLB_A = (1.0, -1.99004745483398, 0.99007225036621)


def lufs_integrated(x: np.ndarray, sr: int) -> float | None:
    """Sonoridad integrada en LUFS (ITU-R BS.1770, mono).

    El pico (dBFS) no dice cómo de fuerte se percibe; LUFS sí: pondera por
    frecuencia (K-weighting) y promedia solo los bloques con contenido
    (gating absoluto −70 LUFS y relativo −10 LU). Devuelve `None` sin scipy.
    """
    try:
        from scipy.signal import lfilter as _lf
    except ImportError:
        return None

    x = np.asarray(x, dtype=np.float64)
    if sr != 48000:
        x = _resample(x, sr, 48000)
        sr = 48000
    if len(x) == 0 or not np.any(x):
        return -math.inf

    y = _lf(_K_PRE_B, _K_PRE_A, x)
    y = _lf(_K_RLB_B, _K_RLB_A, y)

    blk = int(sr * 0.400)
    hop = blk // 4
    if len(y) < blk:
        ms = np.array([float(np.mean(y**2))])
    else:
        n = 1 + (len(y) - blk) // hop
        ms = np.array([float(np.mean(y[i * hop : i * hop + blk] ** 2)) for i in range(n)])

    def to_lufs(m: float) -> float:
        return -0.691 + 10.0 * math.log10(max(m, 1e-20))

    keep = np.array([to_lufs(m) >= -70.0 for m in ms])
    if not keep.any():
        return -math.inf
    rel = to_lufs(float(ms[keep].mean())) - 10.0
    gated = ms[[k and to_lufs(m) >= rel for k, m in zip(keep, ms)]]
    if len(gated) == 0:
        return -math.inf
    return to_lufs(float(gated.mean()))


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
    si_sdr_p5: float
    stoi_in: float | None
    stoi_out: float | None
    pesq_in: float | None
    pesq_out: float | None
    dnsmos_sig_in: float | None
    dnsmos_sig_out: float | None
    dnsmos_bak_in: float | None
    dnsmos_bak_out: float | None
    dnsmos_ovr_in: float | None
    dnsmos_ovr_out: float | None
    level_in: float
    level_out: float
    level_delta: float
    lufs_in: float | None
    lufs_out: float | None
    lufs_delta: float | None
    rtf: float | None
    lag: int
    active_fraction: float
    warnings: list[str] = field(default_factory=list)

    def as_dict(self) -> dict:
        return {
            k: (None if isinstance(v, float) and math.isinf(v) else v)
            for k, v in self.__dict__.items()
        }


def score_clip(
    name: str,
    sr: int,
    clean: np.ndarray,
    noisy: np.ndarray,
    out: np.ndarray,
    rtf: float | None = None,
) -> ClipScore:
    """Calcula todas las métricas para un triplete (limpio, entrada, salida)."""
    n = min(len(clean), len(noisy), len(out))
    clean, noisy, out = clean[:n], noisy[:n], out[:n]

    s_in, s_out = si_sdr(clean, noisy), si_sdr(clean, out)
    speech = si_sdr_speech_only(clean, out, sr)
    if math.isinf(speech):
        speech = s_out - s_in
    p5 = si_sdr_p5(clean, out, sr)
    lvl_in, lvl_out = level_p999(noisy), level_p999(out)
    li, lo = lufs_integrated(noisy, sr), lufs_integrated(out, sr)
    lufs_delta = (lo - li) if li is not None and lo is not None else None
    d_in = dnsmos_scores(noisy, sr) or {}
    d_out = dnsmos_scores(out, sr) or {}
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
    if not math.isinf(p5) and p5 < s_out - 3.0:
        warnings.append(f"daño localizado: peor segundo {p5:.1f} dB vs media {s_out:.1f} dB")

    return ClipScore(
        name=name,
        sr=sr,
        seconds=round(n / sr, 2),
        si_sdr_in=round(s_in, 2),
        si_sdr_out=round(s_out, 2),
        si_sdr_gain=round(s_out - s_in, 2),
        si_sdr_gain_speech=round(speech, 2),
        si_sdr_p5=round(p5, 2) if not math.isinf(p5) else p5,
        stoi_in=stoi_score(clean, noisy, sr),
        stoi_out=stoi_score(clean, out, sr),
        pesq_in=pesq_score(clean, noisy, sr),
        pesq_out=pesq_score(clean, out, sr),
        dnsmos_sig_in=d_in.get("sig"),
        dnsmos_sig_out=d_out.get("sig"),
        dnsmos_bak_in=d_in.get("bak"),
        dnsmos_bak_out=d_out.get("bak"),
        dnsmos_ovr_in=d_in.get("ovr"),
        dnsmos_ovr_out=d_out.get("ovr"),
        level_in=round(lvl_in, 2),
        level_out=round(lvl_out, 2),
        level_delta=round(lvl_out - lvl_in, 2),
        lufs_in=round(li, 2) if li is not None else None,
        lufs_out=round(lo, 2) if lo is not None else None,
        lufs_delta=round(lufs_delta, 2) if lufs_delta is not None else None,
        rtf=rtf,
        lag=lag,
        active_fraction=round(frac, 3),
        warnings=warnings,
    )