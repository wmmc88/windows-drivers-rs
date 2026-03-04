# POC-8: Driver Detection Heuristics Validation
# Compiles and runs the standalone Rust detection program against the real
# windows-drivers-rs example drivers. No admin required.

$ErrorActionPreference = 'Stop'

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RsFile    = Join-Path $ScriptDir 'poc-8-detection.rs'
$OutExe    = Join-Path $ScriptDir 'poc-8-detection.exe'
$RepoRoot  = Resolve-Path (Join-Path $ScriptDir '..\..\..')

# Point to the examples directory in the worktree
$ExamplesDir = Join-Path $RepoRoot 'examples'

Write-Host "=== POC-8: Driver Detection Heuristics ===" -ForegroundColor Cyan
Write-Host "Source:   $RsFile"
Write-Host "Examples: $ExamplesDir"
Write-Host ""

# --- Compile ---
Write-Host "[1/2] Compiling..." -ForegroundColor Yellow
& rustc $RsFile -o $OutExe --edition 2021 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: Compilation failed." -ForegroundColor Red
    exit 1
}
Write-Host "  Compiled OK -> $OutExe" -ForegroundColor Green
Write-Host ""

# --- Run ---
Write-Host "[2/2] Running detection..." -ForegroundColor Yellow
$env:EXAMPLES_DIR = $ExamplesDir
& $OutExe
$exitCode = $LASTEXITCODE

Write-Host ""
if ($exitCode -eq 0) {
    Write-Host "POC-8 PASSED" -ForegroundColor Green
} else {
    Write-Host "POC-8 FAILED (exit code $exitCode)" -ForegroundColor Red
}

# Cleanup
if (Test-Path $OutExe) { Remove-Item $OutExe -Force }

exit $exitCode
