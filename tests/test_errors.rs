//! Test de errores — verifica códigos de salida y tipos de error sin DSP.
//!
//! Ver `specifications.md §2-4` y `design.md §10 punto 4`.
//! Requiere `ffmpeg 6+` real (para probe), `FakeProvider` sin red/modelo.

#[test]
fn test_no_audio_error() {
    // TODO: video sin audio → E_NO_AUDIO
}

#[test]
fn test_invalid_input_error() {
    // TODO: ruta inexistente / extensión no soportada → E_INVALID_INPUT
}

#[test]
fn test_ffmpeg_not_found() {
    // TODO: ffmpeg ausente → E_FFMPEG_NOT_FOUND
}

#[test]
fn test_model_missing() {
    // TODO: modelo corrupto/incompleto → E_MODEL_MISSING
}

#[test]
fn test_output_exists() {
    // TODO: destino existente sin --overwrite → E_OUTPUT_EXISTS
}

#[test]
fn test_output_exists_skip_existing() {
    // TODO: --skip-existing → skipped, exit 0
}

#[test]
fn test_invalid_bitrate() {
    // TODO: bitrate fuera de rango → E_INVALID_INPUT
}

#[test]
fn test_output_name_lot_multi_error() {
    // TODO: --output-name con lote>1 → E_INVALID_INPUT (D20)
}

#[test]
fn test_output_name_self_reference_error() {
    // TODO: --output-name resolviendo a entrada sin prefix/suffix → E_INVALID_INPUT (D_p)
}

#[test]
fn test_io_error() {
    // TODO: directorio no creable → E_IO
}
