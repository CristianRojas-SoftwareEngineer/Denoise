//! Test remux `#[ignore]` — verifica que el remux preserva video sin re-encode.
//!
//! Ver `specifications.md RF-05B` y `design.md §10 punto 3`.
//! Ejecutar con `cargo test -- --ignored`.

#[test]
#[ignore]
fn test_remux_copy_video() {
    // TODO: fixture ffmpeg -y -v error -f lavfi -i testsrc=... -f lavfi -i sine=...
    // → mismo vcodec/res/fps, duración ±0.2s, sin re-encode (extradata igual)
}

#[test]
#[ignore]
fn test_remux_duracion_contenedor() {
    // TODO: verificar que <dur_video> = duración del CONTENEDOR (D_k)
    // Si audio más largo que video, salida se alarga al contenedor
}
