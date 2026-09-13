//! Test canario D41 — verifica que el repo compila y el test puede abrir `docs/design.md` por nombre ASCII.
//!
//! Ver `plan.md T0.5` y `specifications.md RNF-01`.
//! Este test NO requiere ffmpeg ni modelo ONNX.

#[test]
fn test_repo_design_doc_exists() {
    // D41: abre docs/design.md por nombre ASCII — verifica que el repo es accesible.
    let design_path = std::path::Path::new("docs/design.md");
    assert!(
        design_path.exists(),
        "docs/design.md debe existir (D41 test canario)"
    );
    let content = std::fs::read_to_string(design_path).expect("Leer docs/design.md");
    assert!(
        content.contains("Contrato CLI"),
        "docs/design.md §4 debe existir"
    );
}

#[test]
fn test_cargo_toml_exists() {
    let cargo_path = std::path::Path::new("Cargo.toml");
    assert!(cargo_path.exists(), "Cargo.toml debe existir");
    let content = std::fs::read_to_string(cargo_path).expect("Leer Cargo.toml");
    assert!(
        content.contains("1.0.0"),
        "Cargo.toml debe tener version 1.0.0"
    );
}
