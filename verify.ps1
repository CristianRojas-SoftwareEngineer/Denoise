#!/usr/bin/env pwsh
# verify.ps1 — build + test rápido → PASS/FAIL (D_s cerrado 2026-09-13)
# Sin Python. Script local para Windows (PowerShell).
#
# Uso: .\verify.ps1
# Ver `docs/specifications.md §6 DoD` y `docs/plan.md T0.5`.

$ErrorActionPreference = "Stop"

function Test-Step($message) {
    Write-Host "=== $message ===" -ForegroundColor Cyan
}

function Test-Pass($message) {
    Write-Host "PASS: $message" -ForegroundColor Green
}

function Test-Fail($message) {
    Write-Host "FAIL: $message" -ForegroundColor Red
    exit 1
}

# Paso 1: cargo build --release
Test-Step "cargo build --release"
try {
    cargo build --release 2>&1
    if ($LASTEXITCODE -ne 0) { Test-Fail "cargo build --release" }
} catch {
    Test-Fail "cargo build --release: $_"
}
Test-Pass "cargo build --release"

# Paso 2: cargo test (sin --ignored)
Test-Step "cargo test"
try {
    cargo test 2>&1
    if ($LASTEXITCODE -ne 0) { Test-Fail "cargo test" }
} catch {
    Test-Fail "cargo test: $_"
}
Test-Pass "cargo test"

Write-Host ""
Write-Host "verify.ps1: PASS" -ForegroundColor Green
exit 0
