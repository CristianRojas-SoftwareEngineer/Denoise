//! Módulo `cli.rs` — parseo clap, expansión de entradas, resolución de salidas y reporte.
//!
//! Contrato CLI autoritativo: `docs/design.md §4`.
//! Especificaciones: `docs/specifications.md §2, §4, §5`.

use crate::errors::E;
use crate::ffmpeg_io::{find_ffmpeg, verify_ffmpeg};
use clap::Parser;
use serde::Serialize;
use std::collections::HashSet;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Extensiones de video soportadas (case-insensitive).
pub const SUPPORTED_EXTENSIONS: &[&str] = &["mp4", "mov", "mkv", "webm", "avi"];

#[derive(Parser, Debug, Clone)]
#[command(
    name = "denoise",
    version = env!("CARGO_PKG_VERSION"),
 about = "CLI autocontenido denoise v1 — elimina ruido de video con DPDFNet ONNX",
    override_usage = "denoise INPUT... [--output-name NAME] [--output-dir DIR] [--prefix STR] [--suffix STR]\n  [--recursive] [--overwrite | --skip-existing]\n  [--audio-bitrate KBPS] [--model-dir DIR] [--ffmpeg-path PATH]\n  [--dry-run] [--json] [--verbose] [--version]",
    disable_version_flag = true
)]
pub struct Cli {
    /// Rutas de entrada (archivos o directorios)
    #[arg(required_unless_present = "version", num_args = 1..)]
    pub input: Vec<PathBuf>,

    /// Nombre del archivo de salida (solo si lote == 1 archivo)
    #[arg(short = 'o', long = "output-name")]
    pub output_name: Option<String>,

    /// Directorio de salida
    #[arg(long = "output-dir")]
    pub output_dir: Option<PathBuf>,

    /// Prefijo para el nombre de salida
    #[arg(long = "prefix")]
    pub prefix: Option<String>,

    /// Sufijo para el nombre de salida (defecto: _denoised)
    #[arg(long = "suffix")]
    pub suffix: Option<String>,

    /// Escanear directorios recursivamente
    #[arg(long = "recursive")]
    pub recursive: bool,

    /// Sobrescribir archivos existentes
    #[arg(long = "overwrite", conflicts_with = "skip_existing")]
    pub overwrite: bool,

    /// Saltar archivos de salida que ya existan
    #[arg(long = "skip-existing", conflicts_with = "overwrite")]
    pub skip_existing: bool,

    /// Bitrate de audio AAC de salida en kbps (64-320, defecto: 192)
    #[arg(long = "audio-bitrate", default_value = "192")]
    pub audio_bitrate: u32,

    /// Directorio para almacenar/buscar modelos ONNX (defecto: ~/.cache/denoise/models)
    #[arg(long = "model-dir")]
    pub model_dir: Option<PathBuf>,

    /// Ruta explícita al binario de ffmpeg
    #[arg(long = "ffmpeg-path")]
    pub ffmpeg_path: Option<PathBuf>,

    /// Simular procesamiento sin modificar disco ni ejecutar IA
    #[arg(long = "dry-run")]
    pub dry_run: bool,

    /// Formato JSON Lines (JSONL) para scripting
    #[arg(long = "json")]
    pub json: bool,

    /// Salida detallada de depuración a stderr
    #[arg(long = "verbose")]
    pub verbose: bool,

    /// Imprimir versión detallada (denoise + modelo DPDFNet + ffmpeg)
    #[arg(short = 'V', long = "version", action = clap::ArgAction::SetTrue)]
    pub version: bool,
}

/// Representa un item planificado para procesar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedItem {
    pub input: PathBuf,
    pub output: PathBuf,
    pub is_duplicate_adjusted: bool,
}

/// Estado de un item tras simulación o ejecución.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ItemStatus {
    Ok,
    Failed,
    Skipped,
    DryRun,
}

/// Línea de log JSONL por archivo.
#[derive(Debug, Clone, Serialize)]
pub struct JsonLogLine {
    pub input: String,
    pub output: String,
    pub status: ItemStatus,
    pub message: String,
    pub pct: u32,
}

/// Resumen final en JSONL y consola.
#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct BatchSummary {
    pub ok: usize,
    pub failed: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct JsonSummaryLine {
    pub summary: BatchSummary,
}

/// Normaliza una ruta lexicalmente contra `cwd` sin tocar disco ni resolver symlinks.
pub fn normalize_lexical(path: &Path, cwd: &Path) -> PathBuf {
    let abs_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };

    let mut components = Vec::new();
    for comp in abs_path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if let Some(Component::Normal(_)) = components.last() {
                    components.pop();
                }
            }
            c => components.push(c),
        }
    }

    let mut result = PathBuf::new();
    for c in components {
        result.push(c.as_os_str());
    }
    result
}

/// Clave de deduplicación para rutas (case-insensitive en Windows, byte-exact en Unix).
pub fn dedup_key(path: &Path) -> String {
    let s = path.to_string_lossy();
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s.into_owned()
    }
}

/// Comprueba si la extensión del archivo es soportada.
pub fn is_supported_video(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            let ext_lower = ext.to_lowercase();
            SUPPORTED_EXTENSIONS.contains(&ext_lower.as_str())
        })
        .unwrap_or(false)
}

/// Valida los caracteres permitidos en prefijos y sufijos: `[A-Za-z0-9._-]` y no `.` ni `..` exactos.
pub fn validate_affix(name: &str, affix_type: &str) -> Result<(), E> {
    if name.is_empty() {
        return Ok(());
    }
    if name == "." || name == ".." {
        return Err(E::EInvalidInput(format!(
            "El {} no puede ser '.' ni '..'",
            affix_type
        )));
    }
    for c in name.chars() {
        if !c.is_ascii_alphanumeric() && c != '.' && c != '_' && c != '-' {
            return Err(E::EInvalidInput(format!(
                "El {} '{}' contiene caracteres no permitidos. Solo se permiten [A-Za-z0-9._-]",
                affix_type, name
            )));
        }
    }
    Ok(())
}

/// Expande las rutas de entrada determinísticamente.
pub fn expand_inputs(
    inputs: &[PathBuf],
    recursive: bool,
    output_dir: Option<&Path>,
    suffix: &str,
    verbose: bool,
    cwd: &Path,
) -> Result<Vec<PathBuf>, E> {
    let mut collected = Vec::new();
    let mut seen = HashSet::new();

    let norm_out_dir = output_dir.map(|od| normalize_lexical(od, cwd));
    let suffix_filter = format!("{}.mp4", suffix.to_lowercase());

    for input in inputs {
        let norm_input = normalize_lexical(input, cwd);

        if norm_input.is_dir() {
            let mut dir_files = Vec::new();
            collect_dir_entries(
                &norm_input,
                recursive,
                norm_out_dir.as_deref(),
                &suffix_filter,
                verbose,
                &mut dir_files,
            )?;

            // Orden determinista byte-wise UTF-8
            dir_files.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));

            for file in dir_files {
                let key = dedup_key(&file);
                if seen.insert(key) {
                    collected.push(file);
                }
            }
        } else if norm_input.is_file() {
            if !is_supported_video(&norm_input) {
                return Err(E::EInvalidInput(format!(
                    "Extensión no soportada para '{}'. Soportadas: {:?}",
                    input.display(),
                    SUPPORTED_EXTENSIONS
                )));
            }
            let key = dedup_key(&norm_input);
            if seen.insert(key) {
                collected.push(norm_input);
            }
        } else {
            return Err(E::EInvalidInput(format!(
                "La ruta de entrada '{}' no existe",
                input.display()
            )));
        }
    }

    if collected.is_empty() {
        return Err(E::EInvalidInput(
            "Lote vacío sin videos soportados".to_string(),
        ));
    }

    // Orden final determinista byte-wise UTF-8
    collected.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));
    Ok(collected)
}

fn collect_dir_entries(
    dir: &Path,
    recursive: bool,
    norm_out_dir: Option<&Path>,
    suffix_filter: &str,
    verbose: bool,
    out: &mut Vec<PathBuf>,
) -> Result<(), E> {
    let read_dir = std::fs::read_dir(dir).map_err(E::EIo)?;

    for entry in read_dir {
        let entry = entry.map_err(E::EIo)?;
        let path = entry.path();

        if path.is_dir() {
            // Excluir output-dir si está anidado
            if let Some(out_d) = norm_out_dir {
                if path == out_d {
                    if verbose {
                        eprintln!(
                            "[verbose] Excluyendo output-dir anidado: {}",
                            path.display()
                        );
                    }
                    continue;
                }
            }
            if recursive {
                collect_dir_entries(&path, recursive, norm_out_dir, suffix_filter, verbose, out)?;
            }
        } else if path.is_file() && is_supported_video(&path) {
            // Excluir *<suffix>.mp4 en escaneos recursivos
            if recursive {
                let fname = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if fname.ends_with(suffix_filter) {
                    if verbose {
                        eprintln!(
                            "[verbose] Excluyendo archivo ya procesado (*{}): {}",
                            suffix_filter,
                            path.display()
                        );
                    }
                    continue;
                }
            }
            out.push(path);
        }
    }
    Ok(())
}

/// Resuelve los paths de salida para un lote expandido según precedencia.
#[allow(clippy::too_many_arguments)]
pub fn resolve_outputs(
    expanded: &[PathBuf],
    output_name: Option<&str>,
    output_dir: Option<&Path>,
    prefix: Option<&str>,
    suffix: Option<&str>,
    recursive: bool,
    cwd: &Path,
    verbose: bool,
) -> Result<Vec<ResolvedItem>, E> {
    if expanded.is_empty() {
        return Err(E::EInvalidInput("Lote vacío".to_string()));
    }

    let pfx = prefix.unwrap_or("");
    let sfx = suffix.unwrap_or("_denoised");

    validate_affix(pfx, "prefijo")?;
    validate_affix(sfx, "sufijo")?;

    // Regla 1: --output-name con lote == 1
    if let Some(name) = output_name {
        if expanded.len() > 1 {
            return Err(E::EInvalidInput(
                "--output-name solo se permite cuando el lote expandido tiene exactamente 1 video"
                    .to_string(),
            ));
        }

        let input = &expanded[0];
        let mut final_name = name.to_string();
        if !final_name.to_lowercase().ends_with(".mp4") {
            final_name.push_str(".mp4");
        }

        let target_dir = match output_dir {
            Some(od) => normalize_lexical(od, cwd),
            None => cwd.to_path_buf(),
        };
        let out_path = target_dir.join(&final_name);

        // si --output-name resuelve a la propia entrada sin prefix/suffix -> E_INVALID_INPUT exit 2 siempre
        if normalize_lexical(&out_path, cwd) == normalize_lexical(input, cwd) {
            return Err(E::EInvalidInput(
                "--output-name resuelve al mismo archivo de entrada".to_string(),
            ));
        }

        return Ok(vec![ResolvedItem {
            input: input.clone(),
            output: out_path,
            is_duplicate_adjusted: false,
        }]);
    }

    // Si ambos prefijo y sufijo son vacíos y la salida es in-place -> E_INVALID_INPUT
    if pfx.is_empty() && sfx.is_empty() && output_dir.is_none() {
        return Err(E::EInvalidInput(
            "Prefijo y sufijo no pueden estar vacíos simultáneamente si la salida es in-place"
                .to_string(),
        ));
    }

    let mut resolved_list = Vec::new();
    let mut used_outputs: HashSet<String> = HashSet::new();

    for input in expanded {
        let stem = input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("video");
        let filename = format!("{}{}{}.mp4", pfx, stem, sfx);

        let out_dir = match output_dir {
            Some(od) => {
                let base_out = normalize_lexical(od, cwd);
                if recursive {
                    // recrear árbol relativo a cwd
                    let norm_in = normalize_lexical(input, cwd);
                    if let Ok(rel) = norm_in.parent().unwrap_or(&norm_in).strip_prefix(cwd) {
                        base_out.join(rel)
                    } else {
                        if verbose {
                            eprintln!(
                                "[verbose] Entrada fuera de cwd, usando nombre plano: {}",
                                input.display()
                            );
                        }
                        base_out
                    }
                } else {
                    base_out
                }
            }
            None => input.parent().unwrap_or(cwd).to_path_buf(),
        };

        let mut candidate_out = out_dir.join(&filename);
        let mut is_adjusted = false;

        // resolver colisiones intra-lote con _1, _2...
        let mut key = dedup_key(&candidate_out);
        if used_outputs.contains(&key) {
            let mut counter = 1;
            loop {
                let adjusted_filename = format!("{}{}{}_{}.mp4", pfx, stem, sfx, counter);
                let alt_path = out_dir.join(&adjusted_filename);
                let alt_key = dedup_key(&alt_path);
                if !used_outputs.contains(&alt_key) {
                    candidate_out = alt_path;
                    key = alt_key;
                    is_adjusted = true;
                    eprintln!(
                        "[aviso] Salida duplicada intra-lote para '{}', renombrada a '{}'",
                        input.display(),
                        candidate_out.display()
                    );
                    break;
                }
                counter += 1;
            }
        }

        used_outputs.insert(key);

        resolved_list.push(ResolvedItem {
            input: input.clone(),
            output: candidate_out,
            is_duplicate_adjusted: is_adjusted,
        });
    }

    Ok(resolved_list)
}

/// Ejecuta el modo `--dry-run` puro sin I/O ni ffmpeg.
pub fn run_dry_run(
    items: &[ResolvedItem],
    overwrite: bool,
    skip_existing: bool,
    json_mode: bool,
) -> Result<(), E> {
    let mut summary = BatchSummary::default();

    for item in items {
        let exists = item.output.exists();
        let (status, reason) = if exists {
            if overwrite {
                summary.ok += 1;
                (ItemStatus::DryRun, "would overwrite".to_string())
            } else if skip_existing {
                summary.skipped += 1;
                (ItemStatus::DryRun, "would skip".to_string())
            } else {
                summary.failed += 1;
                (
                    ItemStatus::DryRun,
                    "would fail: E_OUTPUT_EXISTS".to_string(),
                )
            }
        } else {
            summary.ok += 1;
            (ItemStatus::DryRun, "would process".to_string())
        };

        if json_mode {
            let line = JsonLogLine {
                input: item.input.display().to_string(),
                output: item.output.display().to_string(),
                status,
                message: reason,
                pct: 0,
            };
            println!("{}", serde_json::to_string(&line).unwrap());
            let _ = std::io::stdout().flush();
        } else {
            eprintln!(
                "{} -> {} ({})",
                item.input.display(),
                item.output.display(),
                reason
            );
        }
    }

    if json_mode {
        let sum_line = JsonSummaryLine { summary };
        println!("{}", serde_json::to_string(&sum_line).unwrap());
        let _ = std::io::stdout().flush();
    } else {
        eprintln!(
            "Summary (dry-run): ok={} failed={} skipped={}",
            summary.ok, summary.failed, summary.skipped
        );
    }

    Ok(())
}

/// Imprime versión según RF-10: `denoise 1.0.0 + modelo DPDFNet + ffmpeg <ver>`.
pub fn print_version(ffmpeg_version: Option<&str>) {
    let pkg_ver = env!("CARGO_PKG_VERSION");
    let ff_str = ffmpeg_version.unwrap_or("ffmpeg missing");
    println!("denoise {} + modelo DPDFNet + {}", pkg_ver, ff_str);
}

/// Punto de entrada principal CLI.
pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Imprimir versión según RF-10 si se pasa --version o -V
    if cli.version {
        let ff_ver = find_ffmpeg(cli.ffmpeg_path.as_deref())
            .ok()
            .and_then(|bin| verify_ffmpeg(&bin).ok());
        print_version(ff_ver.as_deref());
        std::process::exit(0);
    }

    // Validar bitrate estructuralmente
    if !(64..=320).contains(&cli.audio_bitrate) {
        let err = E::EInvalidInput(format!(
            "El bitrate de audio {} kbps está fuera del rango permitido (64-320)",
            cli.audio_bitrate
        ));
        eprintln!("Error: {}", err);
        std::process::exit(err.exit_code());
    }

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // Expansión
    let expanded = match expand_inputs(
        &cli.input,
        cli.recursive,
        cli.output_dir.as_deref(),
        cli.suffix.as_deref().unwrap_or("_denoised"),
        cli.verbose,
        &cwd,
    ) {
        Ok(exp) => exp,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(e.exit_code());
        }
    };

    // Resolución de nombres
    let resolved = match resolve_outputs(
        &expanded,
        cli.output_name.as_deref(),
        cli.output_dir.as_deref(),
        cli.prefix.as_deref(),
        cli.suffix.as_deref(),
        cli.recursive,
        &cwd,
        cli.verbose,
    ) {
        Ok(res) => res,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(e.exit_code());
        }
    };

    // Dry run
    if cli.dry_run {
        if let Err(e) = run_dry_run(&resolved, cli.overwrite, cli.skip_existing, cli.json) {
            eprintln!("Error: {}", e);
            std::process::exit(e.exit_code());
        }
        std::process::exit(0);
    }

    // Configurar signal handler (Ctrl+C)
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let flag_clone = cancel_flag.clone();
    let _ = ctrlc::set_handler(move || {
        if flag_clone.load(Ordering::SeqCst) {
            std::process::exit(3);
        }
        flag_clone.store(true, Ordering::SeqCst);
        eprintln!("\n[aviso] Cancelación solicitada (Ctrl+C), limpiando temporales...");
    });

    // Localizar y verificar ffmpeg
    let ffmpeg_bin = match find_ffmpeg(cli.ffmpeg_path.as_deref()) {
        Ok(bin) => bin,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(e.exit_code());
        }
    };

    if let Err(e) = verify_ffmpeg(&ffmpeg_bin) {
        eprintln!("Error: {}", e);
        std::process::exit(e.exit_code());
    }

    let model_dir = cli
        .model_dir
        .clone()
        .unwrap_or_else(crate::models::default_model_dir);
    let models_provider = crate::models::DefaultModelsProvider::new();

    let params = BatchParams {
        ffmpeg: &ffmpeg_bin,
        model_dir: &model_dir,
        models_provider: &models_provider,
        audio_bitrate: cli.audio_bitrate,
        overwrite: cli.overwrite,
        skip_existing: cli.skip_existing,
        json_mode: cli.json,
        verbose: cli.verbose,
        cancel_flag: Some(cancel_flag),
    };

    let exit_code = process_batch(&resolved, &params);
    std::process::exit(exit_code);
}

/// Parámetros de ejecución para el procesamiento de lotes.
pub struct BatchParams<'a> {
    pub ffmpeg: &'a Path,
    pub model_dir: &'a Path,
    pub models_provider: &'a dyn crate::models::ModelsProvider,
    pub audio_bitrate: u32,
    pub overwrite: bool,
    pub skip_existing: bool,
    pub json_mode: bool,
    pub verbose: bool,
    pub cancel_flag: Option<Arc<AtomicBool>>,
}

/// Procesa un lote de elementos resueltos según el contrato de reportería §8.
pub fn process_batch(items: &[ResolvedItem], params: &BatchParams) -> i32 {
    let mut summary = BatchSummary::default();
    let mut worst_exit_code = 0;
    let total_items = items.len();

    for (idx, item) in items.iter().enumerate() {
        if let Some(flag) = &params.cancel_flag {
            if flag.load(Ordering::SeqCst) {
                return 3;
            }
        }

        let item_idx = idx + 1;
        let in_str = item.input.display().to_string();
        let out_str = item.output.display().to_string();

        let pb = if !params.json_mode {
            let pb = indicatif::ProgressBar::new(100);
            pb.set_style(
                indicatif::ProgressStyle::default_bar()
                    .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}% {msg}")
                    .unwrap_or_else(|_| indicatif::ProgressStyle::default_bar())
                    .progress_chars("=>-"),
            );
            eprintln!("[{}/{}] {} -> {}", item_idx, total_items, in_str, out_str);
            pb
        } else {
            indicatif::ProgressBar::hidden()
        };

        let start_time = std::time::Instant::now();
        let cancel_flag_clone = params.cancel_flag.clone();

        let pipe_params = crate::pipeline::PipelineParams {
            ffmpeg: params.ffmpeg,
            input: &item.input,
            output: &item.output,
            audio_bitrate: params.audio_bitrate,
            model_dir: params.model_dir,
            models_provider: params.models_provider,
            overwrite: params.overwrite,
            skip_existing: params.skip_existing,
            verbose: params.verbose,
            cancel_flag: cancel_flag_clone,
        };

        let result = crate::pipeline::clean_one_video(&pipe_params, |pct, msg| {
            if params.json_mode {
                let line = JsonLogLine {
                    input: in_str.clone(),
                    output: out_str.clone(),
                    status: ItemStatus::Ok,
                    message: msg.to_string(),
                    pct,
                };
                println!("{}", serde_json::to_string(&line).unwrap());
                let _ = std::io::stdout().flush();
            } else {
                pb.set_position(pct as u64);
                pb.set_message(msg.to_string());
            }
        });

        let elapsed = start_time.elapsed().as_secs_f64();

        match result {
            Ok(crate::pipeline::ProcessStatus::Ok) => {
                summary.ok += 1;
                pb.finish_and_clear();
                let size_mb = std::fs::metadata(&item.output)
                    .map(|m| m.len() as f64 / (1024.0 * 1024.0))
                    .unwrap_or(0.0);

                if params.json_mode {
                    let line = JsonLogLine {
                        input: in_str,
                        output: out_str,
                        status: ItemStatus::Ok,
                        message: format!("{:.2} MB, {:.1}s", size_mb, elapsed),
                        pct: 100,
                    };
                    println!("{}", serde_json::to_string(&line).unwrap());
                    let _ = std::io::stdout().flush();
                } else {
                    eprintln!(
                        "[{}/{}] done {} -> {} ({:.2} MB, {:.1}s)",
                        item_idx,
                        total_items,
                        item.input.display(),
                        item.output.display(),
                        size_mb,
                        elapsed
                    );
                }
            }
            Ok(crate::pipeline::ProcessStatus::Skipped) => {
                summary.skipped += 1;
                pb.finish_and_clear();

                if params.json_mode {
                    let line = JsonLogLine {
                        input: in_str,
                        output: out_str,
                        status: ItemStatus::Skipped,
                        message: "skipped".to_string(),
                        pct: 0,
                    };
                    println!("{}", serde_json::to_string(&line).unwrap());
                    let _ = std::io::stdout().flush();
                } else {
                    eprintln!(
                        "[{}/{}] skipped {} -> {}",
                        item_idx,
                        total_items,
                        item.input.display(),
                        item.output.display()
                    );
                }
            }
            Err(e) => {
                summary.failed += 1;
                pb.finish_and_clear();

                let code = e.exit_code();
                if code == 3 {
                    return 3;
                }
                if code == 1 || (code == 2 && worst_exit_code != 1) {
                    worst_exit_code = code;
                }

                if params.json_mode {
                    let line = JsonLogLine {
                        input: in_str,
                        output: out_str,
                        status: ItemStatus::Failed,
                        message: e.to_string(),
                        pct: 100,
                    };
                    println!("{}", serde_json::to_string(&line).unwrap());
                    let _ = std::io::stdout().flush();
                } else {
                    eprintln!(
                        "[{}/{}] failed {} -> {}: {}",
                        item_idx,
                        total_items,
                        item.input.display(),
                        item.output.display(),
                        e
                    );
                }
            }
        }
    }

    if params.json_mode {
        let sum_line = JsonSummaryLine { summary };
        println!("{}", serde_json::to_string(&sum_line).unwrap());
        let _ = std::io::stdout().flush();
    } else {
        eprintln!(
            "Summary: ok={} failed={} skipped={}",
            summary.ok, summary.failed, summary.skipped
        );
    }

    worst_exit_code
}
