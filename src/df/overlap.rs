//! Módulo `df::overlap` — crossfade y recorte entre chunks.
//!
//! Ver `plan.md T4.1` y `design.md §6`.
//! `OVERLAP = 1s`, crossfade lineal.

/// Crossfade lineal entre el final del chunk previo y el inicio del actual.
pub fn crossfade(_prev: &[f32], _current: &[f32], _overlap: usize) -> Vec<f32> {
    // TODO: implementar crossfade lineal
    vec![]
}

/// Recorte del buffer al tamaño de salida válido (HOP:HOP+n).
pub fn trim(_samples: &[f32], _hop: usize, _n: usize) -> Vec<f32> {
    // TODO: implementar recorte
    vec![]
}
