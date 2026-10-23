"""Cálculo de WER (Word Error Rate) limpia-vs-procesada con Whisper.

Transcribe la voz limpia (referencia, cacheada) y la salida del DSP por
clip, y calcula el WER palabra a palabra. Si el procesado pierde palabras,
el WER lo detecta aunque los promedios globales salgan bien.

Criterio: el WER de la señal procesada se evalúa relativamente entre
cambios del DSP (referencia = transcripción de la señal limpia con el mismo
modelo); cualquier clip con WER > 0 requiere inspección auditiva (ver `worst_windows.py`).

Uso:
    python tools/quality/wer.py --set tests_data --work out/bench
    python tools/quality/wer.py --set tests_data --work out/bench --model base --no-cache
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from faster_whisper import WhisperModel

import metrics

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]


def transcribe_audio(model: WhisperModel, audio_path: Path) -> str:
    """Transcribe un archivo de audio a texto usando WhisperModel."""
    segments, _ = model.transcribe(str(audio_path), language="es", beam_size=5)
    return " ".join(segment.text for segment in segments).strip()


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Calcula WER de la salida del DSP contra la voz limpia de referencia."
    )
    parser.add_argument("--set", default="tests_data", help="Nombre del set en clips.json")
    parser.add_argument("--work", default="out/bench", help="Directorio con salidas del benchmark")
    parser.add_argument("--model", default="small", help="Modelo de faster-whisper")
    parser.add_argument("--cache", default="out/wer/cache.json", help="Ruta al archivo de caché JSON")
    parser.add_argument("--no-cache", action="store_true", help="Desactiva la lectura/escritura de caché")
    args = parser.parse_args()

    clips = json.load(open(HERE / "clips.json", encoding="utf-8"))["sets"][args.set]["clips"]
    work_dir = REPO / args.work
    cache_path = REPO / args.cache
    cache: dict[str, str] = {}
    if not args.no_cache and cache_path.exists():
        with open(cache_path, encoding="utf-8") as f:
            cache = json.load(f)

    print(f"Cargando faster-whisper '{args.model}' (cpu int8)...")
    model = WhisperModel(args.model, device="cpu", compute_type="int8")

    print(f"{'clip':<15}{'words':>9}{'WER':>8}  S/D/I")
    degraded_count = 0

    for clip in clips:
        transcripts: dict[str, str] = {}
        for role, file_path in (("clean", REPO / clip["clean"]), ("out", work_dir / clip["output_name"])):
            key = f"{args.model}:{role}:{clip['name']}"
            if key not in cache:
                cache[key] = transcribe_audio(model, file_path)
            transcripts[role] = cache[key]

        ref_words = metrics.normalize_transcript(transcripts["clean"])
        hyp_words = metrics.normalize_transcript(transcripts["out"])
        wer_value, (sub_cnt, del_cnt, ins_cnt) = metrics.calculate_wer(ref_words, hyp_words)

        warning_tag = "  <-- REVISAR" if wer_value > 0 else ""
        if wer_value > 0:
            degraded_count += 1
        print(f"{clip['name']:<15}{len(ref_words):>9}{wer_value:>7.1%}{sub_cnt:>4}/{del_cnt:<3}/{ins_cnt:<3}{warning_tag}")

    if not args.no_cache:
        cache_path.parent.mkdir(parents=True, exist_ok=True)
        with open(cache_path, "w", encoding="utf-8") as f:
            json.dump(cache, f, ensure_ascii=False, indent=1)

    print(f"Clips con WER > 0: {degraded_count}/{len(clips)} (caché en {cache_path})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
