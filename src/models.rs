/// Módulo `models.rs` — descarga y verificación del modelo DeepFilterNet3.
///
/// Artefactos: `dfn3_enc.onnx`, `dfn3_erb_dec.onnx`, `dfn3_df_dec.onnx` (~8MB, tarball 7983136B).
/// Origen: `https://github.com/Rikorose/DeepFilterNet/raw/v0.5.6/models/DeepFilterNet3_onnx.tar.gz`.
/// SHA256: `C94D91F70911001C946E0FABB4AA9ADC37045F45A03B56008CB0C8244CB63616`.
/// Ver `plan.md T2.0` y `design.md §7`.
use std::path::Path;

/// Verifica que el modelo exista en `model_dir`; si no, lo descarga.
pub fn ensure_models(_model_dir: &Path, _progress_cb: impl Fn(f64, f64)) -> anyhow::Result<()> {
    // TODO: verificar SHA256 + tamaño + descarga si falta
    anyhow::bail!("Models not yet downloaded (Fase 2)");
}

/// Descarga y extrae el tarball del modelo.
pub fn download_models(_model_dir: &Path, _progress_cb: impl Fn(f64, f64)) -> anyhow::Result<()> {
    // TODO: reqwest blocking + tar.gz + sha256 verify
    anyhow::bail!("Download not yet implemented (Fase 2)");
}
