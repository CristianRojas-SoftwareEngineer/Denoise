//! Harness de benchmark: procesa pares (ruido -> salida) con el DSP de producción.
//!
//! El CLI `denoise` solo acepta vídeo, así que para medir calidad de audio hay
//! que invocar `denoise_wav` directamente, que es exactamente la ruta que usa
//! `denoise` por dentro. Así el benchmark midie el DSP y no el contenedor.
//!
//! Uso:
//! ```text
//! cargo run --release --example process_wav -- <pares.txt> [model_dir]
//! ```
//!
//! `<pares.txt>` tiene una línea por clip, `entrada<TAB>salida`. Se leen todos
//! los clips en un mismo proceso para amortizar la carga del modelo ONNX.
//!
//! Por cada clip exitoso emite a stdout `RTF<TAB>salida<TAB>valor` (segundos de
//! audio por segundo de pared; <1 es más rápido que tiempo real) para que
//! `tools/quality/benchmark.py` lo registre. El progreso humano va a stderr.
//!
//! Nota: el modelo ONNX se carga una sola vez por proceso, así que el RTF del
//! primer clip incluye la carga y sale más alto; los siguientes miden DSP puro.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use denoise::df::denoise_wav_with_provider;
use denoise::models::{default_model_dir, DefaultModelsProvider};

/// Duración en segundos de un WAV (para RTF). `None` si no se puede leer.
fn wav_seconds(path: &PathBuf) -> Option<f64> {
    let r = hound::WavReader::open(path).ok()?;
    let spec = r.spec();
    if spec.sample_rate == 0 {
        return None;
    }
    Some(r.len() as f64 / spec.sample_rate as f64)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("uso: process_wav <pares.txt> [model_dir]");
        eprintln!("  pares.txt: una línea por clip, `entrada<TAB>salida`");
        return ExitCode::from(2);
    }

    let pairs_path = PathBuf::from(&args[1]);
    let model_dir: PathBuf = args
        .get(2)
        .map(PathBuf::from)
        .unwrap_or_else(default_model_dir);
    let provider = DefaultModelsProvider::new();

    let spec = match std::fs::read_to_string(&pairs_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("no se pudo leer {}: {}", pairs_path.display(), e);
            return ExitCode::from(2);
        }
    };

    let mut failed_count = 0u32;
    let mut processed_count = 0u32;

    for (i, line) in spec.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((input_path, output_path)) = line.split_once('\t') else {
            eprintln!("línea {} sin tabulador: {}", i + 1, line);
            failed_count += 1;
            continue;
        };

        let input_path = PathBuf::from(input_path.trim());
        let output_path = PathBuf::from(output_path.trim());
        if let Some(parent) = output_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        eprint!("[{}] {}", input_path.display(), input_path.display());
        let t0 = Instant::now();
        match denoise_wav_with_provider(&input_path, &output_path, &model_dir, &provider, &|cur, tot| {
            eprint!("\r[{}] {cur:.1}/{tot:.1}s", input_path.display());
        }) {
            Ok(()) => {
                eprintln!("\r[{}] ok -> {}", input_path.display(), output_path.display());
                // Línea máquina a stdout para benchmark.py; el progreso va a stderr.
                let secs = t0.elapsed().as_secs_f64().max(1e-6);
                match wav_seconds(&output_path) {
                    Some(dur) => println!("RTF\t{}\t{:.4}", output_path.display(), dur / secs),
                    None => println!("RTF\t{}\tnan", output_path.display()),
                }
                processed_count += 1;
            }
            Err(e) => {
                eprintln!("\r[{}] ERROR: {}", input_path.display(), e);
                failed_count += 1;
            }
        }
    }

    eprintln!("procesados={processed_count} fallos={failed_count}");
    if failed_count > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
