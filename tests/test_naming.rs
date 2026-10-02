//! Test suite para CLI y nombrado de salidas.
//!
//! Ver `design.md §10 punto 1`, `specifications.md §2 RF-01..04, RF-10`.
//! Todos estos tests son puros (sin ffmpeg ni red ni modelo ONNX).

use denoise::cli::{
    expand_inputs, is_supported_video, normalize_lexical, resolve_outputs, validate_affix,
};
use denoise::errors::E;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

#[test]
fn test_repo_design_doc_exists() {
    // abre docs/design.md por nombre ASCII — verifica que el repo es accesible.
    let design_path = Path::new("docs/design.md");
    assert!(design_path.exists(), "docs/design.md debe existir");
    let content = fs::read_to_string(design_path).expect("Leer docs/design.md");
    assert!(
        content.contains("Contrato CLI"),
        "docs/design.md §4 debe existir"
    );
}

#[test]
fn test_cargo_toml_exists() {
    let cargo_path = Path::new("Cargo.toml");
    assert!(cargo_path.exists(), "Cargo.toml debe existir");
    let content = fs::read_to_string(cargo_path).expect("Leer Cargo.toml");
    assert!(
        content.contains("1.0.0"),
        "Cargo.toml debe tener version 1.0.0"
    );
}

#[test]
fn test_naming_single_file_default() {
    let cwd = Path::new("/workspace");
    let input = PathBuf::from("/workspace/boda.mp4");
    let resolved = resolve_outputs(
        std::slice::from_ref(&input),
        None,
        None,
        None,
        None,
        false,
        cwd,
        false,
    )
    .expect("Resolución por defecto");

    assert_eq!(resolved.len(), 1);
    assert_eq!(
        resolved[0].output,
        PathBuf::from("/workspace/boda_denoised.mp4")
    );
}

#[test]
fn test_naming_prefix_and_suffix() {
    let cwd = Path::new("/workspace");
    let input = PathBuf::from("/workspace/boda.mp4");
    let resolved = resolve_outputs(
        std::slice::from_ref(&input),
        None,
        None,
        Some("pod-"),
        Some("_clean"),
        false,
        cwd,
        false,
    )
    .expect("Resolución con prefijo y sufijo");

    assert_eq!(resolved.len(), 1);
    assert_eq!(
        resolved[0].output,
        PathBuf::from("/workspace/pod-boda_clean.mp4")
    );
}

#[test]
fn test_naming_output_dir_only() {
    let cwd = Path::new("/workspace");
    let input = PathBuf::from("/workspace/video.mov");
    let out_dir = PathBuf::from("/workspace/limpios");
    let resolved = resolve_outputs(
        std::slice::from_ref(&input),
        None,
        Some(&out_dir),
        None,
        None,
        false,
        cwd,
        false,
    )
    .expect("Resolución con output-dir");

    assert_eq!(resolved.len(), 1);
    assert_eq!(
        resolved[0].output,
        PathBuf::from("/workspace/limpios/video_denoised.mp4")
    );
}

#[test]
fn test_naming_output_name_single_lot() {
    let cwd = Path::new("/workspace");
    let input = PathBuf::from("/workspace/a.mp4");
    let resolved = resolve_outputs(
        std::slice::from_ref(&input),
        Some("final"),
        None,
        None,
        None,
        false,
        cwd,
        false,
    )
    .expect("Resolución con output-name");

    assert_eq!(resolved.len(), 1);
    // auto añade .mp4
    assert_eq!(resolved[0].output, PathBuf::from("/workspace/final.mp4"));
}

#[test]
fn test_naming_output_name_complementary_with_output_dir() {
    // output-name y output-dir son complementarios; prefix/suffix se ignoran
    let cwd = Path::new("/workspace");
    let input = PathBuf::from("/workspace/a.mp4");
    let out_dir = PathBuf::from("/workspace/limpio");
    let resolved = resolve_outputs(
        std::slice::from_ref(&input),
        Some("final"),
        Some(&out_dir),
        Some("pre_"),
        Some("_suf"),
        false,
        cwd,
        false,
    )
    .expect("Resolución complementaria");

    assert_eq!(resolved.len(), 1);
    assert_eq!(
        resolved[0].output,
        PathBuf::from("/workspace/limpio/final.mp4")
    );
}

#[test]
fn test_naming_output_name_lot_greater_than_one_error() {
    // --output-name con lote > 1 da error E_INVALID_INPUT
    let cwd = Path::new("/workspace");
    let inputs = vec![
        PathBuf::from("/workspace/a.mp4"),
        PathBuf::from("/workspace/b.mp4"),
    ];
    let err =
        resolve_outputs(&inputs, Some("final"), None, None, None, false, cwd, false).unwrap_err();

    match err {
        E::EInvalidInput(msg) => {
            assert!(msg.contains("solo se permite cuando el lote"));
        }
        _ => panic!("Esperado EInvalidInput, obtenido: {:?}", err),
    }
}

#[test]
fn test_naming_output_name_self_reference_error() {
    // --output-name resolviendo a la propia entrada da E_INVALID_INPUT exit 2 siempre
    let cwd = Path::new("/workspace");
    let input = PathBuf::from("/workspace/a.mp4");
    let err = resolve_outputs(
        std::slice::from_ref(&input),
        Some("a.mp4"),
        None,
        None,
        None,
        false,
        cwd,
        false,
    )
    .unwrap_err();

    match err {
        E::EInvalidInput(msg) => {
            assert!(msg.contains("resuelve al mismo archivo"));
        }
        _ => panic!("Esperado EInvalidInput, obtenido: {:?}", err),
    }
}

#[test]
fn test_naming_empty_prefix_and_suffix_in_place_error() {
    // prefijo y sufijo vacíos simultáneamente in-place da E_INVALID_INPUT
    let cwd = Path::new("/workspace");
    let input = PathBuf::from("/workspace/a.mp4");
    let err = resolve_outputs(
        std::slice::from_ref(&input),
        None,
        None,
        Some(""),
        Some(""),
        false,
        cwd,
        false,
    )
    .unwrap_err();

    match err {
        E::EInvalidInput(msg) => {
            assert!(msg.contains("Prefijo y sufijo no pueden estar vacíos"));
        }
        _ => panic!("Esperado EInvalidInput, obtenido: {:?}", err),
    }
}

#[test]
fn test_affix_validation_invalid_chars() {
    // solo [A-Za-z0-9._-] y prohibido '.' o '..'
    assert!(validate_affix("valid_prefix-1.0", "prefijo").is_ok());
    assert!(validate_affix(".", "prefijo").is_err());
    assert!(validate_affix("..", "sufijo").is_err());
    assert!(validate_affix("invalid/slash", "prefijo").is_err());
    assert!(validate_affix("invalid*char", "sufijo").is_err());
}

#[test]
fn test_intra_batch_collision_dedup() {
    // 2+ archivos en el lote resolviendo al mismo nombre reciben _1, _2...
    let cwd = Path::new("/workspace");
    let inputs = vec![
        PathBuf::from("/workspace/a.mp4"),
        PathBuf::from("/workspace/a.mov"),
        PathBuf::from("/workspace/a.mkv"),
    ];
    let resolved = resolve_outputs(&inputs, None, None, None, None, false, cwd, false)
        .expect("Deduplicación intra-lote");

    assert_eq!(resolved.len(), 3);
    assert_eq!(
        resolved[0].output,
        PathBuf::from("/workspace/a_denoised.mp4")
    );
    assert_eq!(
        resolved[1].output,
        PathBuf::from("/workspace/a_denoised_1.mp4")
    );
    assert_eq!(
        resolved[2].output,
        PathBuf::from("/workspace/a_denoised_2.mp4")
    );
    assert!(!resolved[0].is_duplicate_adjusted);
    assert!(resolved[1].is_duplicate_adjusted);
    assert!(resolved[2].is_duplicate_adjusted);
}

#[test]
fn test_expansion_and_recursive_tree() {
    let tmp_dir = std::env::temp_dir().join("denoise_test_tree");
    let _ = fs::remove_dir_all(&tmp_dir);
    fs::create_dir_all(tmp_dir.join("sub1/sub2")).unwrap();
    fs::create_dir_all(tmp_dir.join("out_nested")).unwrap();

    File::create(tmp_dir.join("root.mp4")).unwrap();
    File::create(tmp_dir.join("sub1/nested.mov")).unwrap();
    File::create(tmp_dir.join("sub1/sub2/deep.mkv")).unwrap();
    File::create(tmp_dir.join("sub1/already_denoised.mp4")).unwrap(); // excluido
    File::create(tmp_dir.join("out_nested/skip_me.mp4")).unwrap(); // Excluido si out_dir

    let out_dir = tmp_dir.join("out_nested");
    let expanded = expand_inputs(
        std::slice::from_ref(&tmp_dir),
        true,
        Some(&out_dir),
        "_denoised",
        false,
        &tmp_dir,
    )
    .expect("Expansión recursiva");

    let filenames: Vec<String> = expanded
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();

    assert!(filenames.contains(&"root.mp4".to_string()));
    assert!(filenames.contains(&"nested.mov".to_string()));
    assert!(filenames.contains(&"deep.mkv".to_string()));
    // Verificamos exclusiones de ya-procesados y output-dir anidado
    assert!(!filenames.contains(&"already_denoised.mp4".to_string()));
    assert!(!filenames.contains(&"skip_me.mp4".to_string()));

    // recrear árbol relativo a cwd
    let resolved = resolve_outputs(
        &expanded,
        None,
        Some(&out_dir),
        None,
        None,
        true,
        &tmp_dir,
        false,
    )
    .expect("Resolución con recreación de árbol");

    let deep_resolved = resolved
        .iter()
        .find(|r| r.input.ends_with("deep.mkv"))
        .unwrap();

    let expected_deep_out =
        normalize_lexical(&out_dir.join("sub1/sub2/deep_denoised.mp4"), &tmp_dir);
    let actual_deep_out = normalize_lexical(&deep_resolved.output, &tmp_dir);
    assert_eq!(actual_deep_out, expected_deep_out);

    let _ = fs::remove_dir_all(&tmp_dir);
}

#[test]
fn test_is_supported_video() {
    assert!(is_supported_video(Path::new("file.MP4")));
    assert!(is_supported_video(Path::new("file.mov")));
    assert!(is_supported_video(Path::new("file.mkv")));
    assert!(is_supported_video(Path::new("file.webm")));
    assert!(is_supported_video(Path::new("file.avi")));
    assert!(!is_supported_video(Path::new("file.txt")));
    assert!(!is_supported_video(Path::new("file.wav")));
    assert!(!is_supported_video(Path::new("file")));
}
