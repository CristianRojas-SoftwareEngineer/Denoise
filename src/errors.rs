//! Módulo `errors.rs` — enum `E_*` + `exit_code()`.
//!
//! Creado primero (T1.0), testeable puro sin I/O.
//! Ver `specifications.md §2-4` y `design.md §8`.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum E {
    #[error("Entrada inválida: {0}")]
    EInvalidInput(String),
    #[error("Destino ya existe: {0}")]
    EOutputExists(String),
    #[error("Sin pista de audio")]
    ENoAudio,
    #[error("ffmpeg no encontrado")]
    EFfmpegNotFound,
    #[error("Modelo faltante: {0}")]
    EModelMissing(String),
    #[error("ffmpeg falló: {0}")]
    EFfmpegFailed(String),
    #[error("Error de E/S: {0}")]
    EIo(anyhow::Error),
    #[error("Cancelado por el usuario")]
    ECancelled,
}

impl E {
    /// Código de salida asociado al error.
    /// `EIo → 1`; ver `specifications.md RNF-06`.
    pub fn exit_code(&self) -> i32 {
        match self {
            E::EIo(_) => 1,
            _ => 1,
        }
    }
}
