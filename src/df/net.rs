//! Módulo `df::net` — Sesión `ort` y ejecución de inferencia para el grafo único DPDFNet.
//!
//! Contrato: `docs/design.md §6, §7`, `docs/specifications.md §2 RF-05 y §3 RNF-04`.
//! Modelo: `dpdfnet8_48khz_hr.onnx` (un solo grafo, stateful streaming).
//!
//! Interfaz ONNX verificada (metadatos del propio modelo):
//! - entrada `spec` : f32 [1, 1, 481, 2] (un frame, espectro complejo real/imag)
//! - entrada `state_in`: f32 [90228]
//! - salida `spec_e` : f32 [1, 1, 481, 2]
//! - salida `state_out`: f32 [90228]
//!
//! El grafo es *stateful*: cada frame debe encadenarse con el `state_out` del
//! anterior (igual que `OfflineSpeechDenoiserDpdfNetImpl` de sherpa-onnx, que
//! conserva las `T` salidas y solo desplaza el recorte de la síntesis). El
//! estado inicial se construye a partir de los metadatos `erb_norm_init` y
//! `spec_norm_init` embebidos en el propio ONNX, por lo que no hay constantes
//! flotantes hardcodeadas aquí.

use crate::errors::E;
use crate::models::ModelPaths;
use ort::session::Session;
use ort::value::Value;
use rustfft::num_complex::Complex32;
use std::sync::Mutex;

/// Estructura contenedora de la sesión ONNX Runtime CPU y su estado recurrente.
pub struct DpdfNetSession {
    session: Session,
    state_size: usize,
    erb_norm_state_size: usize,
    spec_norm_state_size: usize,
}

impl DpdfNetSession {
    /// Carga la sesión ONNX desde la ruta del modelo y lee su contrato de metadatos.
    pub fn load(models: &ModelPaths) -> Result<Self, E> {
        let session = Session::builder()
            .map_err(|e| E::EModelMissing(format!("Error en SessionBuilder: {}", e)))?
            .commit_from_file(&models.onnx)
            .map_err(|e| {
                E::EModelMissing(format!("Error cargando {}: {}", models.onnx.display(), e))
            })?;

        let (
            state_size,
            erb_norm_state_size,
            spec_norm_state_size,
            freq_bins,
            n_fft,
            hop,
            sample_rate,
        ) = {
            // El ámbito acaba aquí para liberar el préstamo de metadatos antes de
            // mover la sesión dentro del struct.
            let meta = session.metadata().map_err(|e| {
                E::EModelMissing(format!("Error leyendo metadatos del modelo: {}", e))
            })?;

            let read_usize = |key: &str| -> Result<usize, E> {
                meta.custom(key)
                    .and_then(|v| v.trim().parse::<usize>().ok())
                    .ok_or_else(|| {
                        E::EModelMissing(format!(
                            "Metadato '{}' ausente o inválido en el modelo",
                            key
                        ))
                    })
            };

            (
                read_usize("state_size")?,
                read_usize("erb_norm_state_size")?,
                read_usize("spec_norm_state_size")?,
                read_usize("freq_bins")?,
                read_usize("n_fft")?,
                read_usize("hop_length")?,
                read_usize("sample_rate")?,
            )
        };

        let expected_bins = n_fft / 2 + 1;
        if freq_bins != expected_bins {
            return Err(E::EModelMissing(format!(
                "Modelo con freq_bins={} incompatible con n_fft={} (esperado {})",
                freq_bins, n_fft, expected_bins
            )));
        }
        if sample_rate != crate::df::stft::SR
            || n_fft != crate::df::stft::FFT_SIZE
            || hop != crate::df::stft::HOP_SIZE
        {
            return Err(E::EModelMissing(format!(
                "Modelo con SR/n_fft/hop ({}/{}/{}) incompatible con el DSP interno ({}/{}/{})",
                sample_rate,
                n_fft,
                hop,
                crate::df::stft::SR,
                crate::df::stft::FFT_SIZE,
                crate::df::stft::HOP_SIZE
            )));
        }
        if erb_norm_state_size + spec_norm_state_size > state_size {
            return Err(E::EModelMissing(
                "Metadatos de estado inconsistentes (erb_norm + spec_norm > state_size)"
                    .to_string(),
            ));
        }

        Ok(Self {
            session,
            state_size,
            erb_norm_state_size,
            spec_norm_state_size,
        })
    }

    /// Construye el vector de estado inicial a partir de los metadatos del modelo.
    fn initial_state(&self) -> Result<Vec<f32>, E> {
        let erb_init;
        let spec_init;
        {
            let meta = self.session.metadata().map_err(|e| {
                E::EModelMissing(format!("Error leyendo metadatos del modelo: {}", e))
            })?;

            let parse_vec = |key: &str| -> Result<Vec<f32>, E> {
                meta.custom(key)
                    .ok_or_else(|| {
                        E::EModelMissing(format!("Metadato '{}' ausente en el modelo", key))
                    })?
                    .split(',')
                    .map(|v| {
                        v.trim().parse::<f32>().map_err(|_| {
                            E::EModelMissing(format!("Valor inválido en metadato '{}': {}", key, v))
                        })
                    })
                    .collect()
            };

            erb_init = parse_vec("erb_norm_init")?;
            spec_init = parse_vec("spec_norm_init")?;
        }

        if erb_init.len() != self.erb_norm_state_size {
            return Err(E::EModelMissing(format!(
                "erb_norm_init tiene {} valores, esperados {}",
                erb_init.len(),
                self.erb_norm_state_size
            )));
        }
        if spec_init.len() != self.spec_norm_state_size {
            return Err(E::EModelMissing(format!(
                "spec_norm_init tiene {} valores, esperados {}",
                spec_init.len(),
                self.spec_norm_state_size
            )));
        }

        let mut state = vec![0.0_f32; self.state_size];
        state[..erb_init.len()].copy_from_slice(&erb_init);
        let off = self.erb_norm_state_size;
        state[off..off + spec_init.len()].copy_from_slice(&spec_init);
        Ok(state)
    }

    /// Ejecuta la inferencia frame a frame sobre una secuencia de frames STFT,
    /// encadenando el estado recurrente entre frames.
    ///
    /// El grafo es stateful: se alimentan los `T` frames en orden encadenando el
    /// estado y se conservan las `T` salidas, igual que
    /// `OfflineSpeechDenoiserDpdfNetImpl` de sherpa-onnx (un `Run` por frame,
    /// sin descartar salidas; el retardo se compensa solo con el recorte de
    /// la síntesis, ver `stft::ISTFT_HEAD_CROP`).
    ///
    /// `frames`: frames de entrada `[T][NB_BINS]` (complejos).
    /// Devuelve los `T` frames de salida en el mismo orden.
    pub fn infer(&mut self, frames: &[Vec<Complex32>]) -> Result<Vec<Vec<Complex32>>, E> {
        let total = frames.len();
        let mut state = self.initial_state()?;

        let mut out = Vec::with_capacity(total);
        for frame in frames.iter() {
            // Tensor de entrada [1, 1, 481, 2] con el espectro en real/imag.
            let mut spec = vec![0.0_f32; crate::df::stft::NB_BINS * 2];
            for (i, c) in frame.iter().enumerate() {
                spec[2 * i] = c.re;
                spec[2 * i + 1] = c.im;
            }

            let spec_in = Value::from_array(([1usize, 1, frame.len(), 2], spec.clone()))
                .map_err(|e| E::EFfmpegFailed(format!("Error creando tensor spec: {}", e)))?;
            // `Value::from_array` toma possession del buffer, así que reescribimos
            // el estado en un vector nuevo en lugar de moverlo.
            let state_in = Value::from_array(([state.len()], state.clone()))
                .map_err(|e| E::EFfmpegFailed(format!("Error creando tensor state_in: {}", e)))?;

            let outputs = self
                .session
                .run(ort::inputs!["spec" => spec_in, "state_in" => state_in])
                .map_err(|e| E::EFfmpegFailed(format!("Error en inferencia DPDFNet: {}", e)))?;

            //(spec_e, state_out)
            let (_shape_e, spec_e) = outputs[0]
                .try_extract_tensor::<f32>()
                .map_err(|e| E::EFfmpegFailed(format!("Error extrayendo spec_e: {}", e)))?;
            let (_shape_s, state_out) = outputs[1]
                .try_extract_tensor::<f32>()
                .map_err(|e| E::EFfmpegFailed(format!("Error extrayendo state_out: {}", e)))?;

            if state_out.len() != self.state_size {
                return Err(E::EFfmpegFailed(format!(
                    "state_out con {} elementos, esperados {}",
                    state_out.len(),
                    self.state_size
                )));
            }
            state.clear();
            state.extend_from_slice(state_out);

            // Reconstruye el frame complejo de salida.
            let mut out_frame = vec![Complex32::new(0.0, 0.0); frame.len()];
            for i in 0..frame.len() {
                if 2 * i + 1 < spec_e.len() {
                    out_frame[i] = Complex32::new(spec_e[2 * i], spec_e[2 * i + 1]);
                }
            }
            out.push(out_frame);
        }

        Ok(out)
    }
}

/// Cache global thread-safe para la sesión ort.
pub static ORT_CACHE: Mutex<Option<DpdfNetSession>> = Mutex::new(None);
