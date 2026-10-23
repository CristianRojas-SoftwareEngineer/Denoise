"""Pruebas unitarias para las herramientas auxiliares de calidad y métricas."""

from __future__ import annotations

import unittest
import numpy as np

import metrics
import worst_windows


class TestQualityTools(unittest.TestCase):
    def test_normalize_transcript(self):
        text = "¡Hola! Esto es una prueba de transcripción con tildes, números 123 y signos."
        expected = ["hola", "esto", "es", "una", "prueba", "de", "transcripcion", "con", "tildes", "numeros", "123", "y", "signos"]
        self.assertEqual(metrics.normalize_transcript(text), expected)

    def test_calculate_wer_identical(self):
        ref = ["palabra", "de", "prueba"]
        hyp = ["palabra", "de", "prueba"]
        wer_val, (s, d, i) = metrics.calculate_wer(ref, hyp)
        self.assertEqual(wer_val, 0.0)
        self.assertEqual((s, d, i), (0, 0, 0))

    def test_calculate_wer_sub_del_ins(self):
        ref = ["el", "perro", "corre", "rapido"]
        hyp = ["el", "gato", "salta", "corre", "muy", "rapido"]
        # gato reemplaza perro (1 sub), salta insertado (1 ins), muy insertado (1 ins) -> 3 errores / 4 palabras ref = 0.75
        wer_val, (s, d, i) = metrics.calculate_wer(ref, hyp)
        self.assertAlmostEqual(wer_val, 3 / 4)
        self.assertEqual(s, 1)
        self.assertEqual(d, 0)
        self.assertEqual(i, 2)

    def test_calculate_wer_empty(self):
        self.assertEqual(metrics.calculate_wer([], [])[0], 0.0)
        self.assertEqual(metrics.calculate_wer([], ["extra"])[0], 1.0)
        self.assertEqual(metrics.calculate_wer(["falta"], [])[0], 1.0)

    def test_find_worst_windows_ordering(self):
        # 3 segundos de señales sintéticas
        sr = worst_windows.SAMPLE_RATE
        n_samples = sr * 3
        ref = np.sin(2 * np.pi * 440 * np.arange(n_samples) / sr)
        mix = ref + 0.1 * np.cos(2 * np.pi * 1000 * np.arange(n_samples) / sr)
        est = mix.copy()
        # Modificar el segundo 1 para que tenga mucha distorsión
        est[sr : 2 * sr] += 2.0 * np.random.randn(sr)

        worst = worst_windows.find_worst_windows(ref, est, mix, top_k=2)
        self.assertGreaterEqual(len(worst), 1)
        # El peor segundo debe ser el segundo 1
        worst_sec, worst_gain = worst[0]
        self.assertEqual(worst_sec, 1)


if __name__ == "__main__":
    unittest.main()
