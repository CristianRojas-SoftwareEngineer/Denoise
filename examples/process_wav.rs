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

use std::path::PathBuf;
use std::process::ExitCode;

use denoise::df::denoise_wav_with_provider;
use denoise::models::{default_model_dir, DefaultModelsProvider};

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

    let mut fallos = 0u32;
    let mut hechos = 0u32;

    for (i, linea) in spec.lines().enumerate() {
        let linea = linea.trim();
        if linea.is_empty() || linea.starts_with('#') {
            continue;
        }
        let Some((entrada, salida)) = linea.split_once('\t') else {
            eprintln!("línea {} sin tabulador: {}", i + 1, linea);
            fallos += 1;
            continue;
        };

        let entrada = PathBuf::from(entrada.trim());
        let salida = PathBuf::from(salida.trim());
        if let Some(parent) = salida.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        eprint!("[{}] {}", entrada.display(), entrada.display());
        match denoise_wav_with_provider(&entrada, &salida, &model_dir, &provider, &|cur, tot| {
            eprint!("\r[{}] {cur:.1}/{tot:.1}s", entrada.display());
        }) {
            Ok(()) => {
                eprintln!("\r[{}] ok -> {}", entrada.display(), salida.display());
                hechos += 1;
            }
            Err(e) => {
                eprintln!("\r[{}] ERROR: {}", entrada.display(), e);
                fallos += 1;
            }
        }
    }

    eprintln!("procesados={hechos} fallos={fallos}");
    if fallos > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}