//! Test de reportería — verifica orden [i/N], JSONL parseable, summary.
//!
//! Ver `specifications.md RF-08` y `design.md §10 punto 5`.

#[test]
fn test_reporting_order() {
    // TODO: lote 3 simulado verifica orden [1/3..3/3]
}

#[test]
fn test_jsonl_parseable() {
    // TODO: JSONL parseable con jq
}

#[test]
fn test_summary_correct() {
    // TODO: summary con ok/failed/skipped
}

#[test]
fn test_no_animation_with_json() {
    // TODO: --json desactiva animación
}

#[test]
fn test_no_animation_without_tty() {
    // TODO: sin TTY: líneas de porcentaje sin animación
}
