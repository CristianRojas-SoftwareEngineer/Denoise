/// Módulos públicos de `denoise` — lib accesible desde tests de integración.
pub mod cli;
pub mod df;
pub mod errors;
pub mod ffmpeg_io;
pub mod models;
pub mod pipeline;
pub mod preset;

// Re-exporta la abstracción canónica de medios.
pub use ffmpeg_io::{MediaKind, ProbeResult};

// Re-exporta los tipos canónicos de presets de calidad.
pub use preset::{EgressParams, Preset};

// Re-exporta el binario principal para `cargo run -- --help` equivalente.
pub fn run() -> anyhow::Result<()> {
    cli::run()
}
