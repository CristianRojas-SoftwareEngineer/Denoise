//! Módulo `pipeline.rs` — Orquestación del pipeline completo por medio (video o audio).
//!
//! Contrato: `docs/design.md §5, §8`.
//! Especificaciones: `docs/specifications.md §2 RF-05, RF-08, RF-09 y §3 RNF-03, RNF-06`.

use crate::df::denoise_wav_with_params;
use crate::errors::E;
use crate::ffmpeg_io::{
    egress_media, extract_audio48k, probe, verify_output_lightweight, MediaKind,
};
use crate::models::ModelsProvider;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Estado de procesamiento de un elemento.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessStatus {
    Ok,
    Skipped,
}

/// Parámetros para procesar un medio individual.
pub struct PipelineParams<'a> {
    pub ffmpeg: &'a Path,
    pub input: &'a Path,
    pub output: &'a Path,
    pub audio_bitrate: u32,
    pub dry_blend: f32,
    pub pause_gate_db: f64,
    pub highpass: bool,
    pub stereo: bool,
    pub target_lufs: Option<f64>,
    pub model_dir: &'a Path,
    pub models_provider: &'a dyn ModelsProvider,
    pub overwrite: bool,
    pub skip_existing: bool,
    pub verbose: bool,
    pub cancel_flag: Option<Arc<AtomicBool>>,
}

/// Guardia para limpiar archivos temporales en error o Drop.
struct TempCleaner {
    paths: Vec<PathBuf>,
    preserve: bool,
}

impl TempCleaner {
    fn new(preserve: bool) -> Self {
        Self {
            paths: Vec::new(),
            preserve,
        }
    }

    fn track(&mut self, path: PathBuf) {
        self.paths.push(path);
    }

    fn cleanup_all(&mut self) {
        if !self.preserve {
            for p in &self.paths {
                let _ = fs::remove_file(p);
            }
        }
        self.paths.clear();
    }
}

impl Drop for TempCleaner {
    fn drop(&mut self) {
        if !self.preserve {
            for p in &self.paths {
                let _ = fs::remove_file(p);
            }
        }
    }
}

/// Procesa un único archivo de medio de principio a fin según el contrato §5.
///
/// Progreso:
/// - 0: Inicio / colisión-check
/// - 1-5%: Extracción de audio ffmpeg
/// - 6-80%: Denoise DPDFNet secuencial
/// - 81-95%: Egresión ffmpeg a `<output_stem>.part.<ext>` + atomic rename
/// - 96-99%: Verificación ligera
/// - 100%: Completado
pub fn clean_media<F>(params: &PipelineParams, progress_cb: F) -> Result<ProcessStatus, E>
where
    F: Fn(u32, &str),
{
    // Verificar cancelación previa
    if let Some(flag) = &params.cancel_flag {
        if flag.load(Ordering::SeqCst) {
            return Err(E::ECancelled);
        }
    }

    // 0. Crear directorios padre si no existen
    if let Some(parent) = params.output.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(E::EIo)?;
        }
    }

    // 0. Colisión check
    if params.output.exists() {
        if params.skip_existing {
            progress_cb(0, "skipped");
            return Ok(ProcessStatus::Skipped);
        } else if !params.overwrite {
            return Err(E::EOutputExists(format!(
                "El archivo de destino ya existe: {}",
                params.output.display()
            )));
        }
    }

    progress_cb(0, "iniciando");

    // Pre-chequeo: probe del contenedor
    let probe_res = probe(params.ffmpeg, params.input)?;
    if !probe_res.has_audio {
        return Err(E::ENoAudio);
    }

    // Asegurar modelos listos
    let _ = params
        .models_provider
        .ensure_models(params.model_dir, &|_, _| {})?;

    // Rutas de temporales junto a la salida; el parcial conserva stem y extensión finales.
    let out_file_name = params
        .output
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_else(|| "out.mp4".to_string());
    let out_stem = params
        .output
        .file_stem()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_else(|| "out".to_string());
    let out_ext = params
        .output
        .extension()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_else(|| "mp4".to_string());

    let tmp_in_wav = params
        .output
        .with_file_name(format!("{}.tmp.in.wav", out_file_name));
    let tmp_out_wav = params
        .output
        .with_file_name(format!("{}.tmp.out.wav", out_file_name));
    let out_part = params
        .output
        .with_file_name(format!("{}.part.{}", out_stem, out_ext));

    let mut cleaner = TempCleaner::new(params.verbose);
    cleaner.track(tmp_in_wav.clone());
    cleaner.track(tmp_out_wav.clone());
    cleaner.track(out_part.clone());

    // 1-5%: Extracción de audio 48k PCM16 (mono o estéreo según flag y contenedor)
    progress_cb(1, "extrayendo audio");
    let channels = if params.stereo && probe_res.channels == 2 {
        2
    } else {
        1
    };
    extract_audio48k(params.ffmpeg, params.input, &tmp_in_wav, channels)?;
    progress_cb(5, "audio extraído");

    if let Some(flag) = &params.cancel_flag {
        if flag.load(Ordering::SeqCst) {
            return Err(E::ECancelled);
        }
    }

    // 6-80%: Denoise DPDFNet DSP
    progress_cb(6, "eliminando ruido");
    denoise_wav_with_params(
        &tmp_in_wav,
        &tmp_out_wav,
        params.model_dir,
        params.models_provider,
        params.dry_blend,
        params.pause_gate_db,
        params.highpass,
        &|cur_secs, total_secs| {
            if total_secs > 0.0 {
                let ratio = (cur_secs / total_secs).clamp(0.0, 1.0);
                let pct = 6 + (ratio * 74.0).round() as u32;
                progress_cb(pct, "procesando DSP");
            }
        },
    )?;
    progress_cb(80, "ruido eliminado");

    if let Some(flag) = &params.cancel_flag {
        if flag.load(Ordering::SeqCst) {
            return Err(E::ECancelled);
        }
    }

    // 81-95%: Egresión especializada según MediaKind a `<out_stem>.part.<ext>`
    let is_video = probe_res.media_kind == MediaKind::Video;
    progress_cb(
        81,
        if is_video {
            "remuxando video"
        } else {
            "empaquetando audio"
        },
    );
    egress_media(
        params.ffmpeg,
        probe_res.media_kind,
        params.input,
        &tmp_out_wav,
        &out_part,
        params.audio_bitrate,
        probe_res.duration,
        params.target_lufs,
    )?;
    progress_cb(
        90,
        if is_video {
            "remux completado"
        } else {
            "audio empaquetado"
        },
    );

    // Rename atómico al archivo final
    fs::rename(&out_part, params.output).map_err(E::EIo)?;
    progress_cb(95, "archivo renombrado");

    // 96-99%: Verificación ligera runtime adaptada al medio
    progress_cb(96, "verificando salida");
    let verified = verify_output_lightweight(
        params.ffmpeg,
        params.output,
        probe_res.duration,
        probe_res.media_kind,
    )?;
    if !verified {
        return Err(E::EFfmpegFailed(format!(
            "Verificación ligera falló para {}",
            params.output.display()
        )));
    }
    progress_cb(99, "verificación exitosa");

    // Limpieza de temporales
    cleaner.cleanup_all();

    progress_cb(100, "completado");
    Ok(ProcessStatus::Ok)
}
