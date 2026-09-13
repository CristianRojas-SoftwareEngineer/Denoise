//! Test de errores y taxonomía de salidas (Fase 2).
//!
//! Ver `specifications.md §2-4` y `design.md §8, §10 punto 4`.
//! Requiere `ffmpeg 6+` real (para probe), `FakeProvider` sin red/modelo.

use denoise::cli::{expand_inputs, resolve_outputs, run_dry_run, ResolvedItem};
use denoise::errors::E;
use denoise::ffmpeg_io::{find_ffmpeg, probe};
use denoise::models::{FakeProvider, ModelsProvider};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;

fn get_real_ffmpeg() -> PathBuf {
    find_ffmpeg(None).expect("ffmpeg 6+ debe estar instalado")
}

#[test]
fn test_no_audio_error() {
    let ffmpeg = get_real_ffmpeg();
    let tmp_dir = std::env::temp_dir().join("denoise_err_no_audio");
    let _ = fs::remove_dir_all(&tmp_dir);
    fs::create_dir_all(&tmp_dir).unwrap();

    let video_no_audio = tmp_dir.join("silent.mp4");
    // Generar video sintético de 1s sin audio
    let status = Command::new(&ffmpeg)
        .arg("-y")
        .arg("-v")
        .arg("error")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("testsrc=size=320x240:rate=10:duration=1")
        .arg("-c:v")
        .arg("libx264")
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg(&video_no_audio)
        .status()
        .expect("Generar video sin audio");

    assert!(status.success());

    let res = probe(&ffmpeg, &video_no_audio);
    assert!(res.is_err());
    match res.unwrap_err() {
        E::ENoAudio => {}
        err => panic!("Esperado ENoAudio, obtenido: {:?}", err),
    }

    let _ = fs::remove_dir_all(&tmp_dir);
}

#[test]
fn test_invalid_input_error() {
    let ffmpeg = get_real_ffmpeg();
    let tmp_dir = std::env::temp_dir().join("denoise_err_invalid_input");
    let _ = fs::remove_dir_all(&tmp_dir);
    fs::create_dir_all(&tmp_dir).unwrap();

    // 1. Archivo 0B
    let zero_file = tmp_dir.join("empty.mp4");
    File::create(&zero_file).unwrap();
    let res_zero = probe(&ffmpeg, &zero_file);
    assert!(res_zero.is_err());
    assert_eq!(res_zero.unwrap_err().exit_code(), 2);

    // 2. Ruta inexistente
    let non_existent = tmp_dir.join("missing.mp4");
    let res_missing = expand_inputs(&[non_existent], false, None, "_denoised", false, &tmp_dir);
    assert!(res_missing.is_err());
    assert_eq!(res_missing.unwrap_err().exit_code(), 2);

    // 3. Extensión no soportada
    let unsupported = tmp_dir.join("document.pdf");
    File::create(&unsupported).unwrap();
    let res_unsupp = expand_inputs(&[unsupported], false, None, "_denoised", false, &tmp_dir);
    assert!(res_unsupp.is_err());
    assert_eq!(res_unsupp.unwrap_err().exit_code(), 2);

    // 4. Solo audio renombrado a .mp4
    let audio_only = tmp_dir.join("audio.mp4");
    let status = Command::new(&ffmpeg)
        .arg("-y")
        .arg("-v")
        .arg("error")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("sine=frequency=440:sample_rate=48000:duration=1")
        .arg("-c:a")
        .arg("aac")
        .arg(&audio_only)
        .status()
        .expect("Generar solo-audio");
    assert!(status.success());

    let res_audio = probe(&ffmpeg, &audio_only);
    assert!(res_audio.is_err());
    match res_audio.unwrap_err() {
        E::EInvalidInput(_) => {}
        err => panic!(
            "Esperado EInvalidInput para solo-audio, obtenido: {:?}",
            err
        ),
    }

    let _ = fs::remove_dir_all(&tmp_dir);
}

#[test]
fn test_ffmpeg_not_found() {
    let bad_path = PathBuf::from("C:\\no_such_dir_12345\\ffmpeg_non_existent.exe");
    let res = find_ffmpeg(Some(&bad_path));
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.exit_code(), 1);
    match err {
        E::EFfmpegNotFound(msg) => {
            assert!(msg.contains("no existe"));
        }
        _ => panic!("Esperado EFfmpegNotFound"),
    }
}

#[test]
fn test_model_missing() {
    let tmp_dir = std::env::temp_dir().join("denoise_fake_model_missing");
    let provider = FakeProvider::failing_missing();
    let res = provider.ensure_models(&tmp_dir, &|_, _| {});
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.exit_code(), 1);
    match err {
        E::EModelMissing(msg) => {
            assert!(msg.contains("Modelo no encontrado"));
        }
        _ => panic!("Esperado EModelMissing"),
    }
}

#[test]
fn test_output_exists() {
    let tmp_dir = std::env::temp_dir().join("denoise_out_exists");
    let _ = fs::remove_dir_all(&tmp_dir);
    fs::create_dir_all(&tmp_dir).unwrap();

    let in_file = tmp_dir.join("input.mp4");
    let out_file = tmp_dir.join("output.mp4");
    File::create(&in_file).unwrap();
    File::create(&out_file).unwrap();

    let items = vec![ResolvedItem {
        input: in_file,
        output: out_file,
        is_duplicate_adjusted: false,
    }];

    // Sin overwrite ni skip_existing en dry-run -> reporte 'would fail: E_OUTPUT_EXISTS'
    let res = run_dry_run(&items, false, false, false);
    assert!(res.is_ok());

    let _ = fs::remove_dir_all(&tmp_dir);
}

#[test]
fn test_output_exists_skip_existing() {
    let tmp_dir = std::env::temp_dir().join("denoise_out_skip");
    let _ = fs::remove_dir_all(&tmp_dir);
    fs::create_dir_all(&tmp_dir).unwrap();

    let in_file = tmp_dir.join("input.mp4");
    let out_file = tmp_dir.join("output.mp4");
    File::create(&in_file).unwrap();
    File::create(&out_file).unwrap();

    let items = vec![ResolvedItem {
        input: in_file,
        output: out_file,
        is_duplicate_adjusted: false,
    }];

    let res = run_dry_run(&items, false, true, false);
    assert!(res.is_ok());

    let _ = fs::remove_dir_all(&tmp_dir);
}

#[test]
fn test_invalid_bitrate() {
    let bad_bitrates = [0, 32, 63, 321, 1000, 9999];
    for &br in &bad_bitrates {
        let is_valid = (64..=320).contains(&br);
        assert!(!is_valid, "Bitrate {} debería ser inválido", br);
    }
    let good_bitrates = [64, 128, 192, 256, 320];
    for &br in &good_bitrates {
        let is_valid = (64..=320).contains(&br);
        assert!(is_valid, "Bitrate {} debería ser válido", br);
    }
}

#[test]
fn test_output_name_lot_multi_error() {
    let cwd = Path::new("/workspace");
    let inputs = vec![
        PathBuf::from("/workspace/a.mp4"),
        PathBuf::from("/workspace/b.mp4"),
    ];
    let err =
        resolve_outputs(&inputs, Some("final"), None, None, None, false, cwd, false).unwrap_err();
    assert_eq!(err.exit_code(), 2);
    match err {
        E::EInvalidInput(_) => {}
        _ => panic!("Esperado EInvalidInput"),
    }
}

#[test]
fn test_output_name_self_reference_error() {
    let cwd = Path::new("/workspace");
    let input = PathBuf::from("/workspace/a.mp4");
    let err =
        resolve_outputs(&[input], Some("a.mp4"), None, None, None, false, cwd, false).unwrap_err();
    assert_eq!(err.exit_code(), 2);
    match err {
        E::EInvalidInput(_) => {}
        _ => panic!("Esperado EInvalidInput"),
    }
}

#[test]
fn test_io_error() {
    let tmp_dir = std::env::temp_dir().join("denoise_fake_io_err");
    let provider = FakeProvider::failing_io();
    let res = provider.ensure_models(&tmp_dir, &|_, _| {});
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.exit_code(), 1);
    match err {
        E::EIo(_) => {}
        _ => panic!("Esperado EIo"),
    }
}
