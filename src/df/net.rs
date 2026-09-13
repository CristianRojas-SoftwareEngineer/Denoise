//! Módulo `df::net` — Sesiones `ort` y ejecución de inferencia para los 3 grafos DeepFilterNet3.
//!
//! Contrato: `docs/design.md §6, §7`, `docs/specifications.md §RF-05, RNF-04`.
//! Modelos: `enc`, `erb_dec`, `df_dec`.

use crate::errors::E;
use crate::models::ModelPaths;
use ort::session::Session;
use ort::value::Value;
use std::sync::Mutex;

/// Estructura contenedora de las 3 sesiones ONNX Runtime CPU.
pub struct DfSessions {
    pub enc: Session,
    pub erb_dec: Session,
    pub df_dec: Session,
}

impl DfSessions {
    /// Carga las 3 sesiones ONNX desde las rutas de los modelos.
    pub fn load(models: &ModelPaths) -> Result<Self, E> {
        let enc = Session::builder()
            .map_err(|e| E::EModelMissing(format!("Error en SessionBuilder: {}", e)))?
            .commit_from_file(&models.enc)
            .map_err(|e| E::EModelMissing(format!("Error cargando dfn3_enc.onnx: {}", e)))?;

        let erb_dec = Session::builder()
            .map_err(|e| E::EModelMissing(format!("Error en SessionBuilder: {}", e)))?
            .commit_from_file(&models.erb_dec)
            .map_err(|e| E::EModelMissing(format!("Error cargando dfn3_erb_dec.onnx: {}", e)))?;

        let df_dec = Session::builder()
            .map_err(|e| E::EModelMissing(format!("Error en SessionBuilder: {}", e)))?
            .commit_from_file(&models.df_dec)
            .map_err(|e| E::EModelMissing(format!("Error cargando dfn3_df_dec.onnx: {}", e)))?;

        Ok(Self {
            enc,
            erb_dec,
            df_dec,
        })
    }

    /// Ejecuta la inferencia de 3 grafos sobre los tensores de features:
    /// Retorna:
    /// - `mask`: ganancia ERB [T, 32]
    /// - `coefs`: coeficientes DF [T, 96, 5, 2]
    /// - `alpha`: factor de mezcla DF/ERB [T]
    /// - `lsnr`: SNR local por frame [T]
    #[allow(clippy::type_complexity)]
    pub fn infer(
        &mut self,
        feat_erb_flat: Vec<f32>,  // [1, 1, T, 32]
        feat_spec_flat: Vec<f32>, // [1, 2, T, 96]
        t_len: usize,
    ) -> Result<(Vec<Vec<f32>>, Vec<Vec<Vec<[f32; 2]>>>, Vec<f32>, Vec<f32>), E> {
        // 1. Grafo Encoder
        let enc_in_erb = Value::from_array(([1, 1, t_len, 32], feat_erb_flat))
            .map_err(|e| E::EFfmpegFailed(format!("Error creando tensor feat_erb: {}", e)))?;
        let enc_in_spec = Value::from_array(([1, 2, t_len, 96], feat_spec_flat))
            .map_err(|e| E::EFfmpegFailed(format!("Error creando tensor feat_spec: {}", e)))?;

        let enc_outputs = self
            .enc
            .run(ort::inputs!["feat_erb" => enc_in_erb, "feat_spec" => enc_in_spec])
            .map_err(|e| E::EFfmpegFailed(format!("Error en inferencia enc: {}", e)))?;

        // Mapeo de salidas enc según nombres oficiales:
        // 0: e0, 1: e1, 2: e2, 3: e3, 4: emb, 5: c0, 6: lsnr
        let e0 = &enc_outputs[0];
        let e1 = &enc_outputs[1];
        let e2 = &enc_outputs[2];
        let e3 = &enc_outputs[3];
        let emb = &enc_outputs[4];
        let c0 = &enc_outputs[5];
        let lsnr_val = &enc_outputs[6];

        // Extraer lsnr como Vec<f32> de tamaño T
        let (_lsnr_shape, lsnr_slice) = lsnr_val
            .try_extract_tensor::<f32>()
            .map_err(|e| E::EFfmpegFailed(format!("Error extrayendo lsnr: {}", e)))?;
        let lsnr: Vec<f32> = lsnr_slice.to_vec();

        // 2. Grafo ERB Decoder
        let erb_outputs = self
            .erb_dec
            .run(ort::inputs!["emb" => emb, "e3" => e3, "e2" => e2, "e1" => e1, "e0" => e0])
            .map_err(|e| E::EFfmpegFailed(format!("Error en inferencia erb_dec: {}", e)))?;

        let (_mask_shape, mask_data) = erb_outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| E::EFfmpegFailed(format!("Error extrayendo mask: {}", e)))?;

        // Reestructurar mask [T, 32]
        let mut mask = Vec::with_capacity(t_len);
        for t in 0..t_len {
            let start = t * 32;
            let end = start + 32;
            if end <= mask_data.len() {
                mask.push(mask_data[start..end].to_vec());
            } else {
                mask.push(vec![1.0; 32]);
            }
        }

        // 3. Grafo DF Decoder
        let df_outputs = self
            .df_dec
            .run(ort::inputs!["emb" => emb, "c0" => c0])
            .map_err(|e| E::EFfmpegFailed(format!("Error en inferencia df_dec: {}", e)))?;

        let (_coef_shape, coef_data) = df_outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| E::EFfmpegFailed(format!("Error extrayendo coefs: {}", e)))?;

        let (_alpha_shape, alpha_data) = df_outputs[1]
            .try_extract_tensor::<f32>()
            .map_err(|e| E::EFfmpegFailed(format!("Error extrayendo alpha: {}", e)))?;
        let alpha: Vec<f32> = alpha_data.to_vec();

        // coefs output shape [1, T, 96, 10] donde 10 = 5 taps * 2 (re, im)
        let mut coefs = Vec::with_capacity(t_len);
        let coef_per_frame = 96 * 10;
        for t in 0..t_len {
            let frame_start = t * coef_per_frame;
            let mut frame_coefs = Vec::with_capacity(96);
            for f in 0..96 {
                let bin_start = frame_start + f * 10;
                let mut taps = Vec::with_capacity(5);
                for p in 0..5 {
                    let tap_start = bin_start + p * 2;
                    if tap_start + 1 < coef_data.len() {
                        taps.push([coef_data[tap_start], coef_data[tap_start + 1]]);
                    } else {
                        taps.push([0.0, 0.0]);
                    }
                }
                frame_coefs.push(taps);
            }
            coefs.push(frame_coefs);
        }

        Ok((mask, coefs, alpha, lsnr))
    }
}

/// Cache global thread-safe para sesiones ort (D6).
pub static ORT_CACHE: Mutex<Option<DfSessions>> = Mutex::new(None);
