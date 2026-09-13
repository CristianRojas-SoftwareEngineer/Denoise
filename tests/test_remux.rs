//! Test remux `#[ignore]` — verifica que el remux preserva video sin re-encode.
//!
//! Ver `specifications.md RF-05B` y `design.md §10 punto 3`.
//! Ejecutar con `cargo test -- --ignored`.

use denoise::ffmpeg_io::{find_ffmpeg, probe, remux_copy, verify_output_ligero};
use hound::{WavSpec, WavWriter};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::process::Command;

fn generate_fixture_video(ffmpeg: &Path, out_path: &Path, v_dur: u32, a_dur: u32) {
    let status = Command::new(ffmpeg)
        .arg("-y")
        .arg("-v")
        .arg("error")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg(format!("testsrc=size=640x480:rate=30:duration={}", v_dur))
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg(format!(
            "sine=frequency=440:sample_rate=48000:duration={}",
            a_dur
        ))
        .arg("-c:v")
        .arg("libx264")
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg("-c:a")
        .arg("aac")
        .arg("-b:a")
        .arg("192k")
        .arg(out_path)
        .status()
        .expect("Generar fixture de video con ffmpeg");

    assert!(status.success(), "ffmpeg debe generar fixture de video");
}

fn generate_dummy_wav(path: &Path, duration_secs: usize) {
    let spec = WavSpec {
        channels: 1,
        sample_rate: 48000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec).expect("Crear WAV");
    for i in 0..(duration_secs * 48000) {
        let sample = ((i as f32 * 0.05).sin() * 10000.0) as i16;
        writer.write_sample(sample).unwrap();
    }
    writer.finalize().unwrap();
}

fn extract_video_stream_hash(ffmpeg: &Path, video_path: &Path) -> Vec<u8> {
    let output = Command::new(ffmpeg)
        .arg("-y")
        .arg("-v")
        .arg("error")
        .arg("-i")
        .arg(video_path)
        .arg("-map")
        .arg("0:v:0")
        .arg("-c")
        .arg("copy")
        .arg("-f")
        .arg("h264")
        .arg("-")
        .output()
        .expect("Extraer flujo de video h264 crudo");

    assert!(
        output.status.success(),
        "Extracción de h264 debe tener éxito"
    );
    let mut hasher = Sha256::new();
    hasher.update(&output.stdout);
    hasher.finalize().to_vec()
}

#[test]
#[ignore]
fn test_remux_copy_video() {
    let ffmpeg = find_ffmpeg(None).expect("ffmpeg disponible");
    let temp_dir = std::env::temp_dir();
    let fixture_path = temp_dir.join("test_remux_in.mp4");
    let clean_wav_path = temp_dir.join("test_remux_clean.wav");
    let out_path = temp_dir.join("test_remux_out.mp4");

    generate_fixture_video(&ffmpeg, &fixture_path, 3, 3);
    generate_dummy_wav(&clean_wav_path, 3);

    let probe_in = probe(&ffmpeg, &fixture_path).expect("Probe de entrada");
    assert!(probe_in.has_video);
    assert!(probe_in.has_audio);

    let in_hash = extract_video_stream_hash(&ffmpeg, &fixture_path);

    remux_copy(
        &ffmpeg,
        &fixture_path,
        &clean_wav_path,
        &out_path,
        192,
        probe_in.duration,
    )
    .expect("Remux exitoso");

    let out_hash = extract_video_stream_hash(&ffmpeg, &out_path);
    assert_eq!(
        in_hash, out_hash,
        "El flujo H.264 copiado debe ser bit-a-bit idéntico al original (sin re-encode)"
    );

    let verified =
        verify_output_ligero(&ffmpeg, &out_path, probe_in.duration).expect("Verificación ligera");
    assert!(
        verified,
        "La salida remuxada debe pasar la verificación ligera"
    );

    let _ = fs::remove_file(&fixture_path);
    let _ = fs::remove_file(&clean_wav_path);
    let _ = fs::remove_file(&out_path);
}

#[test]
#[ignore]
fn test_remux_duracion_contenedor() {
    let ffmpeg = find_ffmpeg(None).expect("ffmpeg disponible");
    let temp_dir = std::env::temp_dir();
    let fixture_path = temp_dir.join("test_remux_dur_in.mp4");
    let clean_wav_path = temp_dir.join("test_remux_dur_clean.wav");
    let out_path = temp_dir.join("test_remux_dur_out.mp4");

    // Audio más largo que video (D_k)
    generate_fixture_video(&ffmpeg, &fixture_path, 2, 4);
    generate_dummy_wav(&clean_wav_path, 4);

    let probe_in = probe(&ffmpeg, &fixture_path).expect("Probe de contenedor");
    assert!(
        (probe_in.duration - 4.0).abs() < 0.5,
        "Duración de contenedor esperada ~4s"
    );

    remux_copy(
        &ffmpeg,
        &fixture_path,
        &clean_wav_path,
        &out_path,
        192,
        probe_in.duration,
    )
    .expect("Remux con duración de contenedor");

    let probe_out = probe(&ffmpeg, &out_path).expect("Probe de salida");
    assert!(
        (probe_out.duration - probe_in.duration).abs() < 0.5,
        "Salida debe coincidir con duración del contenedor"
    );

    let _ = fs::remove_file(&fixture_path);
    let _ = fs::remove_file(&clean_wav_path);
    let _ = fs::remove_file(&out_path);
}
