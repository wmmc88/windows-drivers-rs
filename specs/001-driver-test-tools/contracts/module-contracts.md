# Module Contracts

Version: 0.1.0
Status: Draft (Phase 1)

## vm
Trait: `VmProvider`
```rust
trait VmProvider {
    fn create_vm(&self, cfg: &VmCreateSpec) -> Result<TestVm, VmError>; // Idempotent: returns existing if present
    fn get_vm(&self, name: &str) -> Result<Option<TestVm>, VmError>;    // No side effects
    fn snapshot_vm(&self, vm: &TestVm, label: &str) -> Result<SnapshotId, VmError>; // Creates or returns existing
    fn revert_snapshot(&self, vm: &TestVm, snap: &SnapshotId) -> Result<(), VmError>; // Requires vm stopped
    fn execute(&self, vm: &TestVm, ps_script: &str) -> Result<CommandOutput, VmError>; // PowerShell Direct
    fn copy_file(&self, vm: &TestVm, host_src: &Path, guest_dest: &str) -> Result<(), VmError>; // Creates path
}
```
Invariants:
- `execute` must surface stderr separately.
- `copy_file` must verify source existence before invocation.
- Snapshot names are immutable once created.

Errors:
- `VmError::NotFound` → treat as user error (exit code 1)
- `VmError::Ps` → system failure (exit code 2)

## driver_detect
```rust
trait DriverDetector {
    fn detect(&self, root: &Path, override_type: Option<DriverTypeOverride>) -> Result<DriverPackage, DetectionError>;
    fn companion_app(&self, root: &Path) -> Result<Option<PathBuf>, DetectionError>; // echo.exe or similar
}
```
Invariants:
- Must prefer explicit metadata over inference.
- INF parsing case-insensitive.
- Override always wins but logged as warning.

Errors:
- `DetectionError::NotFound` → user error.

## deploy
```rust
trait Deployer {
    fn ensure_certificate(&self, vm: &TestVm, cert: &Path) -> Result<(), DeployError>; // Idempotent
    fn install_driver(&self, vm: &TestVm, inf: &Path) -> Result<(), DeployError>; // Uses pnputil /add-driver
    fn verify_loaded(&self, vm: &TestVm, expected_version: &str) -> Result<bool, DeployError>; // PnP query
}
```
Invariants:
- Certificate installation sets both Trusted Root + Publishers.
- Verification returns false (not error) if version mismatch.

## debug
```rust
trait DebugCapture {
    fn start(&mut self, vm: &TestVm, log_path: &str) -> Result<CaptureHandle, DebugError>;
    fn tail(&mut self, handle: &CaptureHandle) -> Result<Vec<DebugMessage>, DebugError>; // Non-blocking batch
    fn stop(&mut self, handle: CaptureHandle) -> Result<(), DebugError>;
}
```
Invariants:
- `start` creates directory if missing.
- Messages tagged with monotonic increment if timestamp missing.

## echo_test
```rust
trait EchoTester {
    fn run_basic(&self, vm: &TestVm, app: &Path) -> Result<EchoResult, EchoError>; // Single round trip
    fn run_stress(&self, vm: &TestVm, app: &Path, count: u32) -> Result<EchoStressResult, EchoError>; // Many loops
}
```
Invariants:
- Stress test must abort on first failure but report count completed.

## output
Formatter abstraction (human vs JSON)
```rust
trait OutputFormatter {
    fn emit_result(&self, res: &TestRunResult) -> anyhow::Result<()>;
    fn emit_error(&self, err: &AppError) -> anyhow::Result<()>;
}
```

## errors
Unified error taxonomy: each module returns domain error; top-level converts to `AppError { code, phase, message }`.

## ps
```rust
trait PowerShellHost {
    fn run_json(&self, script: &str, timeout: std::time::Duration) -> Result<serde_json::Value, PsError>;
}
```
Timeout invariant: must kill process if exceeded and return `PsError::Timeout`.

## Concurrency & Parallelism
- Phase 0 design chooses synchronous; traits are future-compatible with async conversions.
- Any future parallel driver deployment must wrap VmProvider with a safe queue to limit simultaneous PS sessions.

## Logging Requirements
- Each public method starts a tracing span: `vm.create`, `driver.detect`, `deploy.install`, etc.
- Errors include fields: `error.kind`, `error.detail`.

## Stability
- Trait method removal → breaking change; add new methods with default blanket impl (if needed) to preserve API.
