//! Generador de vectores de test deterministas para `test_golden` (RNF-04).
//!
//! Ejecutar con `cargo run --example gen_vectors`.
//! Nunca se ejecuta en `cargo test` ni CI (preserva el congelado D14).
//!
//! Ver `docs/design.md §6` y `docs/specifications.md RNF-04`.

use denoise::df::denoise_wav;
use hound::{WavSpec, WavWriter};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::f32::consts::PI;
use std::fs;
use std::path::Path;

const SR: u32 = 48000;
const FREQ: f32 = 440.0;
const AMPLITUDE: f32 = 0.5;
const SNR_DB: f32 = 10.0;

fn box_muller(rng: &mut StdRng) -> f32 {
    let u1: f32 = rng.gen::<f32>().max(1e-7);
    let u2: f32 = rng.gen::<f32>();
    (-2.0 * u1.ln()).sqrt() * (2.0 * PI * u2).cos()
}

fn generate_pair(
    duration_secs: usize,
    seed: u64,
    voz_path: &Path,
    mezcla_path: &Path,
) -> anyhow::Result<()> {
    let num_samples = duration_secs * (SR as usize);
    let mut rng = StdRng::seed_from_u64(seed);

    let mut clean_samples_i16 = Vec::with_capacity(num_samples);
    let mut mix_samples_i16 = Vec::with_capacity(num_samples);

    // Potencia del seno: A^2 / 2 = 0.5^2 / 2 = 0.125
    // Para SNR = 10dB -> Potencia de ruido = 0.0125 -> sigma = sqrt(0.0125)
    let noise_sigma = (0.125_f32 / 10.0_f32.powf(SNR_DB / 10.0)).sqrt();

    for i in 0..num_samples {
        let t = i as f32 / (SR as f32);
        let clean = AMPLITUDE * (2.0 * PI * FREQ * t).sin();
        let noise = box_muller(&mut rng) * noise_sigma;
        let mix = (clean + noise).clamp(-1.0, 1.0);

        clean_samples_i16.push((clean * 32767.0).round() as i16);
        mix_samples_i16.push((mix * 32767.0).round() as i16);
    }

    let spec = WavSpec {
        channels: 1,
        sample_rate: SR,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    write_wav(voz_path, spec, &clean_samples_i16)?;
    write_wav(mezcla_path, spec, &mix_samples_i16)?;

    Ok(())
}

fn write_wav(path: &Path, spec: WavSpec, samples: &[i16]) -> anyhow::Result<()> {
    let mut writer = WavWriter::create(path, spec)?;
    for &s in samples {
        writer.write_sample(s)?;
    }
    writer.finalize()?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let data_dir = Path::new("tests/data");
    fs::create_dir_all(data_dir)?;

    println!("Generando vectores deterministas en {:?}...", data_dir);

    // Par corto 3s (seed 0)
    let voz_3s = data_dir.join("voz.wav");
    let mezcla_3s = data_dir.join("mezcla10dB.wav");
    let ref_3s = data_dir.join("referencia_dfn3.wav");

    generate_pair(3, 0, &voz_3s, &mezcla_3s)?;
    println!("  -> Generado voz.wav y mezcla10dB.wav (3s)");

    println!("  -> Procesando referencia_dfn3.wav...");
    denoise_wav(&mezcla_3s, &ref_3s, |cur, tot| {
        print!("\r     Progreso 3s: {:.1}s / {:.1}s", cur, tot);
    })?;
    println!("\n  -> referencia_dfn3.wav creada.");

    let clean_r = hound::WavReader::open(&voz_3s)?;
    let clean_s: Vec<i16> = clean_r.into_samples().map(|s| s.unwrap()).collect();
    let out_r = hound::WavReader::open(&ref_3s)?;
    let out_s: Vec<i16> = out_r.into_samples().map(|s| s.unwrap()).collect();

    let mut dot = 0.0_f64;
    let mut norm_c = 0.0_f64;
    let mut norm_o = 0.0_f64;
    for (&c, &o) in clean_s.iter().zip(&out_s) {
        let cf = c as f64;
        let of = o as f64;
        dot += cf * of;
        norm_c += cf * cf;
        norm_o += of * of;
    }
    let norm_corr = dot / (norm_c.sqrt() * norm_o.sqrt());
    println!("Normalized correlation <clean, out>: {:.4}", norm_corr);
    println!(
        "Norm clean: {:.0}, Norm out: {:.0}, Dot: {:.0}",
        norm_c.sqrt(),
        norm_o.sqrt(),
        dot
    );

    // Calcular cross-correlation para encontrar el retardo exacto
    let mut best_lag = 0;
    let mut best_corr = f64::NEG_INFINITY;
    for lag in -500..500 {
        let mut corr = 0.0_f64;
        let mut count = 0;
        for i in 1000..2000 {
            let j = (i as isize + lag) as usize;
            if j < clean_s.len() && j < out_s.len() {
                corr += (clean_s[i] as f64) * (out_s[j] as f64);
                count += 1;
            }
        }
        if count > 0 {
            corr /= count as f64;
            if corr > best_corr {
                best_corr = corr;
                best_lag = lag;
            }
        }
    }
    println!(
        "Best correlation lag: {} samples (corr: {:.0})",
        best_lag, best_corr
    );

    // Par largo 65s (seed 1)
    let voz_65s = data_dir.join("voz65s.wav");
    let mezcla_65s = data_dir.join("mezcla65s10dB.wav");
    let ref_65s = data_dir.join("referencia65s_dfn3.wav");

    generate_pair(65, 1, &voz_65s, &mezcla_65s)?;
    println!("  -> Generado voz65s.wav y mezcla65s10dB.wav (65s)");

    println!("  -> Procesando referencia65s_dfn3.wav...");
    denoise_wav(&mezcla_65s, &ref_65s, |cur, tot| {
        print!("\r     Progreso 65s: {:.1}s / {:.1}s", cur, tot);
    })?;
    println!("\n  -> referencia65s_dfn3.wav creada.");

    println!("¡Vectores generados exitosamente!");
    Ok(())
}
