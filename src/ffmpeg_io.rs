/// Módulo `ffmpeg_io.rs` — resolución de ffmpeg, probe, extract, remux, verificación ligera.
///
/// Ver `plan.md T2.1/T2.2/T2.3/T2.4` y `design.md §8`.
/// Requiere `ffmpeg 6+` verificado por regex `ffmpeg version (\d+)\.`.
use std::path::Path;

/// Encuentra `ffmpeg` en PATH (con `PATHEXT` en Win).
pub fn find_ffmpeg(_ffmpeg_path: Option<&Path>) -> anyhow::Result<PathBuf> {
    // TODO: which + PATHEXT
    anyhow::bail!("ffmpeg not found (Fase 2)");
}

/// Verifica que `ffmpeg` sea versión 6+ mediante `ffmpeg -version`.
pub fn verify_ffmpeg(_ffmpeg: &Path) -> anyhow::Result<String> {
    // TODO: regex ffmpeg version (\d+)\. major>=6
    anyhow::bail!("ffmpeg version check not implemented (Fase 2)");
}

/// Detecta si el input tiene pista de audio (`Audio:` en `ffmpeg -hide_banner -i`).
pub fn has_audio(_ffmpeg: &Path, _input: &Path) -> anyhow::Result<bool> {
    // TODO: probe sin ffprobe
    anyhow::bail!("has_audio not implemented (Fase 2)");
}

/// Extrae mono 48k PCM16 a WAV: `ffmpeg -y -v error -i IN -map 0:a:0 -vn -ac 1 -ar 48000 TMP.in.wav`.
pub fn extract_mono48k(_ffmpeg: &Path, _input: &Path, _tmp_wav: &Path) -> anyhow::Result<()> {
    // TODO: Command argv + from_utf8_lossy
    anyhow::bail!("extract not implemented (Fase 3)");
}

/// Remux con copy de video + AAC: `-c:v copy -c:a aac -b:a <bitrate>k -t <dur_video> OUT.part.mp4`.
pub fn remux_copy(
    _ffmpeg: &Path,
    _wav: &Path,
    _input: &Path,
    _out: &Path,
    _bitrate: u32,
    _dur_video: f64,
) -> anyhow::Result<()> {
    // TODO: Command argv + rename atómico
    anyhow::bail!("remux not implemented (Fase 3)");
}

/// Verificación ligera: OUT >0B + duración ±0.5s + Audio AAC presente.
pub fn verify_output(_ffmpeg: &Path, _out: &Path) -> anyhow::Result<bool> {
    // TODO: probe salida
    anyhow::bail!("verify not implemented (Fase 3)");
}

use std::path::PathBuf;
