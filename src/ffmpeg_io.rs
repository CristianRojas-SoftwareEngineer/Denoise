//! Módulo `ffmpeg_io.rs` — localización de ffmpeg, probe, extract, remux y verificación ligera.
//!
//! Contrato: `docs/design.md §5, §8`.
//! Especificaciones: `docs/specifications.md §2 RF-05, RF-07`.

use crate::errors::E;
use regex::Regex;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Resultado del análisis probe con ffmpeg.
#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub has_audio: bool,
    pub has_video: bool,
    pub duration: f64,
    pub raw_stderr: String,
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

    let has_video = stderr.contains("Video:");
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

    if !has_video && has_audio {
        return Err(E::EInvalidInput(format!(
            "Entrada es solo audio, se requiere archivo de video: {}",
            input.display()
        )));
    }

    Ok(ProbeResult {
        has_audio,
        has_video,
        duration,
        raw_stderr: stderr,
    })
}

/// Extrae la primera pista de audio a WAV mono 48kHz PCM16:
/// `ffmpeg -y -v error -i IN -map 0:a:0 -vn -ac 1 -ar 48000 TMP.in.wav`
pub fn extract_mono48k(ffmpeg: &Path, input: &Path, tmp_wav: &Path) -> Result<(), E> {
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
        .arg("1")
        .arg("-ar")
        .arg("48000")
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

/// Remuxa el video original con el audio procesado a AAC:
/// `ffmpeg -y -v error -i IN -i TMP.out.wav -map 0:v:0 -map 1:a:0 -c:v copy -c:a aac -b:a <bitrate>k -t <dur_video> OUT.part.mp4`
pub fn remux_copy(
    ffmpeg: &Path,
    input_video: &Path,
    clean_wav: &Path,
    out_part_mp4: &Path,
    bitrate_kbps: u32,
    dur_video: f64,
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
        .arg(&bitrate_str)
        .arg("-t")
        .arg(&dur_str)
        .arg(out_part_mp4);

    let status = cmd
        .status()
        .map_err(|e| E::EFfmpegFailed(format!("Fallo al ejecutar remux ffmpeg: {}", e)))?;

    if !status.success() {
        return Err(E::EFfmpegFailed(format!(
            "ffmpeg falló en remux para {}",
            out_part_mp4.display()
        )));
    }

    Ok(())
}

/// Verificación ligera runtime: OUT existe >0B + duración ±0.5s + Audio AAC presente.
pub fn verify_output_ligero(ffmpeg: &Path, out_mp4: &Path, expected_dur: f64) -> Result<bool, E> {
    if !out_mp4.exists() {
        return Ok(false);
    }
    let meta = std::fs::metadata(out_mp4).map_err(E::EIo)?;
    if meta.len() == 0 {
        return Ok(false);
    }

    let output = Command::new(ffmpeg)
        .arg("-hide_banner")
        .arg("-i")
        .arg(out_mp4)
        .output()
        .map_err(|e| E::EFfmpegFailed(format!("Error verificando salida: {}", e)))?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.contains("Audio:") {
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
