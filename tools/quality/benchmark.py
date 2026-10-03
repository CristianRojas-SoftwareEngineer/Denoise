"""Benchmark de calidad de audio para denoise (uso periodico + regresiones).

Dos modos:

1. `run`: ejecuta el DSP de produccion sobre el set de clips (via el harness
   `examples/process_wav.rs`, que invoca la misma ruta que usa `denoise`
   internamente) y luego puntua.
2. `score`: puntua un directorio con salidas ya generadas. Sirve para comparar
   dos builds entre si (p. ej. dos worktrees) sin re-ejecutar.

Las metricas viven en `metrics.py`. Aqui solo hay orquestacion, manifesto de
clips y puerta de regresion contra `baseline.json`.

Uso:
    python tools/quality/benchmark.py run --set tests_data --work out/bench
    python tools/quality/benchmark.py score --outputs out/bench --set tests_data
    python tools/quality/benchmark.py score --outputs out/bench --update-baseline
"""

from __future__ import annotations

import argparse
import json
import math
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
sys.path.insert(0, str(HERE))

import metrics  # noqa: E402

TOLERANCES = {
    "si_sdr_gain": 0.5,
    "si_sdr_p5": 1.0,
    "stoi_out": 0.02,
    "pesq_out": 0.05,
    "lufs_delta_band": 1.5,
    "rtf_ratio": 2.0,
}


# ------------------------------------------------------------------- entradas


def read_wav(path: Path) -> tuple["np.ndarray", int]:
    """Lee un WAV mono a f32 [-1, 1]. Prefiere soundfile, usa `wave` de respaldo."""
    import numpy as np

    try:
        import soundfile as sf

        data, sr = sf.read(str(path), dtype="float32", always_2d=True)
        return data[:, 0].astype(np.float64), int(sr)
    except ImportError:
        pass

    import wave

    with wave.open(str(path), "rb") as w:
        n_ch, sr = w.getnchannels(), w.getframerate()
        raw = np.frombuffer(w.readframes(w.getnframes()), dtype="<i2")
    if n_ch > 1:
        raw = raw.reshape(-1, n_ch).mean(axis=1)
    return raw.astype(np.float64) / 32768.0, int(sr)


def read_manifest() -> dict:
    with open(HERE / "clips.json", encoding="utf-8") as f:
        return json.load(f)


def clips_of(manifest: dict, name: str, root: Path) -> list[dict]:
    try:
        clips = manifest["sets"][name]["clips"]
    except KeyError:
        raise SystemExit(f"set '{name}' no existe en clips.json")
    out = []
    for c in clips:
        out.append(
            {
                "name": c["name"],
                "clean": root / c["clean"],
                "noisy": root / c["noisy"],
                "output_name": c.get("output_name", f'{c["name"]}_out.wav'),
            }
        )
        for key in ("clean", "noisy"):
            if not out[-1][key].exists():
                raise SystemExit(f"falta el archivo {out[-1][key]}")
    return out


# ------------------------------------------------------------------- puntuar


def fmt(v, nd=2):
    if v is None or (isinstance(v, float) and (math.isinf(v) or math.isnan(v))):
        return "n/a"
    return f"{v:.{nd}f}"


def score_all(
    clips: list[dict], outputs: Path, rtf_by_name: dict | None = None
) -> list[metrics.ClipScore]:
    scores = []
    for c in clips:
        clean, sr_c = read_wav(c["clean"])
        noisy, sr_n = read_wav(c["noisy"])
        out_path = outputs / c["output_name"]
        if not out_path.exists():
            raise SystemExit(f"falta la salida {out_path} (correr `run` primero)")
        out, sr_o = read_wav(out_path)
        if not (sr_c == sr_n == sr_o):
            raise SystemExit(
                f"{c['name']}: tasas distintas clean={sr_c} noisy={sr_n} out={sr_o}"
            )
        rtf = (rtf_by_name or {}).get(c["output_name"])
        scores.append(metrics.score_clip(c["name"], sr_c, clean, noisy, out, rtf=rtf))
    return scores


def parse_rtf(stdout: str) -> dict:
    """Extrae `RTF<TAB>salida<TAB>valor` del harness, claveado por nombre de fichero."""
    out = {}
    for line in stdout.splitlines():
        parts = line.split("\t")
        if len(parts) == 3 and parts[0] == "RTF":
            try:
                out[Path(parts[1]).name] = float(parts[2])
            except ValueError:
                pass
    return out


def print_table(scores: list[metrics.ClipScore]) -> None:
    head = (
        f"{'clip':<12} {'in->out SI-SDR':>17} {'ganancia':>9} {'p5':>7} "
        f"{'STOI':>13} {'PESQ':>13} {'nivel':>8} {'LUFS':>7} {'RTF':>6} {'voz%':>6}  avisos"
    )
    print(head)
    print("-" * len(head))
    for s in scores:
        si = f"{s.si_sdr_in:+.1f}->{s.si_sdr_out:+.1f}"
        st = f"{fmt(s.stoi_in,3)}->{fmt(s.stoi_out,3)}"
        pq = f"{fmt(s.pesq_in,3)}->{fmt(s.pesq_out,3)}"
        warn = "; ".join(s.warnings)
        print(
            f"{s.name:<12} {si:>17} {s.si_sdr_gain:>+8.2f} {fmt(s.si_sdr_p5,1):>7} "
            f"{st:>13} {pq:>13} {s.level_delta:>+7.1f} {fmt(s.lufs_delta,1):>7} "
            f"{fmt(s.rtf,2):>6} {s.active_fraction*100:>5.0f}%  {warn}"
        )
    print()
    print("SI-SDR: cuanto ruido se quito (ganancia = salida - entrada).")
    print("p5: peor segundo audible; si cae muy bajo hay daño localizado.")
    print("STOI/PESQ: cuanta voz quedo integra (penalizan la deformacion).")
    print("LUFS: cambio de sonoridad percibida. RTF: segundos de audio por")
    print("segundo de pared (<1 es mas rapido que tiempo real).")
    print("Si SI-SDR mejora pero STOI/PESQ no, el modelo sobre-suprime.")


# ----------------------------------------------------------------- regresion


def check_baseline(
    scores: list[metrics.ClipScore], baseline_path: Path
) -> list[str]:
    """Compara contra la linea base. Devuelve la lista de regresiones."""
    with open(baseline_path, encoding="utf-8") as f:
        base = json.load(f)["clips"]

    failures = []
    for s in scores:
        b = base.get(s.name)
        if b is None:
            failures.append(f"{s.name}: sin linea base (agregar con --update-baseline)")
            continue
        if s.si_sdr_gain < b["si_sdr_gain"] - TOLERANCES["si_sdr_gain"]:
            failures.append(
                f"{s.name}: SI-SDR +{s.si_sdr_gain:.2f} < base +{b['si_sdr_gain']:.2f}"
            )
        if s.si_sdr_p5 < b.get("si_sdr_p5", s.si_sdr_p5) - TOLERANCES["si_sdr_p5"]:
            failures.append(
                f"{s.name}: p5 {s.si_sdr_p5:.2f} < base {b.get('si_sdr_p5'):.2f}"
            )
        for key, tol in (("stoi_out", TOLERANCES["stoi_out"]), ("pesq_out", TOLERANCES["pesq_out"])):
            cur, prev = getattr(s, key), b.get(key)
            if cur is not None and prev is not None and cur < prev - tol:
                failures.append(f"{s.name}: {key} {cur:.3f} < base {prev:.3f}")
        cur_lu, prev_lu = s.lufs_delta, b.get("lufs_delta")
        if (
            cur_lu is not None
            and prev_lu is not None
            and abs(cur_lu - prev_lu) > TOLERANCES["lufs_delta_band"]
        ):
            failures.append(
                f"{s.name}: LUFS {cur_lu:+.2f} fuera de banda (base {prev_lu:+.2f})"
            )
        cur_rtf, prev_rtf = s.rtf, b.get("rtf")
        if (
            cur_rtf is not None
            and prev_rtf
            and cur_rtf > prev_rtf * TOLERANCES["rtf_ratio"]
        ):
            failures.append(
                f"{s.name}: RTF {cur_rtf:.2f} > 2x base {prev_rtf:.2f}"
            )
    return failures


def write_baseline(scores: list[metrics.ClipScore], path: Path) -> None:
    data = {"clips": {s.name: s.as_dict() for s in scores}}
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)
    print(f"linea base escrita en {path}")


# --------------------------------------------------------------------- modos


def cmd_run(args) -> int:
    clips = clips_of(read_manifest(), args.set, args.root)
    args.work.mkdir(parents=True, exist_ok=True)

    pairs = args.work / "pairs.txt"
    with open(pairs, "w", encoding="utf-8") as f:
        for c in clips:
            f.write(f"{c['noisy']}\t{args.work / c['output_name']}\n")

    cmd = [
        "cargo", "run", "--release", "--example", "process_wav", "--",
        str(pairs),
    ]
    if args.model_dir:
        cmd.append(str(args.model_dir))
    r = subprocess.run(cmd, cwd=str(args.root), stdout=subprocess.PIPE, text=True)
    if r.returncode != 0:
        return r.returncode

    return cmd_score(args, rtf_by_name=parse_rtf(r.stdout))


def cmd_score(args, rtf_by_name: dict | None = None) -> int:
    clips = clips_of(read_manifest(), args.set, args.root)
    scores = score_all(clips, Path(args.outputs), rtf_by_name)

    print_table(scores)

    report = {"clips": {s.name: s.as_dict() for s in scores}}
    report_path = Path(args.outputs) / "report.json"
    with open(report_path, "w", encoding="utf-8") as f:
        json.dump(report, f, indent=2, ensure_ascii=False)
    print(f"reporte en {report_path}")

    if args.update_baseline:
        write_baseline(scores, HERE / "baseline.json")
        return 0

    baseline = HERE / "baseline.json"
    if baseline.exists() and not args.no_check:
        failures = check_baseline(scores, baseline)
        if failures:
            print("REGRESIONES:")
            for f in failures:
                print(f"  - {f}")
            return 1
        print("sin regresiones respecto a la linea base")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="Benchmark de calidad de audio (denoise)")
    sub = ap.add_subparsers(dest="mode", required=True)

    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--set", default="tests_data")
    common.add_argument("--root", type=Path, default=REPO)

    r = sub.add_parser("run", parents=[common], help="ejecutar el DSP y puntuar")
    r.add_argument("--work", type=Path, required=True)
    r.add_argument("--model-dir", type=Path, default=None)
    r.add_argument("--outputs", type=Path, default=None)
    r.add_argument("--update-baseline", action="store_true")
    r.add_argument("--no-check", action="store_true")

    s = sub.add_parser("score", parents=[common], help="puntuar salidas existentes")
    s.add_argument("--outputs", type=Path, required=True)
    s.add_argument("--update-baseline", action="store_true")
    s.add_argument("--no-check", action="store_true")

    args = ap.parse_args()
    if args.mode == "run":
        if args.outputs is None:
            args.outputs = Path(args.work)
        return cmd_run(args)
    return cmd_score(args)


if __name__ == "__main__":
    sys.exit(main())