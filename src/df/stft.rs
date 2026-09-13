//! Módulo `df::stft` — Framing, STFT, iSTFT, WNORM.
//!
//! Ver `plan.md T4.1` y `design.md §6`.
//! Constantes: `FFT960 / HOP480 / LOOKAHEAD2`.

/// Framing streaming: frame t = [(t-1)*HOP, (t+1)*HOP) + pad inicial HOP + cola FFT+LOOKAHEAD*HOP.
pub fn frame_signal(_samples: &[f32], _hop: usize) -> Vec<Vec<f32>> {
    // TODO: implementar framing
    vec![]
}

/// STFT con ventana vorbis + WNORM.
pub fn stft(_frame: &[f32]) -> Vec<Vec<f32>> {
    // TODO: implementar STFT
    vec![]
}

/// iSTFT con ventana + overlap-add.
pub fn istft(_spectrum: &[Vec<f32>], _hop: usize) -> Vec<f32> {
    // TODO: implementar iSTFT
    vec![]
}
