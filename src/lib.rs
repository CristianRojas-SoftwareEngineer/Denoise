/// Módulos públicos de `denoise` — lib accesible desde tests de integración.
pub mod cli;
pub mod df;
pub mod errors;
pub mod ffmpeg_io;
pub mod models;
pub mod pipeline;

// Re-exporta el binario principal para `cargo run -- --help` equivalente.
pub fn run() -> anyhow::Result<()> {
    cli::run()
}
