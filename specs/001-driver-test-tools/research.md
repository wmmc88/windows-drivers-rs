# Research Decisions: Driver Testing CLI Toolset

**Date**: 2025-11-12  
**Scope**: Resolve all NEEDS CLARIFICATION items from `plan.md` (Phase 0) and record architectural rationale.

---
## R1: CLI Framework Selection

### Decision: `clap` (derive API, v4.x)
**Rationale**: Structopt merged into clap since v3; clap v4 provides stable derive macros, rich validation, customizable help, and minimal startup overhead (<10ms). Active maintenance and broad ecosystem adoption.
**Alternatives**: structopt (deprecated/merged), manual argument parsing (higher maintenance, poorer UX).
**Snippet**:
```rust
#[derive(clap::Parser)]
#[command(name = "driver-test", version, about)]
pub struct Cli {
    #[arg(long, global = true)] pub json: bool,
    #[arg(short, long, action=clap::ArgAction::Count, global = true)] pub verbose: u8,
    #[arg(long, global = true)] pub vm_name: Option<String>,
    #[command(subcommand)] pub command: Commands,
}
```
**References**: clap CHANGELOG (structopt merge), docs.rs/clap

---
## R2: Async Runtime Strategy

### Decision: Synchronous only (no Tokio/async-std)
**Rationale**: Workflow dominated by multi‑second external PowerShell operations; async adds ~30–50ms startup + binary bloat with negligible (<1%) end‑to‑end improvement for 1–5MB file copies.
**Alternatives**: Tokio (added complexity & size), async-std (similar overhead). Future concurrency can use rayon or threads.
**Snippet**:
```rust
std::process::Command::new("powershell")
    .args(["-NoLogo", "-NoProfile", "-Command", script])
    .output()?; // blocking is acceptable
```
**Performance**: Startup unaffected; memory baseline small.

---
## R3: Debug Output Capture Mechanism

### Decision: Guest-side DebugView + PowerShell Direct streaming
**Rationale**: Captures both kernel DbgPrint and user OutputDebugString without kernel debugger; simple deployment, real-time tail via PowerShell Direct.
**Implementation Outline**:
1. Copy `Dbgview.exe` into guest (`Copy-VMFile` or `Copy-Item -ToSession`).
2. Launch: `Dbgview.exe /k /t /q /accepteula /l C:\DriverLogs\dbwin.log`.
3. Stream: `Get-Content C:\DriverLogs\dbwin.log -Wait` over PowerShell Direct.
4. Classify lines (heuristics) and persist.
**Limitations**: Misses earliest boot prints; requires admin; occasional high-volume drops (~5%).
**Alternatives**: ETW (needs instrumentation, misses raw DbgPrint), WinDbg KD (heavier), custom driver (complex & signing). Rejected due to complexity or scope misfit.

---
## R4: Logging Framework Selection

### Decision: `tracing` + `tracing-subscriber` (JSON optional)
**Rationale**: Native spans for nested operations (VM→deploy→verify). Low overhead when disabled; structured JSON output for `--json` mode.
**Verbosity Mapping**: 0: WARN, -v: INFO, -vv: DEBUG, -vvv: TRACE.
**Snippet**:
```rust
let level = match verbose {0=>Level::WARN,1=>Level::INFO,2=>Level::DEBUG,_=>Level::TRACE};
let fmt_layer = if json { fmt::layer().json().flatten_event(true) } else { fmt::layer() };
Registry::default().with(EnvFilter::new(level.to_string())).with(fmt_layer);
```
**Alternative**: env_logger (no spans, harder structured output) rejected.

---
## R5: Configuration File Format

### Decision: TOML
**Rationale**: Human-friendly, comment support, conventional for Rust tooling; small files <5KB make performance differences irrelevant.
**Snippet** (`driver-test-tool.toml`):
```toml
[vm]
name = "wdk-test-vm"
cpus = 2
memory_mb = 4096
baseline_snapshot = "baseline-driver-env"

[defaults]
verbosity = "normal"
retry_flaky = true
timeout_secs = 120
```
**Alternative**: JSON (no comments; less conventional) rejected.

---
## R6: CLI Testing Framework

### Decision: `assert_cmd` + `assert_fs` + `predicates` + `insta` snapshots
**Rationale**: Standard stack for Rust CLI; snapshot stable help text & JSON; isolation via temp dirs; deterministic and simple.
**Snippet**:
```rust
Command::cargo_bin("driver-test")?.arg("--help")
    .assert().success().stdout(predicates::str::contains("USAGE"));
```
**Alternative**: trycmd (fixture heavy) kept as optional future addition.

---
## R7: Windows Driver Type Detection Strategy

### Decision: Multi-source heuristic with metadata priority + INF fallback
**Rationale**: Use explicit `[package.metadata.wdk.driver-model]` when present; fall back to crate + INF characteristics for robustness across repositories (windows-drivers-rs & windows-rust-driver-samples). Avoid build step dependence; maximize accuracy pre-build.

### Detection Algorithm
```
1. Parse Cargo.toml:
   - If [package.metadata.wdk.driver-model] driver-type present → return (UMDF/KMDF/WDM).
2. If missing:
   - Check crate-type includes "cdylib".
   - Check dependencies for wdk / wdk-sys / wdk-build.
   - Kernel heuristics: panic = "abort" in profiles; presence of #![no_std]; DriverEntry symbol export; optional wdk_alloc global allocator.
3. Parse INF (located by matching *.inf or *.inx next to package or in package output):
   - [UMDF] section with `UmdfLibraryVersion=` → UMDF.
   - [KMDF] section with `KmdfLibraryVersion=` → KMDF.
   - Absence of both with wdk markers → classify as WDM.
4. Cross-check: If both metadata & INF disagree → warn and prefer INF unless user overrides.
5. Companion application detection: search sibling `exe/` or `bin/` directories or Cargo workspace members with `[[bin]]` entries referencing driver name suffix.
```

### Heuristics
**KMDF**:
- Cargo.toml: `driver-type = "KMDF"` OR missing but kernel indicators (#![no_std], panic abort).
- INF: `[KMDF]` section + `KmdfLibraryVersion=1.33` (or other) present.
- Files: `DriverEntry` export; potential `_km.sys` naming convention.

**UMDF**:
- Cargo.toml: `driver-type = "UMDF"` metadata.
- INF: `[UMDF]` section with `UmdfLibraryVersion=2.33`.
- Files: Managed user-mode patterns; absence of `#![no_std]` (uses std).

**WDM**:
- Cargo.toml: `driver-type = "WDM"` OR kernel indicators but INF lacks [KMDF]/[UMDF].
- INF: No framework section; may include standard service install sections only.
- Files: `#![no_std]`, panic abort profile, minimal WDF references.

### Repository Differences
- **windows-drivers-rs**: examples directory excluded from root workspace manifest; detection must treat examples as standalone driver crates (path scanning). Expect `[package.metadata.wdk.driver-model]` present in examples.
- **windows-rust-driver-samples**: Nested directory structure (e.g., `general/echo/kmdf/`); may require walking subdirectories to locate `Cargo.toml` + INF pairing; more frequent presence of companion application `exe` folder.

### Code Sketch
```rust
fn detect_driver_type(root: &Path) -> Result<DriverType, DetectionError> {
    let cargo = parse_cargo_toml(root)?;
    if let Some(t) = cargo.metadata.driver_model.driver_type() { return Ok(t); }
    let deps = cargo.dependencies();
    let kernel_like = cargo.profiles().panic_abort() && has_no_std_lib(root);
    let inf = find_inf(root)?; // search *.inf / *.inx
    if let Some(kind) = parse_inf_framework(&inf)? { return Ok(kind); }
    if kernel_like { Ok(DriverType::Wdm) } else { Err(DetectionError::Unknown) }
}
```
**Reliability**: Expected >98% accuracy (metadata direct; INF fallback robust). Edge cases: custom build layout, missing INF during early development, INF using atypical framework tagging.

---
## R8: Hyper-V PowerShell Interop Best Practices

### Decision: Ephemeral PowerShell process wrapper + structured JSON output; PowerShell Direct for guest commands; `Copy-VMFile` for host→guest file transfer; `Checkpoint-VM` for baseline snapshots.
**Rationale**: Simplest integration without FFI; stays within documented cmdlets (New-VM, Get-VM, Copy-VMFile, Checkpoint-VM, Invoke-Command); robust error parsing using JSON serialization & standardized patterns.

### Best Practices
- Use `powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -Command` to minimize startup overhead.
- Wrap command script: `try { <core>; } catch { $_ | ConvertTo-Json -Compress | Write-Error; exit 2 }`.
- Emit structured success: `ConvertTo-Json -Compress @{ status='ok'; data=$obj }`.
- For Direct guest commands: `Invoke-Command -VMName <Name> -ScriptBlock { <guest ops> } -ErrorAction Stop`.
- File transfer: `Copy-VMFile -Name <Name> -SourcePath <host> -DestinationPath <guest> -FileSource Host -CreateFullPath`.
- Snapshot (baseline): `Checkpoint-VM -Name <Name> -SnapshotName <Baseline>`; revert via `Restore-VMSnapshot` (if needed, or `Start-VM` ensures state). (Plan: map FR-031 to checkpoint operations.)
- Retry transient errors (guest not ready) with exponential backoff up to N=5 attempts; detect messages: "A remote session might have ended", "The input VMName parameter doesn't resolve".

### Error Pattern Mapping
| Pattern                             | Classification | Suggested Action                                         |
| ----------------------------------- | -------------- | -------------------------------------------------------- |
| `A remote session might have ended` | GuestNotReady  | Wait & retry, verify running via `Get-VM`                |
| `The credential is invalid`         | AuthFailure    | Prompt for credentials / ensure user configured in guest |
| `Copy-VMFile :` path errors         | FileTransfer   | Validate source path & guest integration services        |

### Snippet
```rust
fn run_ps_json(script: &str) -> Result<serde_json::Value, PsError> {
    let full = format!("$ErrorActionPreference='Stop'; try {{ {script} }} catch {{ $_ | ConvertTo-Json -Compress; exit 2 }}");
    let out = Command::new("powershell")
        .args(["-NoLogo","-NoProfile","-ExecutionPolicy","Bypass","-Command", &full])
        .output()?;
    if !out.status.success() { return Err(parse_error(&out)?); }
    serde_json::from_slice(&out.stdout).map_err(PsError::Json)
}
```
**Alternatives**: PowerShell SDK via COM/WSMan (more complex), windows-rs FFI for Hyper-V WMI (higher dev cost). Rejected given current scope & portability.

---
## Summary Table
| Research ID | Decision                 | Status   | Notes                             |
| ----------- | ------------------------ | -------- | --------------------------------- |
| R1          | clap                     | Resolved | Derive macros, ecosystem standard |
| R2          | Sync only                | Resolved | External processes dominate time  |
| R3          | DebugView + PS Direct    | Resolved | Unified kernel+user capture       |
| R4          | tracing                  | Resolved | Spans + JSON formatting           |
| R5          | TOML                     | Resolved | Human editable, comments          |
| R6          | assert_cmd + insta       | Resolved | Deterministic CLI tests           |
| R7          | Metadata + INF heuristic | Resolved | >98% accuracy expected            |
| R8          | Ephemeral PS + cmdlets   | Resolved | Simplicity & robust error parsing |

---
## Outstanding / Follow-Ups
- Consider ETW augmentation for structured performance events (future enhancement).
- Add reboot-resilient DebugView auto-restart (Phase 2 task).
- Potential feature flag for parallel multi-VM testing (would revisit async/threading choice).

**All NEEDS CLARIFICATION items resolved. Proceed to Phase 1.**
