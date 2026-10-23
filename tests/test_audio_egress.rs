//! Test rápido de egresión de audio (sin modelo ONNX).
//!
//! Ver `design.md §10 punto 5` y `specifications.md RNF-07`.
//!
//! Verifica la matriz de códecs de `encode_audio` y la verificación ligera
//! adaptada a `MediaKind::Audio`. Requiere `ffmpeg 6+` con los códecs de la
//! matriz (`libmp3lame`, `aac`, `flac`, `libvorbis`, `libopus`).

use denoise::ffmpeg_io::{encode_audio, find_ffmpeg, probe, verify_output_lightweight, MediaKind};
use hound::{WavSpec, WavWriter};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn write_clean_wav(path: &Path, duration_secs: usize) {
    let spec = WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec).expect("Crear WAV limpio");
    for i in 0..(duration_secs * 48_000) {
        let sample = ((i as f32 * 0.05).sin() * 10_000.0) as i16;
        writer.write_sample(sample).expect("Escribir muestra WAV");
    }
    writer.finalize().expect("Finalizar WAV");
}

fn audio_codec_tag(ffmpeg: &Path, path: &Path) -> String {
    let output = Command::new(ffmpeg)
        .arg("-hide_banner")
        .arg("-i")
        .arg(path)
        .output()
        .expect("Ejecutar ffmpeg para leer el códec");
    let stderr = String::from_utf8_lossy(&output.stderr);
    stderr
        .lines()
        .find_map(|line| line.split("Audio: ").nth(1))
        .and_then(|rest| rest.split(',').next())
        .map(|codec| codec.trim().to_string())
        .unwrap_or_default()
}

fn assert_decodable(ffmpeg: &Path, path: &Path) {
    let status = Command::new(ffmpeg)
        .arg("-v")
        .arg("error")
        .arg("-i")
        .arg(path)
        .arg("-f")
        .arg("null")
        .arg("-")
        .status()
        .expect("Ejecutar decodificación ffmpeg");
    assert!(
        status.success(),
        "La salida no es decodificable: {:?}",
        path
    );
}

#[test]
fn test_encode_audio_codec_matrix() {
    let ffmpeg = find_ffmpeg(None).expect("ffmpeg 6+ debe estar instalado");
    let tmp_dir = std::env::temp_dir().join("denoise_audio_egress");
    let _ = fs::remove_dir_all(&tmp_dir);
    fs::create_dir_all(&tmp_dir).expect("Crear directorio temporal del test");

    let clean_wav = tmp_dir.join("clean.wav");
    write_clean_wav(&clean_wav, 1);

    // (extensión final, prefijo del códec esperado en ffmpeg -i)
    let cases = [
        ("wav", "pcm_s16le"),
        ("mp3", "mp3"),
        ("m4a", "aac"),
        ("aac", "aac"),
        ("flac", "flac"),
        ("ogg", "vorbis"),
        ("opus", "opus"),
    ];

    for (ext, expected_codec) in cases {
        let out_part: PathBuf = tmp_dir.join(format!("audio.part.{}", ext));
        encode_audio(&ffmpeg, &clean_wav, &clean_wav, &out_part, 128, None)
            .unwrap_or_else(|e| panic!("encode_audio falló para .{}: {:?}", ext, e));

        let meta = fs::metadata(&out_part).expect("La salida de audio debe existir");
        assert!(meta.len() > 0, "La salida .{} está vacía", ext);

        let probed = probe(&ffmpeg, &out_part)
            .unwrap_or_else(|e| panic!("probe falló para .{}: {:?}", ext, e));
        assert!(
            probed.has_audio,
            "La salida .{} no tiene stream de audio",
            ext
        );
        assert!(!probed.has_video, "La salida .{} no debe tener video", ext);
        assert_eq!(probed.media_kind, MediaKind::Audio);

        let codec = audio_codec_tag(&ffmpeg, &out_part);
        assert!(
            codec.starts_with(expected_codec),
            "Códec inesperado para .{}: '{}' (esperado '{}')",
            ext,
            codec,
            expected_codec
        );

        assert_decodable(&ffmpeg, &out_part);

        let verified = verify_output_lightweight(&ffmpeg, &out_part, 1.0, MediaKind::Audio)
            .unwrap_or_else(|e| panic!("Verificación falló para .{}: {:?}", ext, e));
        assert!(
            verified,
            "La salida .{} no pasó la verificación ligera",
            ext
        );
    }

    let _ = fs::remove_dir_all(&tmp_dir);
}
