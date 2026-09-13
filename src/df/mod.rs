//! Módulo `df` — DeepFilterNet3 denoising DSP.
//!
//! Submódulos: `stft`, `erb`, `net`, `overlap`.
//! Ver `plan.md T4.1` y `design.md §6`.
//! Cada submódulo <300 líneas (D40 opción A).

pub mod erb;
pub mod net;
pub mod overlap;
pub mod stft;

use std::path::Path;
use std::sync::Mutex;

/// Punto de entrada del DSP: denoisa un WAV mono 48k PCM16 → WAV limpio.
///
/// Progreso por callback `(processed_seconds, total_seconds)`.
pub fn denoise_wav(
    _in_wav: &Path,
    _out_wav: &Path,
    progress_cb: impl Fn(f64, f64),
) -> anyhow::Result<()> {
    // TODO: implementar pipeline DSP (STFT → ERB features → DF3 nets → iSTFT → overlap-add)
    let _ = progress_cb;
    anyhow::bail!("DF3 denoising not yet implemented (Fase 3)");
}

/// Sesiones `ort` cacheadas por modelo.
/// Ver `design.md §7` y `plan.md T2.2`.
pub struct OrtSessionCache {
    _sessions: Mutex<Vec<String>>,
}

impl OrtSessionCache {
    pub fn new() -> Self {
        Self {
            _sessions: Mutex::new(Vec::new()),
        }
    }
}

impl Default for OrtSessionCache {
    fn default() -> Self {
        Self::new()
    }
}
