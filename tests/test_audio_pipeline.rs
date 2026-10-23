//! Test E2E `#[ignore]` — pipeline completo sobre un archivo de audio real.
//!
//! Ver `specifications.md RF-01..05` y `design.md §5`.
//! Requiere `ffmpeg 6+` y el modelo DPDFNet (se descarga en la primera corrida).
//! Ejecutar con `cargo test --release --test test_audio_pipeline -- --ignored`.

use denoise::ffmpeg_io::{find_ffmpeg, probe, verify_output_lightweight, MediaKind};
use denoise::models::{default_model_dir, DefaultModelsProvider};
use denoise::pipeline::{clean_media, PipelineParams, ProcessStatus};
use hound::WavReader;
use std::cell::RefCell;
use std::fs;
use std::path::Path;

#[test]
#[ignore]
fn test_audio_wav_pipeline_e2e() {
    let ffmpeg = find_ffmpeg(None).expect("ffmpeg 6+ debe estar instalado");
    let input = Path::new("tests/data/voz_noisy.wav");
    assert!(
        input.exists(),
        "tests/data/voz_noisy.wav debe existir (ver tests/data/README.md)"
    );

    let tmp_dir = std::env::temp_dir().join("denoise_audio_e2e");
    let _ = fs::remove_dir_all(&tmp_dir);
    fs::create_dir_all(&tmp_dir).expect("Crear directorio temporal del test");
    let output = tmp_dir.join("voz_denoised.wav");

    let probe_in = probe(&ffmpeg, input).expect("Probe del WAV de entrada");
    assert!(!probe_in.has_video);
    assert!(probe_in.has_audio);
    assert_eq!(probe_in.media_kind, MediaKind::Audio);

    let model_dir = default_model_dir();
    let provider = DefaultModelsProvider::new();
    let params = PipelineParams {
        ffmpeg: &ffmpeg,
        input,
        output: &output,
        audio_bitrate: 192,
        dry_blend: 0.0,
        pause_gate_db: -25.0,
        highpass: false,
        stereo: false,
        target_lufs: None,
        model_dir: &model_dir,
        models_provider: &provider,
        overwrite: true,
        skip_existing: false,
        verbose: false,
        cancel_flag: None,
    };

    // `clean_media` recibe un `Fn`: la telemetría del test usa interior mutabilidad.
    let progress = RefCell::new((0u32, Vec::<String>::new()));
    let status = clean_media(&params, |pct, msg| {
        let mut state = progress.borrow_mut();
        state.0 = pct;
        state.1.push(msg.to_string());
    })
    .expect("El pipeline de audio debe completarse");
    let (last_pct, messages) = progress.into_inner();

    assert_eq!(status, ProcessStatus::Ok);
    assert_eq!(last_pct, 100, "El progreso debe alcanzar 100%");
    assert!(
        messages.iter().any(|m| m == "empaquetando audio"),
        "El pipeline de audio debe reportar la fase de empaquetado: {:?}",
        messages
    );

    // Salida WAV válida, reproducible y con la duración esperada
    let meta = fs::metadata(&output).expect("La salida WAV debe existir");
    assert!(meta.len() > 0, "La salida WAV no puede estar vacía");

    let mut reader = WavReader::open(&output).expect("La salida debe ser un WAV legible por hound");
    let spec = reader.spec();
    assert_eq!(spec.channels, 1);
    assert_eq!(spec.sample_rate, 48_000);
    assert_eq!(spec.bits_per_sample, 16);

    let out_duration = reader.duration() as f64 / spec.sample_rate as f64;
    assert!(
        (out_duration - probe_in.duration).abs() <= 0.5,
        "Duración de salida {:.3}s debe coincidir con la entrada {:.3}s (±0.5s)",
        out_duration,
        probe_in.duration
    );

    let samples: Vec<i16> = reader
        .samples::<i16>()
        .collect::<Result<Vec<_>, _>>()
        .expect("Leer muestras PCM16 de la salida");
    assert!(!samples.is_empty(), "La salida no puede tener 0 muestras");

    let verified = verify_output_lightweight(&ffmpeg, &output, probe_in.duration, MediaKind::Audio)
        .expect("Verificación ligera de audio");
    assert!(
        verified,
        "La salida de audio debe pasar la verificación ligera"
    );

    // Sin temporales ni parciales huérfanos junto a la salida
    let leftovers: Vec<String> = fs::read_dir(&tmp_dir)
        .expect("Listar directorio de salida")
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".part.") || name.contains(".tmp."))
        .collect();
    assert!(
        leftovers.is_empty(),
        "No deben quedar temporales ni parciales: {:?}",
        leftovers
    );

    let _ = fs::remove_dir_all(&tmp_dir);
}
