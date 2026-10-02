//! Test golden `#[ignore]` — verificación SI-SDR con vectores PCM16 deterministas.
//!
//! Ver `specifications.md RNF-04` y `design.md §6`.
//! Requiere modelo DPDFNet descargado + `tests/data/*.wav` generados.
//! Ejecutar con `cargo test --release -- --ignored golden`.

#[path = "common/si_sdr.rs"]
mod si_sdr;

use denoise::df::denoise_wav;
use hound::WavReader;
use std::fs;
use std::path::Path;

fn read_wav_f32(path: &Path) -> Vec<f32> {
    let mut reader = WavReader::open(path).expect("Abrir archivo WAV de test");
    reader
        .samples::<i16>()
        .map(|s| (s.expect("Muestra WAV válida") as f32) / 32768.0)
        .collect()
}

#[test]
#[ignore]
fn test_golden_3s() {
    let data_dir = Path::new("tests/data");
    let voz_path = data_dir.join("voz_clean.wav");
    let mezcla_path = data_dir.join("voz_noisy.wav");
    let ref_path = data_dir.join("referencia_dpdfnet.wav");
    let out_path = std::env::temp_dir().join("golden_3s_out.wav");

    assert!(
        voz_path.exists(),
        "tests/data/voz_clean.wav debe existir (ejecutar 'cargo run --example gen_vectors')"
    );
    assert!(
        mezcla_path.exists(),
        "tests/data/voz_noisy.wav debe existir"
    );
    assert!(
        ref_path.exists(),
        "tests/data/referencia_dpdfnet.wav debe existir"
    );

    denoise_wav(&mezcla_path, &out_path, |cur, tot| {
        eprintln!("test_golden_3s avance: {:.1}s / {:.1}s", cur, tot);
    })
    .expect("Ejecución denoise_wav 3s");

    let clean = read_wav_f32(&voz_path);
    let mix = read_wav_f32(&mezcla_path);
    let denoised = read_wav_f32(&out_path);
    let reference = read_wav_f32(&ref_path);

    let sdr_mix = si_sdr::si_sdr(&clean, &mix);
    let sdr_denoised = si_sdr::si_sdr(&clean, &denoised);
    let sdr_parity = si_sdr::si_sdr(&reference, &denoised);

    let improvement = sdr_denoised - sdr_mix;
    eprintln!(
        "Par 3s: SI-SDR mezcla = {:.2} dB, SI-SDR limpio = {:.2} dB, Mejora = {:.2} dB (mín 5.0 dB)",
        sdr_mix, sdr_denoised, improvement
    );
    eprintln!(
        "Par 3s: Paridad vs referencia = {:.2} dB (mín 60.0 dB)",
        sdr_parity
    );

    assert!(
        improvement >= 5.0,
        "Mejora SI-SDR {:.2} dB debe ser >= 5.0 dB",
        improvement
    );
    assert!(
        sdr_parity >= 60.0,
        "Paridad SI-SDR {:.2} dB debe ser >= 60.0 dB",
        sdr_parity
    );

    let _ = fs::remove_file(&out_path);
}

#[test]
#[ignore]
fn test_golden_65s() {
    let data_dir = Path::new("tests/data");
    let voz_path = data_dir.join("voz65s_clean.wav");
    let mezcla_path = data_dir.join("voz65s_noisy.wav");
    let ref_path = data_dir.join("referencia65s_dpdfnet.wav");
    let out_path = std::env::temp_dir().join("golden_65s_out.wav");

    assert!(
        voz_path.exists(),
        "tests/data/voz65s_clean.wav debe existir (ejecutar 'cargo run --example gen_vectors')"
    );
    assert!(
        mezcla_path.exists(),
        "tests/data/voz65s_noisy.wav debe existir"
    );
    assert!(
        ref_path.exists(),
        "tests/data/referencia65s_dpdfnet.wav debe existir"
    );

    denoise_wav(&mezcla_path, &out_path, |cur, tot| {
        eprintln!("test_golden_65s avance: {:.1}s / {:.1}s", cur, tot);
    })
    .expect("Ejecución denoise_wav 65s");

    let clean = read_wav_f32(&voz_path);
    let mix = read_wav_f32(&mezcla_path);
    let denoised = read_wav_f32(&out_path);
    let reference = read_wav_f32(&ref_path);

    let sdr_mix = si_sdr::si_sdr(&clean, &mix);
    let sdr_denoised = si_sdr::si_sdr(&clean, &denoised);
    let sdr_parity = si_sdr::si_sdr(&reference, &denoised);

    let improvement = sdr_denoised - sdr_mix;
    eprintln!(
        "Par 65s: SI-SDR mezcla = {:.2} dB, SI-SDR limpio = {:.2} dB, Mejora = {:.2} dB (informativo: este par no asserta mejora, solo paridad)",
        sdr_mix, sdr_denoised, improvement
    );
    eprintln!(
        "Par 65s: Paridad vs referencia = {:.2} dB (mín 60.0 dB)",
        sdr_parity
    );

    // Par 65s verifica estabilidad en clips largos (paridad >= 60.0 dB)
    assert!(
        sdr_parity >= 60.0,
        "Paridad SI-SDR {:.2} dB debe ser >= 60.0 dB",
        sdr_parity
    );

    let _ = fs::remove_file(&out_path);
}
