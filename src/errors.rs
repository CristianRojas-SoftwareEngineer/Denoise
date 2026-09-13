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
    /// ECancelled → 3, EInvalidInput/EOutputExists/ENoAudio → 2,
    /// EFfmpegNotFound/EModelMissing/EFfmpegFailed/EIo → 1.
    pub fn exit_code(&self) -> i32 {
        match self {
            E::ECancelled => 3,
            E::EInvalidInput(_) | E::EOutputExists(_) | E::ENoAudio => 2,
            E::EFfmpegNotFound | E::EModelMissing(_) | E::EFfmpegFailed(_) | E::EIo(_) => 1,
        }
    }
}
