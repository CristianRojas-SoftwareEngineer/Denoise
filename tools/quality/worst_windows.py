"""Peores segundos de cada clip para escucha (aceptación por peor caso).

Por clip del set: calcula la ganancia SI-SDR por ventanas de 1 s
(salida vs entrada, con `metrics.si_sdr_windowed`) y exporta las N
peores ventanas como tríos clean/noisy/out en WAV para escucharlas.

Uso:
    python tools/quality/worst_windows.py --set tests_data --work out/bench
    python tools/quality/worst_windows.py --set tests_data --work out/bench --top-k 5
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np
import soundfile as sf

import metrics

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
SAMPLE_RATE = 48000


def find_worst_windows(
    ref: np.ndarray,
    est: np.ndarray,
    mix: np.ndarray,
    top_k: int = 3,
) -> list[tuple[int, float]]:
    """Índices de las top_k ventanas con peor ganancia SI-SDR (salida - entrada)."""
    gain_out = metrics.si_sdr_windowed(ref, est, SAMPLE_RATE)
    gain_in = metrics.si_sdr_windowed(ref, mix, SAMPLE_RATE)
    gain = gain_out - gain_in
    is_finite = np.isfinite(gain)
    sorted_indices = np.argsort(np.where(is_finite, gain, np.inf))[:top_k]
    return [(int(idx), float(gain[idx])) for idx in sorted_indices if is_finite[idx]]


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Exporta los peores segundos por clip según ganancia SI-SDR."
    )
    parser.add_argument("--set", default="tests_data", help="Nombre del set en clips.json")
    parser.add_argument("--work", default="out/bench", help="Directorio con salidas del benchmark")
    parser.add_argument("--out", default="out/worst_windows", help="Directorio de destino para WAVs")
    parser.add_argument("-n", "--top-k", type=int, default=3, help="Cantidad de peores ventanas a extraer")
    args = parser.parse_args()

    clips = json.load(open(HERE / "clips.json", encoding="utf-8"))["sets"][args.set]["clips"]
    work_dir = REPO / args.work
    dest_dir = REPO / args.out
    dest_dir.mkdir(parents=True, exist_ok=True)
    manifest: dict[str, list[dict[str, float | int]]] = {}

    for clip in clips:
        ref, _ = sf.read(REPO / clip["clean"], dtype="float64")
        mix, _ = sf.read(REPO / clip["noisy"], dtype="float64")
        est, _ = sf.read(work_dir / clip["output_name"], dtype="float64")
        num_seconds = min(len(ref), len(mix), len(est)) // SAMPLE_RATE
        ref = ref[: num_seconds * SAMPLE_RATE]
        mix = mix[: num_seconds * SAMPLE_RATE]
        est = est[: num_seconds * SAMPLE_RATE]

        worst_windows = find_worst_windows(ref, est, mix, args.top_k)
        manifest[clip["name"]] = [
            {"second": sec, "gain_db": round(gain, 1)} for sec, gain in worst_windows
        ]
        for sec, gain in worst_windows:
            window_slice = slice(sec * SAMPLE_RATE, (sec + 1) * SAMPLE_RATE)
            base_name = f"{clip['name']}_s{sec:02d}"
            sf.write(dest_dir / f"{base_name}_clean.wav", ref[window_slice], SAMPLE_RATE, subtype="PCM_16")
            sf.write(dest_dir / f"{base_name}_noisy.wav", mix[window_slice], SAMPLE_RATE, subtype="PCM_16")
            sf.write(dest_dir / f"{base_name}_out.wav", est[window_slice], SAMPLE_RATE, subtype="PCM_16")
        formatted = ", ".join(f"s{sec} ({gain:+.1f} dB)" for sec, gain in worst_windows)
        print(f"{clip['name']}: {formatted}")

    with open(dest_dir / "worst_windows.json", "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=1)
    print(f"Tríos WAV exportados en {dest_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
