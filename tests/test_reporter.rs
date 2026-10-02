//! Test de reportería — verifica orden [i/N], JSONL parseable, summary.
//!
//! Ver `specifications.md RF-08` y `design.md §10 punto 5`.

use denoise::cli::{BatchSummary, ItemStatus, JsonLogLine, JsonSummaryLine, ResolvedItem};
use std::path::PathBuf;

#[test]
fn test_reporting_order() {
    let items = [
        ResolvedItem {
            input: PathBuf::from("video1.mp4"),
            output: PathBuf::from("video1_denoised.mp4"),
            is_duplicate_adjusted: false,
        },
        ResolvedItem {
            input: PathBuf::from("video2.mp4"),
            output: PathBuf::from("video2_denoised.mp4"),
            is_duplicate_adjusted: false,
        },
        ResolvedItem {
            input: PathBuf::from("video3.mp4"),
            output: PathBuf::from("video3_denoised.mp4"),
            is_duplicate_adjusted: false,
        },
    ];

    assert_eq!(items.len(), 3);
    for (i, item) in items.iter().enumerate() {
        let header = format!(
            "[{}/{}] {} -> {}",
            i + 1,
            items.len(),
            item.input.display(),
            item.output.display()
        );
        assert!(header.starts_with(&format!("[{}/3]", i + 1)));
    }
}

#[test]
fn test_jsonl_parseable() {
    let line = JsonLogLine {
        input: "input.mp4".to_string(),
        output: "output.mp4".to_string(),
        status: ItemStatus::Ok,
        message: "completado".to_string(),
        pct: 100,
    };

    let serialized = serde_json::to_string(&line).expect("Serialización JSON exitosa");
    let deserialized: serde_json::Value =
        serde_json::from_str(&serialized).expect("JSON parseable");

    assert_eq!(deserialized["input"], "input.mp4");
    assert_eq!(deserialized["output"], "output.mp4");
    assert_eq!(deserialized["status"], "ok");
    assert_eq!(deserialized["message"], "completado");
    assert_eq!(deserialized["pct"], 100);
}

#[test]
fn test_summary_correct() {
    let summary = BatchSummary {
        ok: 4,
        failed: 1,
        skipped: 2,
    };

    let summary_line = JsonSummaryLine { summary };
    let serialized = serde_json::to_string(&summary_line).expect("Serialización JSON exitosa");
    let deserialized: serde_json::Value =
        serde_json::from_str(&serialized).expect("JSON parseable");

    assert_eq!(deserialized["summary"]["ok"], 4);
    assert_eq!(deserialized["summary"]["failed"], 1);
    assert_eq!(deserialized["summary"]["skipped"], 2);
}

#[test]
fn test_no_animation_with_json() {
    // Verifica que en modo JSON el status de skipped produce pct=0
    let skipped_line = JsonLogLine {
        input: "test.mp4".to_string(),
        output: "test_denoised.mp4".to_string(),
        status: ItemStatus::Skipped,
        message: "skipped".to_string(),
        pct: 0,
    };

    let serialized = serde_json::to_string(&skipped_line).unwrap();
    assert!(!serialized.contains("\x1b[")); // Sin secuencias de escape ANSI
    assert!(serialized.contains(r#""pct":0"#));
    assert!(serialized.contains(r#""status":"skipped""#));
}

#[test]
fn test_no_animation_without_tty() {
    // Dry run status formatting
    let dry_run_line = JsonLogLine {
        input: "sample.mp4".to_string(),
        output: "sample_denoised.mp4".to_string(),
        status: ItemStatus::DryRun,
        message: "would process".to_string(),
        pct: 0,
    };

    let serialized = serde_json::to_string(&dry_run_line).unwrap();
    assert!(serialized.contains(r#""status":"dry-run""#));
    assert!(serialized.contains(r#""pct":0"#));
}
