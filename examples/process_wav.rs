//! Harness de benchmark: procesa pares (ruido -> salida) con el DSP de producción.
//!
//! El CLI `denoise` solo acepta vídeo, así que para medir calidad de audio hay
//! que invocar `denoise_wav` directamente, que es exactamente la ruta que usa
//! `denoise` por dentro. Así el benchmark midie el DSP y no el contenedor.
//!
//! Uso:
//! ```text
//! cargo run --release --example process_wav -- <pares.txt> [model_dir] [flags...]
//! ```
//!
//! `<pares.txt>` tiene una línea por clip, `entrada<TAB>salida`. Se leen todos
//! los clips en un mismo proceso para amortizar la carga del modelo ONNX.
//!
//! Flags opcionales (inyectadas por `benchmark.py --profile`):
//!   --highpass           Activar filtro pasa-altos a 70 Hz
//!   --dry-blend <f32>    Fracción de mezcla con audio original (0.0–1.0)
//!   --pause-gate-db <f64> Nivel de atenuación en pausas en dB (ej: -25, -30)
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

use denoise::df::{denoise_wav_with_params, GATE_CRUSH_DB};
use denoise::models::{default_model_dir, DefaultModelsProvider};

/// Parámetros parseados de la línea de comandos.
struct HarnessArgs {
    pairs_path: PathBuf,
    model_dir: PathBuf,
    highpass: bool,
    dry_blend: f32,
    pause_gate_db: f64,
}

/// Parsea argumentos posicionales (`pares.txt`, `model_dir`) y flags opcionales
/// (`--highpass`, `--dry-blend`, `--pause-gate-db`). No usa `clap` para evitar
/// dependencias extra en el example.
fn parse_args(args: &[String]) -> Result<HarnessArgs, String> {
    let mut positionals: Vec<&str> = Vec::new();
    let mut highpass = false;
    let mut dry_blend: f32 = 0.0;
    let mut pause_gate_db: f64 = GATE_CRUSH_DB;

    let mut i = 1; // saltar argv[0]
    while i < args.len() {
        match args[i].as_str() {
            "--highpass" => {
                highpass = true;
            }
            "--dry-blend" => {
                i += 1;
                dry_blend = args
                    .get(i)
                    .ok_or("--dry-blend requiere un valor")?
                    .parse::<f32>()
                    .map_err(|e| format!("--dry-blend inválido: {e}"))?;
            }
            "--pause-gate-db" => {
                i += 1;
                pause_gate_db = args
                    .get(i)
                    .ok_or("--pause-gate-db requiere un valor")?
                    .parse::<f64>()
                    .map_err(|e| format!("--pause-gate-db inválido: {e}"))?;
            }
            other if other.starts_with("--") => {
                return Err(format!("flag desconocida: {other}"));
            }
            _ => {
                positionals.push(&args[i]);
            }
        }
        i += 1;
    }

    if positionals.is_empty() {
        return Err("falta <pares.txt>".to_string());
    }

    let pairs_path = PathBuf::from(positionals[0]);
    let model_dir = positionals
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(default_model_dir);

    Ok(HarnessArgs {
        pairs_path,
        model_dir,
        highpass,
        dry_blend,
        pause_gate_db,
    })
}

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
    let harness = match parse_args(&args) {
        Ok(h) => h,
        Err(msg) => {
            eprintln!("error: {msg}");
            eprintln!("uso: process_wav <pares.txt> [model_dir] [--highpass] [--dry-blend F] [--pause-gate-db F]");
            eprintln!("  pares.txt: una línea por clip, `entrada<TAB>salida`");
            return ExitCode::from(2);
        }
    };

    // Trazabilidad: imprimir flags activas a stderr.
    eprintln!(
        "perfil: highpass={} dry_blend={:.3} pause_gate_db={:.1}",
        harness.highpass, harness.dry_blend, harness.pause_gate_db
    );

    let provider = DefaultModelsProvider::new();

    let spec = match std::fs::read_to_string(&harness.pairs_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "no se pudo leer {}: {}",
                harness.pairs_path.display(),
                e
            );
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
        match denoise_wav_with_params(
            &input_path,
            &output_path,
            &harness.model_dir,
            &provider,
            harness.dry_blend,
            harness.pause_gate_db,
            harness.highpass,
            &|cur, tot| {
                eprint!("\r[{}] {cur:.1}/{tot:.1}s", input_path.display());
            },
        ) {
            Ok(()) => {
                eprintln!(
                    "\r[{}] ok -> {}",
                    input_path.display(),
                    output_path.display()
                );
                // Línea máquina a stdout para benchmark.py; el progreso va a stderr.
                let secs = t0.elapsed().as_secs_f64().max(1e-6);
                match wav_seconds(&output_path) {
                    Some(dur) => {
                        println!("RTF\t{}\t{:.4}", output_path.display(), dur / secs)
                    }
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
