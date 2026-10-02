//! Generador de vectores de test deterministas para `test_golden` (RNF-04).
//!
//! Ejecutar con `cargo run --example gen_vectors`.
//! Nunca se ejecuta en `cargo test` ni CI (preserva el congelado).
//!
//! Ver `docs/design.md §6` y `docs/specifications.md RNF-04`.
//!
//! Los vectores usan voz real extraída del conjunto de evaluación de DPDFNet
//! (`Ceva-IP/DPDFNet_EvalSet`, Apache 2.0). Un tono sintético no sirve: DPDFNet
//! lo clasifica como tono tonal y lo suprime, con lo que la aserción de mejora
//! de SI-SDR mediría supresión de tono en lugar de mejora de voz.

use denoise::df::denoise_wav;
use hound::{WavReader, WavSpec, WavWriter};
use std::fs;
use std::path::{Path, PathBuf};

/// Longitud de los clips en muestras a 48 kHz.
const SHORT_SECS: usize = 3;
const LONG_SECS: usize = 65;

const SR: u32 = 48000;

/// Lee un WAV PCM16 mono o estéreo y lo devuelve como mono f32 [-1, 1] a 48 kHz,
/// remuestreando con interpolación lineal si el original no está a 48 kHz.
fn read_wav_mono_f32(path: &Path) -> anyhow::Result<Vec<f32>> {
    let mut reader = WavReader::open(path)?;
    let spec = reader.spec();
    let src_sr = spec.sample_rate;
    let ch = spec.channels.max(1) as usize;
    let raw: Vec<i16> = reader.samples::<i16>().collect::<Result<_, _>>()?;
    let frames = raw.len() / ch;

    // Downmix a mono en el dominio original.
    let mut mono = Vec::with_capacity(frames);
    for f in 0..frames {
        let mut acc = 0.0_f32;
        for c in 0..ch {
            acc += raw[f * ch + c] as f32;
        }
        mono.push(acc / ch as f32 / 32768.0);
    }

    if src_sr == SR {
        return Ok(mono);
    }

    // Remuestreo lineal a 48 kHz para que los vectores congelados estén en la
    // misma tasa que exige el pipeline.
    let out_len = (mono.len() as u64 * SR as u64 / src_sr as u64) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = i as f64 * src_sr as f64 / SR as f64;
        let i0 = pos.floor() as usize;
        let frac = (pos - i0 as f64) as f32;
        let a = mono.get(i0).copied().unwrap_or(0.0);
        let b = mono.get(i0 + 1).copied().unwrap_or(a);
        out.push(a + (b - a) * frac);
    }
    Ok(out)
}

/// Aplica una mezcla ruido+señal con la SNR objetivo, partiendo de `noise`.
fn mix_at_snr(clean: &[f32], noise: &[f32], snr_db: f32) -> anyhow::Result<Vec<i16>> {
    let n = clean.len().min(noise.len());
    let sig_p: f64 = clean[..n].iter().map(|&v| (v as f64) * (v as f64)).sum();
    let noi_p: f64 = noise[..n].iter().map(|&v| (v as f64) * (v as f64)).sum();
    let g = (sig_p / (noi_p * 10f64.powf(snr_db as f64 / 10.0))).sqrt();
    Ok((0..n)
        .map(|i| {
            ((clean[i] as f64 + g * noise[i] as f64).clamp(-1.0, 1.0) * 32767.0).round() as i16
        })
        .collect())
}

fn write_wav(path: &Path, samples: &[i16]) -> anyhow::Result<()> {
    let mut writer = WavWriter::create(
        path,
        WavSpec {
            channels: 1,
            sample_rate: SR,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )?;
    for &s in samples {
        writer.write_sample(s)?;
    }
    writer.finalize()?;
    Ok(())
}

fn to_i16(samples: &[f32]) -> Vec<i16> {
    samples
        .iter()
        .map(|&v| (v.clamp(-1.0, 1.0) * 32767.0).round() as i16)
        .collect()
}

/// Construye un par (voz limpia, voz+ruido) a partir de las fuentes del
/// conjunto de evaluación y escribe `*_clean.wav` y `*_noisy.wav`.
fn build_pair(
    data_dir: &Path,
    tag: &str,
    secs: usize,
    clean_src: &Path,
    noise_src: &Path,
    snr_db: f32,
) -> anyhow::Result<(std::path::PathBuf, std::path::PathBuf)> {
    let n = secs * SR as usize;
    let clean = read_wav_mono_f32(clean_src)?;
    let noise = read_wav_mono_f32(noise_src)?;

    if clean.len() < n {
        anyhow::bail!(
            "el clip fuente tiene {:.1}s, menos que los {}s solicitados",
            clean.len() as f64 / SR as f64,
            secs
        );
    }

    // Elegimos la ventana de `secs` con mayor energia de voz, para no caer en
    // tramos silenciosos que darian vectores degenerados.
    let windows = clean.len() - n + 1;
    let stride = 480.max(windows / 200);
    let mut best_start = 0usize;
    let mut best_energy = f64::NEG_INFINITY;
    let mut s = 0usize;
    while s < windows {
        let e: f64 = clean[s..s + n]
            .iter()
            .map(|&v| (v as f64) * (v as f64))
            .sum();
        if e > best_energy {
            best_energy = e;
            best_start = s;
        }
        s += stride;
    }
    let clean_slice = clean[best_start..best_start + n].to_vec();

    // El ruido se toma de la misma posicion relativa en su clip.
    let noise_start = (best_start / clean.len() * noise.len()).min(noise.len().saturating_sub(n));
    let noise_slice = noise[noise_start..(noise_start + n).min(noise.len())].to_vec();

    let clean_path = data_dir.join(format!("{}_clean.wav", tag));
    let noisy_path = data_dir.join(format!("{}_noisy.wav", tag));

    write_wav(&clean_path, &to_i16(&clean_slice))?;
    write_wav(
        &noisy_path,
        &mix_at_snr(&clean_slice, &noise_slice, snr_db)?,
    )?;
    Ok((clean_path, noisy_path))
}

fn main() -> anyhow::Result<()> {
    let data_dir = Path::new("tests/data");
    fs::create_dir_all(data_dir)?;

    // Directorio del conjunto de evaluación de DPDFNet (Ceva-IP/DPDFNet_EvalSet,
    // Apache 2.0) descargado como herramienta de un solo uso. Los vectores
    // resultantes quedan congelados en tests/data y no lo requieren en runtime.
    let eval_dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../bench/eval"));

    let clean_short = eval_dir.join("Clean/spanish_street_snr10_rt60none_0_mixture_clean.wav");
    let noisy_short = eval_dir.join("Noisy/spanish_street_snr0_rt60none_0_mixture_noisy.wav");

    if !clean_short.exists() || !noisy_short.exists() {
        anyhow::bail!(
 "Faltan las fuentes de voz real en {}. Uso: cargo run --example gen_vectors -- <dir_eval>",
 eval_dir.display()
 );
    }

    println!("Generando vectores deterministas en {:?}...", data_dir);

    // Par corto 3s a SNR 0 dB: el test golden exige mejora >= 5 dB, que con
    // una mezcla suave a 10 dB no se alcanza (el modelo ya parte de 10 dB).
    let (_voz_3s, mezcla_3s) =
        build_pair(data_dir, "voz", SHORT_SECS, &clean_short, &noisy_short, 0.0)?;
    println!(
        " -> Generado voz_clean.wav y voz_noisy.wav ({})",
        SHORT_SECS
    );
    let ref_3s = data_dir.join("referencia_dpdfnet.wav");
    println!(" -> Procesando referencia_dpdfnet.wav...");
    denoise_wav(&mezcla_3s, &ref_3s, |cur, tot| {
        print!("\r Progreso {}s: {:.1}s / {:.1}s", SHORT_SECS, cur, tot);
    })?;
    println!("\n -> referencia_dpdfnet.wav creada.");

    // Par largo 65s, sobre un clip fuente mas largo que la ventana pedida.
    let (_voz_65s, mezcla_65s) = build_pair(
        data_dir,
        "voz65s",
        LONG_SECS,
        &eval_dir.join("Clean/spanish_subway_snr10_rt60none_0_mixture_clean.wav"),
        &eval_dir.join("Noisy/spanish_subway_snr10_rt60none_0_mixture_noisy.wav"),
        10.0,
    )?;
    println!(
        " -> Generado voz65s_clean.wav y voz65s_noisy.wav ({})",
        LONG_SECS
    );

    let ref_65s = data_dir.join("referencia65s_dpdfnet.wav");
    println!(" -> Procesando referencia65s_dpdfnet.wav...");
    denoise_wav(&mezcla_65s, &ref_65s, |cur, tot| {
        print!("\r Progreso {}s: {:.1}s / {:.1}s", LONG_SECS, cur, tot);
    })?;
    println!("\n -> referencia65s_dpdfnet.wav creada.");

    println!("¡Vectores generados exitosamente!");
    Ok(())
}
