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
//!
//! ## Arquitectura: `DpdfNetSession` + `DpdfStream`
//!
//! - **`DpdfNetSession`** (lógicamente inmutable tras `load()`): sesión ONNX,
//!   tamaño del estado y vector inicial cacheado. Se cachea globalmente en
//!   `ORT_CACHE`. La referencia `&mut` que toma `DpdfStream` es requisito de
//!   `ort::Session::run`, no porque modifique estado de aplicación.
//! - **`DpdfStream`** (mutable, por archivo): estado de ejecución por stream
//!   con buffers pre-allocados (estado recurrente + frame de salida). Una vez
//!   creado con `session.stream()`, el bucle de inferencia no ejecuta ninguna
//!   asignación en heap: el espectro va en stack (~3,8 KB), los tensores de
//!   entrada son vistas prestadas (`TensorRef`), y la salida se escribe en el
//!   buffer reutilizado del stream.

use crate::errors::E;
use crate::models::ModelPaths;
use ort::session::Session;
use ort::value::TensorRef;
use rustfft::num_complex::Complex32;
use std::sync::Mutex;

// ─── Sesión (lógicamente inmutable) ──────────────────────────────────────────

/// Sesión ONNX Runtime CPU con el estado inicial cacheado.
///
/// Tras `load()`, ningún campo de aplicación cambia. `ort::Session::run`
/// requiere `&mut self` por su API interna, pero no altera estado observable.
/// Para procesar audio, crear un `DpdfStream` vía `session.stream()` que
/// encapsula todo el estado mutable por archivo.
pub struct DpdfNetSession {
    session: Session,
    state_size: usize,
    /// Estado inicial cacheado: parseado una sola vez en `load()` para evitar
    /// parsear los metadatos CSV en cada llamada.
    cached_initial_state: Vec<f32>,
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
            cached_initial_state,
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

            let state_size = read_usize("state_size")?;
            let erb_norm_state_size = read_usize("erb_norm_state_size")?;
            let spec_norm_state_size = read_usize("spec_norm_state_size")?;
            let freq_bins = read_usize("freq_bins")?;
            let n_fft = read_usize("n_fft")?;
            let hop = read_usize("hop_length")?;
            let sample_rate = read_usize("sample_rate")?;

            // Parsear el estado inicial una sola vez y cachearlo en el struct.
            // En lotes de N vídeos evita N parseos de CSV; en uso individual
            // es equivalente al código anterior.
            let erb_init = parse_vec("erb_norm_init")?;
            let spec_init = parse_vec("spec_norm_init")?;

            if erb_init.len() != erb_norm_state_size {
                return Err(E::EModelMissing(format!(
                    "erb_norm_init tiene {} valores, esperados {}",
                    erb_init.len(),
                    erb_norm_state_size
                )));
            }
            if spec_init.len() != spec_norm_state_size {
                return Err(E::EModelMissing(format!(
                    "spec_norm_init tiene {} valores, esperados {}",
                    spec_init.len(),
                    spec_norm_state_size
                )));
            }

            let mut init = vec![0.0_f32; state_size];
            init[..erb_init.len()].copy_from_slice(&erb_init);
            init[erb_norm_state_size..erb_norm_state_size + spec_init.len()]
                .copy_from_slice(&spec_init);

            (
                state_size,
                erb_norm_state_size,
                spec_norm_state_size,
                freq_bins,
                n_fft,
                hop,
                sample_rate,
                init,
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
            cached_initial_state,
        })
    }

    /// Crea un `DpdfStream` con los buffers pre-allocados para procesar un
    /// archivo de audio. El stream encapsula todo el estado mutable por archivo:
    /// el buffer del estado recurrente y el frame de salida pre-allocado.
    ///
    /// Toma `&mut self` porque `ort::Session::run` lo requiere, no porque
    /// modifique estado de aplicación.
    ///
    /// El bucle de inferencia sobre el stream no ejecuta asignaciones en heap.
    pub fn stream(&mut self) -> DpdfStream<'_> {
        let initial_state = self.cached_initial_state.clone();
        DpdfStream {
            session: self,
            state_buf: initial_state,
            out_frame: vec![Complex32::new(0.0, 0.0); crate::df::stft::NB_BINS],
        }
    }
}

// ─── Stream de ejecución por archivo ─────────────────────────────────────────

/// Estado de ejecución mutable por archivo de audio.
///
/// Creado vía `DpdfNetSession::stream()`, mantiene el estado recurrente del
/// modelo y el buffer de salida pre-allocado. Durante el bucle de inferencia
/// frame a frame, `process_frame()` no ejecuta ninguna asignación en heap:
///
/// - El espectro de entrada va en un array en stack (~3,8 KB).
/// - Los tensores de entrada son vistas prestadas (`TensorRef`, zero-copy).
/// - El estado recurrente se actualiza in-place vía `copy_from_slice`.
/// - El frame de salida se escribe en `out_frame` reutilizado.
pub struct DpdfStream<'s> {
    session: &'s mut DpdfNetSession,
    /// Buffer del estado recurrente del modelo (90228 floats = ~352 KB).
    /// Se inicializa con `cached_initial_state` y se actualiza in-place
    /// en cada llamada a `process_frame()`.
    state_buf: Vec<f32>,
    /// Buffer pre-allocado para el frame de salida (481 bins complejos).
    /// `process_frame()` escribe aquí y devuelve una referencia de lectura;
    /// el llamador copia los datos al `Vec<Vec<Complex32>>` de salida con
    /// una sola operación `to_vec()` o `clone_from_slice()`.
    out_frame: Vec<Complex32>,
}

impl<'s> DpdfStream<'s> {
    /// Procesa un único frame STFT y devuelve una referencia al frame de salida.
    ///
    /// **Zero heap allocations**: el espectro va en stack, los tensores son
    /// vistas prestadas, el estado se actualiza in-place y la salida se escribe
    /// en el buffer pre-allocado `out_frame`.
    ///
    /// El llamador debe copiar `&[Complex32]` al destino antes de la siguiente
    /// llamada, ya que `process_frame` sobrescribe `out_frame`.
    pub fn process_frame(&mut self, frame: &[Complex32]) -> Result<&[Complex32], E> {
        // Tensor de entrada [1, 1, 481, 2] con el espectro en real/imag en stack (~3,8 KB).
        let mut spec = [0.0_f32; crate::df::stft::NB_BINS * 2];
        for (i, c) in frame.iter().enumerate() {
            spec[2 * i] = c.re;
            spec[2 * i + 1] = c.im;
        }

        let spec_in = TensorRef::from_array_view(([1usize, 1, frame.len(), 2], spec.as_slice()))
            .map_err(|e| E::EFfmpegFailed(format!("Error creando tensor spec: {}", e)))?;

        // Vista prestada del estado (zero-copy): sin clonación de 352 KB por frame.
        let state_in =
            TensorRef::from_array_view(([self.state_buf.len()], self.state_buf.as_slice()))
                .map_err(|e| {
                    E::EFfmpegFailed(format!("Error creando tensor state_in: {}", e))
                })?;

        let outputs = self
            .session
            .session
            .run(ort::inputs!["spec" => spec_in, "state_in" => state_in])
            .map_err(|e| E::EFfmpegFailed(format!("Error en inferencia DPDFNet: {}", e)))?;

        let (_shape_e, spec_e) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| E::EFfmpegFailed(format!("Error extrayendo spec_e: {}", e)))?;
        let (_shape_s, state_out) = outputs[1]
            .try_extract_tensor::<f32>()
            .map_err(|e| E::EFfmpegFailed(format!("Error extrayendo state_out: {}", e)))?;

        if state_out.len() != self.session.state_size {
            return Err(E::EFfmpegFailed(format!(
                "state_out con {} elementos, esperados {}",
                state_out.len(),
                self.session.state_size
            )));
        }

        // Actualizar estado in-place (sin clear + extend, que podría reasignar).
        self.state_buf.copy_from_slice(state_out);

        // Escribir el frame de salida en el buffer pre-allocado.
        for i in 0..frame.len() {
            if 2 * i + 1 < spec_e.len() {
                self.out_frame[i] = Complex32::new(spec_e[2 * i], spec_e[2 * i + 1]);
            } else {
                self.out_frame[i] = Complex32::new(0.0, 0.0);
            }
        }
        Ok(&self.out_frame[..frame.len()])
    }
}

/// Cache global thread-safe para la sesión ort.
pub static ORT_CACHE: Mutex<Option<DpdfNetSession>> = Mutex::new(None);

