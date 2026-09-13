//! Módulo `models.rs` — descarga, verificación SHA256 y gestión del modelo DeepFilterNet3.
//!
//! Artefactos: `dfn3_enc.onnx`, `dfn3_erb_dec.onnx`, `dfn3_df_dec.onnx` (~8MB, tarball 7983136B).
//! Origen canónico: `https://github.com/Rikorose/DeepFilterNet/raw/v0.5.6/models/DeepFilterNet3_onnx.tar.gz`.
//! SHA256: `C94D91F70911001C946E0FABB4AA9ADC37045F45A03B56008CB0C8244CB63616`.
//! Contrato: `docs/design.md §7`, `docs/specifications.md §2 RF-06, RNF-05`.

use crate::errors::E;
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use sysinfo::Disks;
use tar::Archive;

pub const DFN3_TAR_URL: &str =
    "https://github.com/Rikorose/DeepFilterNet/raw/v0.5.6/models/DeepFilterNet3_onnx.tar.gz";
pub const DFN3_TAR_SHA256: &str =
    "c94d91f70911001c946e0fabb4aa9adc37045f45a03b56008cb0c8244cb63616";
pub const DFN3_TAR_SIZE: u64 = 7983136;
pub const MIN_DISK_SPACE_BYTES: u64 = 50 * 1024 * 1024; // 50MB (D17, D29)

/// Rutas absolutas a los 3 archivos ONNX del modelo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelPaths {
    pub enc: PathBuf,
    pub erb_dec: PathBuf,
    pub df_dec: PathBuf,
}

/// Trait para abstracción de proveedores de modelos (permite `FakeProvider` en tests).
pub trait ModelsProvider: Send + Sync {
    fn ensure_models(
        &self,
        model_dir: &Path,
        progress_cb: &dyn Fn(f64, f64),
    ) -> Result<ModelPaths, E>;
}

/// Proveedor de modelos por defecto con descarga HTTPS y verificación SHA256.
pub struct DefaultModelsProvider;

impl DefaultModelsProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DefaultModelsProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelsProvider for DefaultModelsProvider {
    fn ensure_models(
        &self,
        model_dir: &Path,
        progress_cb: &dyn Fn(f64, f64),
    ) -> Result<ModelPaths, E> {
        let enc_path = model_dir.join("dfn3_enc.onnx");
        let erb_dec_path = model_dir.join("dfn3_erb_dec.onnx");
        let df_dec_path = model_dir.join("dfn3_df_dec.onnx");

        if enc_path.exists()
            && erb_dec_path.exists()
            && df_dec_path.exists()
            && fs::metadata(&enc_path)
                .map(|m| m.len() > 0)
                .unwrap_or(false)
            && fs::metadata(&erb_dec_path)
                .map(|m| m.len() > 0)
                .unwrap_or(false)
            && fs::metadata(&df_dec_path)
                .map(|m| m.len() > 0)
                .unwrap_or(false)
        {
            return Ok(ModelPaths {
                enc: enc_path,
                erb_dec: erb_dec_path,
                df_dec: df_dec_path,
            });
        }

        // Crear directorio padre de modelos si no existe (D7)
        fs::create_dir_all(model_dir).map_err(E::EIo)?;

        // Chequeo de espacio en disco >= 50MB (D17, D29)
        check_disk_space(model_dir)?;

        // Descarga y extracción
        download_and_extract_model(model_dir, progress_cb)?;

        if enc_path.exists() && erb_dec_path.exists() && df_dec_path.exists() {
            Ok(ModelPaths {
                enc: enc_path,
                erb_dec: erb_dec_path,
                df_dec: df_dec_path,
            })
        } else {
            Err(E::EModelMissing(format!(
                "Extracción incompleta. Archivos faltantes en {}",
                model_dir.display()
            )))
        }
    }
}

/// Proveedor simulado para tests unitarios rápidos sin red ni ONNX real (D22).
pub struct FakeProvider {
    pub should_fail: bool,
    pub fail_with_io: bool,
}

impl FakeProvider {
    pub fn success() -> Self {
        Self {
            should_fail: false,
            fail_with_io: false,
        }
    }

    pub fn failing_missing() -> Self {
        Self {
            should_fail: true,
            fail_with_io: false,
        }
    }

    pub fn failing_io() -> Self {
        Self {
            should_fail: true,
            fail_with_io: true,
        }
    }
}

impl ModelsProvider for FakeProvider {
    fn ensure_models(
        &self,
        model_dir: &Path,
        _progress_cb: &dyn Fn(f64, f64),
    ) -> Result<ModelPaths, E> {
        if self.should_fail {
            if self.fail_with_io {
                return Err(E::EIo(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "Simulated I/O permission denied on model directory",
                )));
            } else {
                return Err(E::EModelMissing(format!(
                    "Modelo no encontrado (simulado). Descarga manual en: {}",
                    DFN3_TAR_URL
                )));
            }
        }

        let enc_path = model_dir.join("dfn3_enc.onnx");
        let erb_dec_path = model_dir.join("dfn3_erb_dec.onnx");
        let df_dec_path = model_dir.join("dfn3_df_dec.onnx");

        // Crea archivos fake si no existen
        let _ = fs::create_dir_all(model_dir);
        let _ = File::create(&enc_path);
        let _ = File::create(&erb_dec_path);
        let _ = File::create(&df_dec_path);

        Ok(ModelPaths {
            enc: enc_path,
            erb_dec: erb_dec_path,
            df_dec: df_dec_path,
        })
    }
}

/// Devuelve la ruta por defecto del directorio de modelos: `~/.cache/denoise/models`.
pub fn default_model_dir() -> PathBuf {
    home::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".cache/denoise/models")
}

fn check_disk_space(model_dir: &Path) -> Result<(), E> {
    let disks = Disks::new_with_refreshed_list();
    let norm_path = model_dir
        .canonicalize()
        .unwrap_or_else(|_| model_dir.to_path_buf());

    for disk in disks.list() {
        let mount_point = disk.mount_point();
        if norm_path.starts_with(mount_point) {
            if disk.available_space() < MIN_DISK_SPACE_BYTES {
                return Err(E::EIo(std::io::Error::other(format!(
                    "Espacio insuficiente en disco para el modelo (disponible: {}B, requerido: {}B)",
                    disk.available_space(),
                    MIN_DISK_SPACE_BYTES
                ))));
            }
            return Ok(());
        }
    }
    // Si no se pudo determinar el disco exacto, permitimos continuar
    Ok(())
}

fn download_and_extract_model(model_dir: &Path, progress_cb: &dyn Fn(f64, f64)) -> Result<(), E> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("denoise/1.0.0")
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| E::EModelMissing(format!("Error inicializando cliente HTTP: {}", e)))?;

    let tar_part_path = model_dir.join("DeepFilterNet3_onnx.tar.gz.part");
    let mut last_err = None;

    // Retry con 3 intentos y backoff
    for attempt in 1..=3 {
        match download_tarball(&client, &tar_part_path, progress_cb) {
            Ok(bytes_read) => {
                // Verificar tamaño >= 98%
                if bytes_read < (DFN3_TAR_SIZE * 98) / 100 {
                    let _ = fs::remove_file(&tar_part_path);
                    return Err(E::EModelMissing(format!(
                        "Tamaño del modelo descargado incompleto ({} bytes de {} esperados)",
                        bytes_read, DFN3_TAR_SIZE
                    )));
                }

                // Verificar SHA256
                let computed_hash = compute_file_sha256(&tar_part_path)?;
                if computed_hash.to_lowercase() != DFN3_TAR_SHA256 {
                    let _ = fs::remove_file(&tar_part_path);
                    return Err(E::EModelMissing(format!(
                        "SHA256 mismatch en modelo descargado. Esperado: {}, obtenido: {}",
                        DFN3_TAR_SHA256, computed_hash
                    )));
                }

                // Extraer con anti tar-slip
                extract_tarball(&tar_part_path, model_dir)?;
                let _ = fs::remove_file(&tar_part_path);
                return Ok(());
            }
            Err(e) => {
                last_err = Some(e);
                if attempt < 3 {
                    std::thread::sleep(Duration::from_secs(attempt * 2));
                }
            }
        }
    }

    let _ = fs::remove_file(&tar_part_path);
    Err(E::EModelMissing(format!(
        "Error descargando modelo desde '{}' tras 3 intentos: {:?}. Puedes descargarlo manualmente y colocar los archivos ONNX en '{}'",
        DFN3_TAR_URL,
        last_err,
        model_dir.display()
    )))
}

fn download_tarball(
    client: &reqwest::blocking::Client,
    dest: &Path,
    progress_cb: &dyn Fn(f64, f64),
) -> Result<u64, E> {
    let mut response = client
        .get(DFN3_TAR_URL)
        .send()
        .map_err(|e| E::EModelMissing(format!("Fallo de conexión HTTP: {}", e)))?;

    if !response.status().is_success() {
        return Err(E::EModelMissing(format!(
            "HTTP {} al descargar modelo",
            response.status()
        )));
    }

    let total_size = response.content_length().unwrap_or(DFN3_TAR_SIZE) as f64;
    let mut file = File::create(dest).map_err(E::EIo)?;
    let mut downloaded = 0u64;
    let mut buffer = [0u8; 64 * 1024];

    loop {
        let bytes_read = response
            .read(&mut buffer)
            .map_err(|e| E::EModelMissing(format!("Error leyendo stream HTTP: {}", e)))?;
        if bytes_read == 0 {
            break;
        }
        file.write_all(&buffer[..bytes_read]).map_err(E::EIo)?;
        downloaded += bytes_read as u64;
        progress_cb(downloaded as f64, total_size);
    }

    Ok(downloaded)
}

fn compute_file_sha256(path: &Path) -> Result<String, E> {
    let mut file = File::open(path).map_err(E::EIo)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(E::EIo)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn extract_tarball(tar_gz_path: &Path, out_dir: &Path) -> Result<(), E> {
    let tar_gz_file = File::open(tar_gz_path).map_err(E::EIo)?;
    let tar = GzDecoder::new(tar_gz_file);
    let mut archive = Archive::new(tar);

    for entry in archive.entries().map_err(E::EIo)? {
        let mut entry = entry.map_err(E::EIo)?;
        let path = entry.path().map_err(E::EIo)?.to_path_buf();

        // Anti tar-slip: solo permitir nombres relativos seguros en tmp/export/
        let path_str = path.to_string_lossy().replace('\\', "/");
        let dest_filename = if path_str.ends_with("enc.onnx") {
            "dfn3_enc.onnx"
        } else if path_str.ends_with("erb_dec.onnx") {
            "dfn3_erb_dec.onnx"
        } else if path_str.ends_with("df_dec.onnx") {
            "dfn3_df_dec.onnx"
        } else {
            // Ignorar otros ficheros (como config.ini)
            continue;
        };

        let target_path = out_dir.join(dest_filename);
        entry.unpack(&target_path).map_err(E::EIo)?;
    }

    Ok(())
}
