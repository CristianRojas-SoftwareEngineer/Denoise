/// Módulo `df::net` — sesiones `ort` cacheadas para los 3 grafos DFN3.
///
/// Grafos: `enc(feat_erb, feat_spec) → emb,e0..e3,c0,lsnr`,
///         `erb_dec(emb,e3,e2,e1,e0) → mask`,
///         `df_dec(emb,c0) → coefs`.
///
/// Ver `plan.md T2.2` y `design.md §7`.
/// Carga los 3 modelos ONNX en `model_dir`.
pub fn load_models(_model_dir: &std::path::Path) -> anyhow::Result<()> {
    // TODO: cargar dfn3_enc.onnx, dfn3_erb_dec.onnx, dfn3_df_dec.onnx
    anyhow::bail!("Models not yet loaded (Fase 2)");
}

/// Inferencia sobre un chunk: `enc → emb,e0..e3,c0,lsnr → erb_dec → mask → df_dec → coefs`.
pub fn infer(_features: &[Vec<f32>]) -> anyhow::Result<Vec<f32>> {
    // TODO: implementar inferencia
    anyhow::bail!("Inference not yet implemented (Fase 3)");
}
