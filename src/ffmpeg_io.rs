//! Módulo `ffmpeg_io.rs` — localización de ffmpeg, probe, extract, egresión de medios y verificación ligera.
//!
//! Contrato: `docs/design.md §5, §8`.
//! Especificaciones: `docs/specifications.md §2 RF-02, RF-05, RF-07`.

use crate::errors::E;
use regex::Regex;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Clasificación canónica del medio de entrada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Video,
    Audio,
}

/// Resultado del análisis probe con ffmpeg.
#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub has_audio: bool,
    pub has_video: bool,
    pub duration: f64,
    pub channels: u16,
    pub media_kind: MediaKind,
}

/// Encuentra `ffmpeg` en `--ffmpeg-path` o en PATH (usando `PATHEXT` en Windows).
pub fn find_ffmpeg(ffmpeg_path: Option<&Path>) -> Result<PathBuf, E> {
    if let Some(p) = ffmpeg_path {
        if p.exists() {
            return Ok(p.to_path_buf());
        } else {
            return Err(E::EFfmpegNotFound(format!(
                "La ruta de ffmpeg especificada no existe: {}",
                p.display()
            )));
        }
    }

    match which::which("ffmpeg") {
        Ok(path) => Ok(path),
        Err(_) => Err(E::EFfmpegNotFound(
            "ffmpeg no encontrado en PATH. Instálalo con 'winget install Gyan.FFmpeg' (Win), 'brew install ffmpeg' (macOS) o 'apt install ffmpeg' (Linux)".to_string(),
        )),
    }
}

/// Verifica que `ffmpeg` sea versión 6+ mediante `ffmpeg -version`.
pub fn verify_ffmpeg(ffmpeg: &Path) -> Result<String, E> {
    let output = Command::new(ffmpeg)
        .arg("-version")
        .output()
        .map_err(|e| E::EFfmpegNotFound(format!("No se pudo ejecutar ffmpeg: {}", e)))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let first_line = stdout.lines().next().unwrap_or("");

    let re_ver = Regex::new(r"ffmpeg version (\d+)\.").unwrap();
    if let Some(caps) = re_ver.captures(first_line) {
        if let Some(major_str) = caps.get(1) {
            if let Ok(major) = major_str.as_str().parse::<u32>() {
                if major >= 6 {
                    return Ok(first_line.to_string());
                } else {
                    return Err(E::EFfmpegNotFound(format!(
                        "ffmpeg versión {} no soportada (se requiere versión 6+). Instala ffmpeg 6+ vía winget/brew/apt.",
                        major
                    )));
                }
            }
        }
    }

    // fallback a build git 'N-' (BtbN/gyan, nightly/master >= 6)
    if first_line.contains("ffmpeg version N-") || first_line.contains("ffmpeg version git-") {
        return Ok(first_line.to_string());
    }

    Err(E::EFfmpegNotFound(format!(
        "No se pudo determinar versión de ffmpeg (se requiere 6+): {}. Instala ffmpeg 6+ vía winget/brew/apt.",
        first_line
    )))
}

/// Detecta un stream de video real: ignora las carátulas adjuntas (`attached pic`),
/// que acompañan a algunos audios y no convierten el medio en video.
fn has_real_video_stream(stderr: &str) -> bool {
    stderr.lines().any(|line| {
        line.contains("Video:") && !line.contains("attached pic") && !line.contains("attached_pic")
    })
}

fn is_mov_mp4(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    matches!(
        ext.as_str(),
        "mp4" | "mov" | "m4v" | "m4a" | "3gp" | "3g2" | "mj2"
    )
}

/// Analiza el contenedor de entrada mediante `ffmpeg -hide_banner -i`.
pub fn probe(ffmpeg: &Path, input: &Path) -> Result<ProbeResult, E> {
    if !input.exists() {
        return Err(E::EInvalidInput(format!(
            "Archivo no encontrado: {}",
            input.display()
        )));
    }

    let meta = std::fs::metadata(input).map_err(E::EIo)?;
    if meta.len() == 0 {
        return Err(E::EInvalidInput(format!(
            "El archivo tiene 0 bytes: {}",
            input.display()
        )));
    }

    let mut cmd = Command::new(ffmpeg);
    cmd.arg("-hide_banner");
    if is_mov_mp4(input) {
        cmd.arg("-ignore_editlist").arg("1");
    }
    cmd.arg("-i").arg(input);

    let output = cmd
        .output()
        .map_err(|e| E::EFfmpegFailed(format!("Error ejecutando probe: {}", e)))?;

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // Parsear duración del contenedor
    let re_dur = Regex::new(r"Duration:\s*(\d+):(\d+):(\d+(?:\.\d+)?)").unwrap();
    let duration = if let Some(caps) = re_dur.captures(&stderr) {
        let h: f64 = caps
            .get(1)
            .map_or(0.0, |m| m.as_str().parse().unwrap_or(0.0));
        let m: f64 = caps
            .get(2)
            .map_or(0.0, |m| m.as_str().parse().unwrap_or(0.0));
        let s: f64 = caps
            .get(3)
            .map_or(0.0, |m| m.as_str().parse().unwrap_or(0.0));
        h * 3600.0 + m * 60.0 + s
    } else {
        0.0
    };

    let has_video = has_real_video_stream(&stderr);
    let has_audio = stderr.contains("Audio:");

    // validaciones probe
    if !has_video && !has_audio {
        return Err(E::EInvalidInput(format!(
            "Archivo inválido o corrupto (sin streams de medios reconocidos): {}",
            input.display()
        )));
    }

    if has_video && !has_audio {
        return Err(E::ENoAudio);
    }

    let media_kind = if has_video {
        MediaKind::Video
    } else {
        MediaKind::Audio
    };

    let channels = if has_audio {
        let re_chan = Regex::new(r"Audio:.*?(mono|stereo|(\d+)\s*channels?)").unwrap();
        if let Some(caps) = re_chan.captures(&stderr) {
            if let Some(m) = caps.get(1) {
                let s = m.as_str();
                if s == "mono" {
                    1
                } else if s == "stereo" {
                    2
                } else if let Some(digits) = caps.get(2) {
                    digits.as_str().parse::<u16>().unwrap_or(2).clamp(1, 2)
                } else {
                    2
                }
            } else {
                2
            }
        } else if stderr.contains("stereo") {
            2
        } else {
            1
        }
    } else {
        0
    };

    Ok(ProbeResult {
        has_audio,
        has_video,
        duration,
        channels,
        media_kind,
    })
}

/// Extrae la primera pista de audio a WAV 48kHz PCM16 con la cantidad de canales especificada:
/// `ffmpeg -y -v error -i IN -map 0:a:0 -vn -ac <channels> -ar 48000 -sample_fmt s16 TMP.in.wav`
///
/// `-sample_fmt s16` es obligatorio: sin él FFmpeg puede emitir `flt` (32-bit float)
/// cuando el contenedor de entrada lo sugiere, lo que causaría un fallo silencioso
/// en `WavReader::samples::<i16>()` o datos incorrectos en el pipeline DSP.
pub fn extract_audio48k(
    ffmpeg: &Path,
    input: &Path,
    tmp_wav: &Path,
    channels: u16,
) -> Result<(), E> {
    let mut cmd = Command::new(ffmpeg);
    cmd.arg("-y").arg("-v").arg("error");
    if is_mov_mp4(input) {
        cmd.arg("-ignore_editlist").arg("1");
    }
    cmd.arg("-i")
        .arg(input)
        .arg("-map")
        .arg("0:a:0")
        .arg("-vn")
        .arg("-ac")
        .arg(channels.to_string())
        .arg("-ar")
        .arg("48000")
        .arg("-sample_fmt")
        .arg("s16")
        .arg(tmp_wav);

    let status = cmd
        .status()
        .map_err(|e| E::EFfmpegFailed(format!("Fallo al ejecutar extracción ffmpeg: {}", e)))?;

    if !status.success() {
        return Err(E::EFfmpegFailed(format!(
            "ffmpeg falló al extraer audio de {}",
            input.display()
        )));
    }

    Ok(())
}

/// Extrae la primera pista de audio a WAV mono 48kHz PCM16:
pub fn extract_mono48k(ffmpeg: &Path, input: &Path, tmp_wav: &Path) -> Result<(), E> {
    extract_audio48k(ffmpeg, input, tmp_wav, 1)
}

/// Remuxa el video original con el audio procesado a AAC (con normalización loudnorm opcional):
/// `ffmpeg -y -v error -i IN -i TMP.out.wav -map 0:v:0 -map 1:a:0 -c:v copy -c:a aac -b:a <bitrate>k [-af loudnorm=I=<target_lufs>:TP=-1.5:LRA=11] -t <dur_video> OUT.part.mp4`
pub fn remux_video(
    ffmpeg: &Path,
    input_video: &Path,
    clean_wav: &Path,
    out_part: &Path,
    bitrate_kbps: u32,
    dur_video: f64,
    target_lufs: Option<f64>,
) -> Result<(), E> {
    let dur_str = format!("{:.3}", dur_video);
    let bitrate_str = format!("{}k", bitrate_kbps);

    let mut cmd = Command::new(ffmpeg);
    cmd.arg("-y").arg("-v").arg("error");
    if is_mov_mp4(input_video) {
        cmd.arg("-ignore_editlist").arg("1");
    }
    cmd.arg("-i")
        .arg(input_video)
        .arg("-i")
        .arg(clean_wav)
        .arg("-map")
        .arg("0:v:0")
        .arg("-map")
        .arg("1:a:0")
        .arg("-c:v")
        .arg("copy")
        .arg("-c:a")
        .arg("aac")
        .arg("-b:a")
        .arg(&bitrate_str);

    if let Some(lufs) = target_lufs {
        cmd.arg("-af")
            .arg(format!("loudnorm=I={:.1}:TP=-1.5:LRA=11", lufs));
    }

    cmd.arg("-t").arg(&dur_str).arg(out_part);

    let status = cmd
        .status()
        .map_err(|e| E::EFfmpegFailed(format!("Fallo al ejecutar remux ffmpeg: {}", e)))?;

    if !status.success() {
        return Err(E::EFfmpegFailed(format!(
            "ffmpeg falló en remux para {}",
            out_part.display()
        )));
    }

    Ok(())
}

/// Devuelve el códec de audio de egresión según la extensión del archivo de salida.
fn audio_codec_for(ext: &str) -> Option<&'static str> {
    match ext {
        "wav" => Some("pcm_s16le"),
        "mp3" => Some("libmp3lame"),
        "m4a" | "aac" | "mp4" | "mov" => Some("aac"),
        "flac" => Some("flac"),
        "ogg" => Some("libvorbis"),
        "opus" => Some("libopus"),
        _ => None,
    }
}

/// Empaqueta el audio limpio preservando el contenedor pedido por la extensión
/// de `out_part`, con metadatos del medio original cuando el contenedor lo permite.
pub fn encode_audio(
    ffmpeg: &Path,
    input: &Path,
    clean_wav: &Path,
    out_part: &Path,
    bitrate_kbps: u32,
    target_lufs: Option<f64>,
) -> Result<(), E> {
    let ext = out_part
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    let codec = audio_codec_for(&ext).ok_or_else(|| {
        E::EInvalidInput(format!(
            "Extensión de salida de audio no soportada: '{}'",
            out_part.display()
        ))
    })?;

    let mut cmd = Command::new(ffmpeg);
    cmd.arg("-y").arg("-v").arg("error");

    if ext == "wav" {
        // Escritura PCM16 sin pérdida desde el WAV limpio del DSP.
        cmd.arg("-i").arg(clean_wav);
    } else {
        // Se abre el medio original para heredar sus metadatos de contenedor.
        if is_mov_mp4(input) {
            cmd.arg("-ignore_editlist").arg("1");
        }
        cmd.arg("-i")
            .arg(input)
            .arg("-i")
            .arg(clean_wav)
            .arg("-map")
            .arg("1:a:0")
            .arg("-map_metadata")
            .arg("0");
    }

    cmd.arg("-c:a").arg(codec);

    // Bitrate solo aplica a códecs con pérdida; PCM16 y FLAC son sin pérdida.
    if matches!(
        ext.as_str(),
        "mp3" | "m4a" | "aac" | "mp4" | "mov" | "ogg" | "opus"
    ) {
        cmd.arg("-b:a").arg(format!("{}k", bitrate_kbps));
    }

    if let Some(lufs) = target_lufs {
        cmd.arg("-af")
            .arg(format!("loudnorm=I={:.1}:TP=-1.5:LRA=11", lufs));
    }

    cmd.arg(out_part);

    let status = cmd
        .status()
        .map_err(|e| E::EFfmpegFailed(format!("Fallo al ejecutar egresión de audio: {}", e)))?;

    if !status.success() {
        return Err(E::EFfmpegFailed(format!(
            "ffmpeg falló al empaquetar audio para {}",
            out_part.display()
        )));
    }

    Ok(())
}

/// Orquesta la egresión según el tipo de medio de entrada.
#[allow(clippy::too_many_arguments)]
pub fn egress_media(
    ffmpeg: &Path,
    kind: MediaKind,
    input: &Path,
    clean_wav: &Path,
    out_part: &Path,
    bitrate_kbps: u32,
    duration: f64,
    target_lufs: Option<f64>,
) -> Result<(), E> {
    match kind {
        MediaKind::Video => remux_video(
            ffmpeg,
            input,
            clean_wav,
            out_part,
            bitrate_kbps,
            duration,
            target_lufs,
        ),
        MediaKind::Audio => encode_audio(
            ffmpeg,
            input,
            clean_wav,
            out_part,
            bitrate_kbps,
            target_lufs,
        ),
    }
}

/// Verificación ligera runtime: OUT existe >0B + duración ±0.5s + stream Audio presente.
/// Solo exige stream de video cuando el medio es `MediaKind::Video`.
pub fn verify_output_lightweight(
    ffmpeg: &Path,
    out_file: &Path,
    expected_dur: f64,
    kind: MediaKind,
) -> Result<bool, E> {
    if !out_file.exists() {
        return Ok(false);
    }
    let meta = std::fs::metadata(out_file).map_err(E::EIo)?;
    if meta.len() == 0 {
        return Ok(false);
    }

    let output = Command::new(ffmpeg)
        .arg("-hide_banner")
        .arg("-i")
        .arg(out_file)
        .output()
        .map_err(|e| E::EFfmpegFailed(format!("Error verificando salida: {}", e)))?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.contains("Audio:") {
        return Ok(false);
    }

    if kind == MediaKind::Video && !stderr.contains("Video:") {
        return Ok(false);
    }

    let re_dur = Regex::new(r"Duration:\s*(\d+):(\d+):(\d+(?:\.\d+)?)").unwrap();
    if let Some(caps) = re_dur.captures(&stderr) {
        let h: f64 = caps
            .get(1)
            .map_or(0.0, |m| m.as_str().parse().unwrap_or(0.0));
        let m: f64 = caps
            .get(2)
            .map_or(0.0, |m| m.as_str().parse().unwrap_or(0.0));
        let s: f64 = caps
            .get(3)
            .map_or(0.0, |m| m.as_str().parse().unwrap_or(0.0));
        let actual_dur = h * 3600.0 + m * 60.0 + s;
        if (actual_dur - expected_dur).abs() > 0.5 {
            return Ok(false);
        }
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::has_real_video_stream;

    #[test]
    fn test_attached_picture_is_not_a_video_stream() {
        let audio_with_cover = "  Stream #0:0: Audio: mp3, 48000 Hz, stereo\n  Stream #0:1: Video: mjpeg (attached pic), yuvj420p, 600x600";
        assert!(!has_real_video_stream(audio_with_cover));

        let audio_only = "  Stream #0:0: Audio: pcm_s16le, 48000 Hz, mono";
        assert!(!has_real_video_stream(audio_only));

        let real_video = "  Stream #0:0: Video: h264, yuv420p, 1920x1080\n  Stream #0:1: Audio: aac, 48000 Hz, stereo";
        assert!(has_real_video_stream(real_video));
    }
}
