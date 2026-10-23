"""Benchmark de calidad de audio para denoise (uso periodico + regresiones).

Dos modos:

1. `run`: ejecuta el DSP de produccion sobre el set de clips (via el harness
   `examples/process_wav.rs`, que invoca la misma ruta que usa `denoise`
   internamente) y luego puntua.
2. `score`: puntua un directorio con salidas ya generadas. Sirve para comparar
   dos builds entre si (p. ej. dos worktrees) sin re-ejecutar.

Las metricas viven en `metrics.py`. Aqui solo hay orquestacion, manifesto de
clips y puerta de regresion contra `baseline_{perfil}.json`.

Perfiles: cada perfil define un conjunto de flags DSP (highpass, dry_blend,
pause_gate_db) declarados en `clips.json["profiles"]`. Su linea base vive en
`baseline_{nombre}.json`.

Uso:
    python tools/quality/benchmark.py run --set tests_data --work out/bench
    python tools/quality/benchmark.py run --set tests_data --work out/bench --profile hq_speech
    python tools/quality/benchmark.py run --set tests_data --work out/bench --profile all
    python tools/quality/benchmark.py score --outputs out/bench --set tests_data
    python tools/quality/benchmark.py run --set tests_data --work out/bench --update-baseline
Nota: ratificar siempre con `run` (`score` no mide RTF y dejaria `rtf: null`
en la baseline).
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
import numpy as np  # noqa: E402

TOLERANCES = {
    "si_sdr_gain": 0.5,
    "si_sdr_p5": 1.0,
    "stoi_out": 0.02,
    "pesq_out": 0.05,
    "lufs_delta_band": 1.5,
    "rtf_abs_threshold": 5.0,
    "dnsmos_ovr_out": 0.1,
    "dnsmos_sig_out": 0.15,
    "dnsmos_bak_out": 0.15,
}

# Métricas que hacen fallar la puerta (caída bajo tolerancia).
# Las tolerancias DNSMOS vienen del protocolo de varianza de Fase 2
# (desvío 0.000000 en 5 corridas): cubren diferencias de entorno, no ruido.
GATED_LOWER = (
    "si_sdr_gain",
    "si_sdr_p5",
    "stoi_out",
    "pesq_out",
    "dnsmos_sig_out",
    "dnsmos_bak_out",
    "dnsmos_ovr_out",
)

# Métricas que se reportan pero no fallan (reservado para futuras candidatas).
INFORMATIVAS: tuple = ()

# RTF: gate absoluto, no relativo. El RTF es hardware-dependent (RNF-03) y no
# mide calidad DSP. Un umbral de 5.0 (5x tiempo real) captura regresiones reales
# del pipeline sin falsos positivos por hardware más lento. Ver specs §RNF-03.


# ------------------------------------------------------------------- perfiles


def read_profiles(manifest: dict) -> dict:
    """Carga los perfiles de flags DSP del manifiesto."""
    return manifest["profiles"]


def baseline_path_for(profile_name: str) -> Path:
    """Ruta canónica de la línea base de un perfil: baseline_{nombre}.json."""
    return HERE / f"baseline_{profile_name}.json"


def build_harness_cmd(
    pairs: Path, model_dir: Path | None, profile_flags: dict
) -> list[str]:
    """Construye el comando del harness Rust inyectando las flags del perfil."""
    cmd = [
        "cargo", "run", "--release", "--example", "process_wav", "--",
        str(pairs),
    ]
    if model_dir:
        cmd.append(str(model_dir))
    if profile_flags.get("highpass", False):
        cmd.append("--highpass")
    if "dry_blend" in profile_flags:
        cmd.extend(["--dry-blend", str(profile_flags["dry_blend"])])
    if "pause_gate_db" in profile_flags:
        cmd.extend(["--pause-gate-db", str(profile_flags["pause_gate_db"])])
    return cmd


# ------------------------------------------------------------------- entradas


def read_wav(path: Path) -> tuple[np.ndarray, int]:
    """Lee un WAV mono a f32 [-1, 1]. Prefiere soundfile, usa `wave` de respaldo."""
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


def print_table(scores: list[metrics.ClipScore], profile_name: str) -> None:
    print(f"\n{'=' * 20} perfil: {profile_name} {'=' * 20}")
    head = (
        f"{'clip':<12} {'in->out SI-SDR':>17} {'ganancia':>9} {'p5':>7} "
        f"{'STOI':>13} {'PESQ':>13} {'SIG/BAK/OVR':>17} "
        f"{'nivel':>8} {'LUFS':>7} {'RTF':>6} {'voz%':>6}  avisos"
    )
    print(head)
    print("-" * len(head))
    for s in scores:
        si = f"{s.si_sdr_in:+.1f}->{s.si_sdr_out:+.1f}"
        st = f"{fmt(s.stoi_in,3)}->{fmt(s.stoi_out,3)}"
        pq = f"{fmt(s.pesq_in,3)}->{fmt(s.pesq_out,3)}"
        dn = f"{fmt(s.dnsmos_sig_out,2)}/{fmt(s.dnsmos_bak_out,2)}/{fmt(s.dnsmos_ovr_out,2)}"
        warn = "; ".join(s.warnings)
        print(
            f"{s.name:<12} {si:>17} {s.si_sdr_gain:>+8.2f} {fmt(s.si_sdr_p5,1):>7} "
            f"{st:>13} {pq:>13} {dn:>17} "
            f"{s.level_delta:>+7.1f} {fmt(s.lufs_delta,1):>7} "
            f"{fmt(s.rtf,2):>6} {s.active_fraction*100:>5.0f}%  {warn}"
        )
    print()
    print("SI-SDR: cuanto ruido se quito (ganancia = salida - entrada).")
    print("p5: peor segundo audible; si cae muy bajo hay daño localizado.")
    print("STOI/PESQ: cuanta voz quedo integra (penalizan la deformacion).")
    print("SIG/BAK/OVR (DNSMOS, sin referencia): voz / fondo / global.")
    print("LUFS: cambio de sonoridad percibida. RTF: segundos de audio por")
    print("segundo de pared (<1 es mas rapido que tiempo real).")
    print("Si SI-SDR mejora pero STOI/PESQ no, el modelo sobre-suprime.")


# ----------------------------------------------------------------- regresion


def check_non_finite_metric(name: str, key: str, cur, prev) -> list[str]:
    """NaN o ±inf en una métrica de puerta es fallo explícito, nunca verde.

    `None` (métrica no calculada) no es no-finita: no genera fallo.
    """
    failures = []
    for suffix, val in (("", cur), (" en la baseline", prev)):
        if val is not None and not math.isfinite(val):
            failures.append(f"{name}: {key} no finita{suffix} ({val})")
    return failures


def check_baseline(
    scores: list[metrics.ClipScore], baseline_path: Path
) -> list[str]:
    """Compara contra la línea base. Devuelve la lista de regresiones."""
    with open(baseline_path, encoding="utf-8") as f:
        base = json.load(f)["clips"]

    failures = []
    for s in scores:
        b = base.get(s.name)
        if b is None:
            failures.append(f"{s.name}: sin línea base (agregar con --update-baseline)")
            continue
        for key in GATED_LOWER:
            cur, prev = getattr(s, key), b.get(key)
            non_finite_errors = check_non_finite_metric(s.name, key, cur, prev)
            if non_finite_errors:
                failures.extend(non_finite_errors)
                continue
            if cur is None or prev is None:
                continue
            if cur < prev - TOLERANCES[key]:
                failures.append(f"{s.name}: {key} {cur:.3f} < base {prev:.3f}")
        cur_lu, prev_lu = s.lufs_delta, b.get("lufs_delta")
        non_finite_errors = check_non_finite_metric(s.name, "lufs_delta", cur_lu, prev_lu)
        if non_finite_errors:
            failures.extend(non_finite_errors)
        elif (
            cur_lu is not None
            and prev_lu is not None
            and abs(cur_lu - prev_lu) > TOLERANCES["lufs_delta_band"]
        ):
            failures.append(
                f"{s.name}: LUFS {cur_lu:+.2f} fuera de banda (base {prev_lu:+.2f})"
            )
        cur_rtf, prev_rtf = s.rtf, b.get("rtf")
        non_finite_errors = check_non_finite_metric(s.name, "rtf", cur_rtf, prev_rtf)
        if non_finite_errors:
            failures.extend(non_finite_errors)
        elif cur_rtf is not None and cur_rtf > TOLERANCES["rtf_abs_threshold"]:
            # RTF (tiempo de ejecución vs tiempo real) no mide calidad DSP: es
            # dependiente del hardware. Un umbral absoluto captura regresiones
            # reales del pipeline (5x más lento) sin disparar falsos positivos
            # por cambios de hardware. Ver RNF-03: objetivo <=1.0, sin gate automatizado.
            failures.append(
                f"{s.name}: RTF {cur_rtf:.2f} > umbral absoluto {TOLERANCES['rtf_abs_threshold']:.1f}"
            )
    return failures


def write_baseline(scores: list[metrics.ClipScore], path: Path) -> None:
    data = {"clips": {s.name: s.as_dict() for s in scores}}
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)
    print(f"linea base escrita en {path}")


# -------------------------------------------------------- resolución perfiles


def resolve_profiles(profile_arg: str, manifest: dict) -> list[str]:
    """Resuelve --profile a la lista de nombres de perfiles a ejecutar."""
    all_profiles = read_profiles(manifest)
    if profile_arg == "all":
        return list(all_profiles.keys())
    if profile_arg not in all_profiles:
        raise SystemExit(
            f"perfil '{profile_arg}' no existe en clips.json; "
            f"disponibles: {', '.join(all_profiles.keys())}"
        )
    return [profile_arg]


def work_dir_for(base_work: Path, profile_name: str, multi: bool) -> Path:
    """Directorio de trabajo: subdirectorio por perfil si hay varios."""
    if multi:
        return base_work / profile_name
    return base_work


# --------------------------------------------------------------------- modos


def run_single_profile(
    profile_name: str,
    profile_flags: dict,
    clips: list[dict],
    work: Path,
    root: Path,
    model_dir: Path | None,
    update_baseline: bool,
    no_check: bool,
) -> int:
    """Ejecuta un perfil: harness → puntuar → puerta de regresión."""
    work.mkdir(parents=True, exist_ok=True)

    pairs = work / "pairs.txt"
    with open(pairs, "w", encoding="utf-8") as f:
        for c in clips:
            f.write(f"{c['noisy']}\t{work / c['output_name']}\n")

    cmd = build_harness_cmd(pairs, model_dir, profile_flags)
    r = subprocess.run(cmd, cwd=str(root), stdout=subprocess.PIPE, text=True)
    if r.returncode != 0:
        return r.returncode

    return score_single_profile(
        profile_name, clips, work, parse_rtf(r.stdout),
        update_baseline, no_check,
    )


def score_single_profile(
    profile_name: str,
    clips: list[dict],
    outputs: Path,
    rtf_by_name: dict | None,
    update_baseline: bool,
    no_check: bool,
) -> int:
    """Puntúa y evalúa un perfil contra su baseline."""
    scores = score_all(clips, outputs, rtf_by_name)

    print_table(scores, profile_name)

    report = {"clips": {s.name: s.as_dict() for s in scores}}
    report_path = outputs / "report.json"
    with open(report_path, "w", encoding="utf-8") as f:
        json.dump(report, f, indent=2, ensure_ascii=False)
    print(f"reporte en {report_path}")

    bl = baseline_path_for(profile_name)

    if update_baseline:
        write_baseline(scores, bl)
        return 0

    if bl.exists() and not no_check:
        failures = check_baseline(scores, bl)
        if failures:
            print(f"REGRESIONES (perfil {profile_name}):")
            for f in failures:
                print(f"  - {f}")
            return 1
        print(f"sin regresiones respecto a baseline_{profile_name}.json")
    elif not bl.exists() and not no_check:
        print(f"WARN: no existe {bl.name}; ejecutar con --update-baseline para crearla")
    return 0


def cmd_run(args) -> int:
    manifest = read_manifest()
    clips = clips_of(manifest, args.set, args.root)
    all_profiles = read_profiles(manifest)
    profile_names = resolve_profiles(args.profile, manifest)
    multi = len(profile_names) > 1

    exit_code = 0
    for pname in profile_names:
        pflags = all_profiles[pname].get("flags", {})
        work = work_dir_for(args.work, pname, multi)
        rc = run_single_profile(
            pname, pflags, clips, work, args.root,
            args.model_dir, args.update_baseline, args.no_check,
        )
        if rc != 0:
            exit_code = rc
    return exit_code


def cmd_score(args) -> int:
    manifest = read_manifest()
    clips = clips_of(manifest, args.set, args.root)
    profile_names = resolve_profiles(args.profile, manifest)
    multi = len(profile_names) > 1

    exit_code = 0
    for pname in profile_names:
        outputs = work_dir_for(Path(args.outputs), pname, multi) if multi else Path(args.outputs)
        rc = score_single_profile(
            pname, clips, outputs, None,
            args.update_baseline, args.no_check,
        )
        if rc != 0:
            exit_code = rc
    return exit_code


def main() -> int:
    ap = argparse.ArgumentParser(description="Benchmark de calidad de audio (denoise)")
    sub = ap.add_subparsers(dest="mode", required=True)

    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--set", default="tests_data")
    common.add_argument("--root", type=Path, default=REPO)
    common.add_argument("--profile", default="default",
                        help="perfil de flags DSP (nombre o 'all')")

    r = sub.add_parser("run", parents=[common], help="ejecutar el DSP y puntuar")
    r.add_argument("--work", type=Path, required=True)
    r.add_argument("--model-dir", type=Path, default=None)
    r.add_argument("--update-baseline", action="store_true")
    r.add_argument("--no-check", action="store_true")

    s = sub.add_parser("score", parents=[common], help="puntuar salidas existentes")
    s.add_argument("--outputs", type=Path, required=True)
    s.add_argument("--update-baseline", action="store_true")
    s.add_argument("--no-check", action="store_true")

    args = ap.parse_args()
    if args.mode == "run":
        return cmd_run(args)
    return cmd_score(args)


if __name__ == "__main__":
    sys.exit(main())