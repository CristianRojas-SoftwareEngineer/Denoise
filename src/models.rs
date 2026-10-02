//! Módulo `models.rs` — descarga, verificación SHA256 y gestión del modelo DPDFNet.
//!
//! Artefacto: `dpdfnet8_48khz_hr.onnx` (14.857.107 B), un único grafo ONNX.
//! Origen canónico: `https://huggingface.co/Ceva-IP/DPDFNet/resolve/main/onnx/dpdfnet8_48khz_hr.onnx`.
//! SHA256: `7b3afbb260a08fe9af3d16e3bda992971be1e7e951d1dee7c2d235f5c43f5631`.
//! Licencia del modelo: Apache 2.0 (Ceva-IP/DPDFNet).
//! Contrato: `docs/design.md §7`, `docs/specifications.md §2 RF-06, RNF-05`.

use crate::errors::E;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use sysinfo::Disks;

/// URL canónica del ONNX de DPDFNet 48 kHz HR (variante `-8`, 8 bloques DPRNN).
pub const DPDFNET8_URL: &str =
    "https://huggingface.co/Ceva-IP/DPDFNet/resolve/main/onnx/dpdfnet8_48khz_hr.onnx";
pub const DPDFNET8_SHA256: &str =
    "7b3afbb260a08fe9af3d16e3bda992971be1e7e951d1dee7c2d235f5c43f5631";
pub const DPDFNET8_SIZE: u64 = 14857107;
pub const MODEL_FILE_NAME: &str = "dpdfnet8_48khz_hr.onnx";
pub const MIN_DISK_SPACE_BYTES: u64 = 50 * 1024 * 1024; // 50MB

/// Rutas absolutas al archivo ONNX del modelo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelPaths {
    pub onnx: PathBuf,
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
        let onnx_path = model_dir.join(MODEL_FILE_NAME);

        if onnx_path.exists()
            && fs::metadata(&onnx_path)
                .map(|m| m.len() > 0)
                .unwrap_or(false)
        {
            return Ok(ModelPaths { onnx: onnx_path });
        }

        // Crear directorio padre de modelos si no existe
        fs::create_dir_all(model_dir).map_err(E::EIo)?;

        // Chequeo de espacio en disco >= 50MB
        check_disk_space(model_dir)?;

        // Descarga directa con verificación SHA256
        download_model(model_dir, progress_cb)?;

        if onnx_path.exists() {
            Ok(ModelPaths { onnx: onnx_path })
        } else {
            Err(E::EModelMissing(format!(
                "Descarga incompleta. Archivo faltante en {}",
                model_dir.display()
            )))
        }
    }
}

/// Proveedor simulado para tests unitarios rápidos sin red ni ONNX real.
pub struct FakeProvider {
    pub should_fail: bool,
    pub fail_with_io: bool,
}

impl FakeProvider {
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
                    DPDFNET8_URL
                )));
            }
        }

        let onnx_path = model_dir.join(MODEL_FILE_NAME);

        // Crea archivo fake si no existe
        let _ = fs::create_dir_all(model_dir);
        let _ = File::create(&onnx_path);

        Ok(ModelPaths { onnx: onnx_path })
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

fn download_model(model_dir: &Path, progress_cb: &dyn Fn(f64, f64)) -> Result<(), E> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("denoise/1.0.0")
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| E::EModelMissing(format!("Error inicializando cliente HTTP: {}", e)))?;

    let part_path = model_dir.join(format!("{}.part", MODEL_FILE_NAME));
    let mut last_err = None;

    // Retry con 3 intentos y backoff
    for attempt in 1..=3 {
        match download_file(&client, &part_path, progress_cb) {
            Ok(bytes_read) => {
                // Verificar tamaño >= 98%
                if bytes_read < (DPDFNET8_SIZE * 98) / 100 {
                    let _ = fs::remove_file(&part_path);
                    return Err(E::EModelMissing(format!(
                        "Tamaño del modelo descargado incompleto ({} bytes de {} esperados)",
                        bytes_read, DPDFNET8_SIZE
                    )));
                }

                // Verificar SHA256
                let computed_hash = compute_file_sha256(&part_path)?;
                if computed_hash.to_lowercase() != DPDFNET8_SHA256 {
                    let _ = fs::remove_file(&part_path);
                    return Err(E::EModelMissing(format!(
                        "SHA256 mismatch en modelo descargado. Esperado: {}, obtenido: {}",
                        DPDFNET8_SHA256, computed_hash
                    )));
                }

                // Renombrar.part -> destino final
                let final_path = model_dir.join(MODEL_FILE_NAME);
                if final_path.exists() {
                    let _ = fs::remove_file(&final_path);
                }
                fs::rename(&part_path, &final_path).map_err(E::EIo)?;
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

    let _ = fs::remove_file(&part_path);
    Err(E::EModelMissing(format!(
 "Error descargando modelo desde '{}' tras 3 intentos: {:?}. Puedes descargarlo manualmente y colocarlo en '{}'",
 DPDFNET8_URL,
        last_err,
        model_dir.display()
    )))
}

fn download_file(
    client: &reqwest::blocking::Client,
    dest: &Path,
    progress_cb: &dyn Fn(f64, f64),
) -> Result<u64, E> {
    let mut response = client
        .get(DPDFNET8_URL)
        .send()
        .map_err(|e| E::EModelMissing(format!("Fallo de conexión HTTP: {}", e)))?;

    if !response.status().is_success() {
        return Err(E::EModelMissing(format!(
            "HTTP {} al descargar modelo",
            response.status()
        )));
    }

    let total_size = response.content_length().unwrap_or(DPDFNET8_SIZE) as f64;
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
