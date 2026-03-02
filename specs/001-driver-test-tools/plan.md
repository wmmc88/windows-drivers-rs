# Implementation Plan: Driver Testing CLI Toolset

**Feature Branch**: `001-driver-test-tools`  
**Specification**: [spec.md](./spec.md)  
**Created**: November 12, 2025  
**Status**: Phase 1 Completed (detection, VM lifecycle, deployment, debug capture, echo tests, CLI integration)

## Executive Summary

Building a comprehensive Rust CLI toolset for automated Windows driver testing on Hyper-V VMs. The tool automates the complete workflow: VM provisioning, driver package detection and deployment, certificate installation, version verification, PnP validation, and debug output capture for KMDF, UMDF, and WDM drivers.

**Core Value**: Reduce driver testing iteration time from manual multi-step process to single-command automation, targeting <5min deployment + verification cycles.

## Technical Context

### Technology Stack

- **Language**: Rust (stable channel, edition 2021)
- **CLI Framework**: clap (derive) — see R1 in `research.md`
- **Hyper-V Integration**: PowerShell Direct via Windows Management APIs
- **Async Runtime**: Synchronous (no async runtime; external processes dominate) — R2
- **Error Handling**: thiserror for library errors, anyhow acceptable for application layer
- **Logging**: tracing + tracing-subscriber (JSON optional) — R4
- **Configuration**: TOML — R5
- **Testing**: assert_cmd + predicates + assert_fs + insta snapshots — R6

### External Dependencies

- **Windows SDK**: For PnP manager queries, device enumeration
- **Hyper-V PowerShell Module**: For VM lifecycle management
- **PowerShell Direct**: For in-guest command execution (requires Hyper-V host access)
- **Cargo Build System**: For detecting and building driver packages
- **Windows Certificate Store APIs**: For test certificate installation
- **Debug Output Capture**: Guest DebugView + PowerShell Direct tail — R3

### Integration Points

1. **Cargo Ecosystem**: Parse Cargo.toml, detect driver crate metadata, invoke cargo-wdk
2. **Hyper-V Management**: Create/configure/snapshot VMs, file transfer via PowerShell Direct
3. **Windows Driver Framework**: Install drivers via devcon/pnputil, query PnP manager state
4. **Windows Certificate Store**: Install/verify test signing certificates
5. **Debug Monitoring**: Capture DbgPrint (kernel) and OutputDebugString (user-mode) output
6. **Repository Structure Detection**: Adapt to windows-drivers-rs vs Windows-Rust-driver-samples layouts

### Architectural Decisions

**CLI Structure**: Multi-command interface (test, setup, clean, snapshot) with global options
**VM Lifecycle**: Hybrid persistent VM with baseline snapshot (per FR-031); default reuse, optional revert/rebuild
**Execution Channel**: PowerShell Direct only (per FR-032); fail fast if unavailable
**Configuration Storage**: Local config file + VM metadata persisted in Hyper-V VM notes
**Error Recovery**: Preserve VM state on failures; provide diagnostic commands

## Constitution Check

### Principle I: Rust Idiomatic Code Quality ✅

**Compliance Plan**:
- All CLI parsing via idiomatic clap derive macros with builder patterns
- Error types use thiserror for structured errors with context
- Public API follows RFC 430: snake_case functions, PascalCase types, SCREAMING_SNAKE_CASE constants
- Zero `unwrap()` in library code; `?` operator for error propagation
- All external dependencies audited via cargo-audit in CI

**Risk Areas**:
- Windows FFI boundaries require unsafe code → Mitigation: Isolated unsafe modules with SAFETY comments
- PowerShell interop via process execution → Mitigation: Typed wrappers with Result returns

### Principle II: Test-First Development ✅

**Compliance Plan**:
- Unit tests for each domain module (detection, deployment, verification) before implementation
- Integration tests for full workflows using temporary test VMs
- Contract tests for PowerShell Direct communication layer
- Mock Hyper-V interactions for fast unit testing; real VM tests in CI
- Target: 80% coverage for core paths (VM lifecycle, driver deployment), 60% overall

**Test Strategy**:
- Phase 0: Research findings → documented in research.md with validation criteria
- Phase 1: Data model → unit tests for entity validation rules before implementation
- Phase 2: CLI commands → assert_cmd tests before implementation
- Phase 3: Integration → end-to-end tests on actual test VMs

### Principle III: User Experience Consistency ✅

**Compliance Plan**:
- Comprehensive --help for all commands with examples
- Structured errors with suggested actions (e.g., "Hyper-V not enabled → run Enable-WindowsOptionalFeature")
- JSON output mode for CI/automation via --json flag
- Progress indicators for VM creation (>15 min), file transfer, driver installation
- Exit codes: 0 (success), 1 (user error: missing files, invalid config), 2 (system error: Hyper-V failure)
- Verbose modes: -v (info), -vv (debug), -vvv (trace) for troubleshooting

**UX Validation**:
- Manual smoke testing of error scenarios (missing Hyper-V, corrupt driver package)
- Help text reviewed for clarity and actionability

### Principle IV: Performance & Reliability Standards ✅

**Compliance Plan**:
- Startup time: <200ms for command parsing and config load (lazy VM enumeration)
- Memory: <50MB baseline, <200MB during file transfer operations
- Idempotent operations: VM creation checks existence, driver installation checks version
- Configurable timeouts: VM operations default 5min, file transfer 30s/MB
- Structured logging via tracing for all operations with span contexts
- Graceful degradation: Continue with warnings if debug output capture unavailable

**Performance Targets** (from Success Criteria):
- SC-001: Build to loaded verification in <5min (excludes VM creation)
- SC-002: First-time VM creation <15min
- SC-006: Driver-app interaction testing <3min after VM ready
- SC-010: Complete validation cycle <10min

### Gate Evaluation

**Pre-Implementation Gates**:
- ✅ All unknowns resolvable via research (no blocking technical constraints)
- ✅ Constitution principles applicable (standard Rust CLI application)
- ⚠️ External dependency risk: Hyper-V PowerShell module API stability (mitigation: version pinning in docs)

**No Constitution Violations Detected**: Proceed to Phase 0 research.

## Phase 0: Research & Technical Discovery (Completed)

### Research Tasks (Historical Record)

#### R1: CLI Framework Selection (Resolved → clap)
**Question**: Which framework provides best ergonomics for multi-command CLI with global options and subcommand-specific arguments?

**Research Criteria**:
- Derive macro support for type-safe argument parsing
- Error message quality for user-facing errors
- Help generation customization
- Maintenance status and ecosystem adoption
- Performance impact on startup time

**Decision Target**: Select framework and document rationale in research.md

#### R2: Async Runtime Strategy (Resolved → Synchronous)
**Question**: Do we need async runtime (tokio/async-std) or is sync sufficient for process execution and file I/O?

**Research Criteria**:
- Performance benefit of async file I/O for multi-MB driver packages
- Complexity overhead for spawning PowerShell processes
- Compatibility with Windows-specific APIs (some may be sync-only)
- Concurrent VM operations support (if future requirement)

**Decision Target**: Async vs sync, and if async which runtime; document tradeoffs

#### R3: Debug Output Capture Mechanism (Resolved → DebugView + PS Direct)
**Question**: Best approach for capturing DbgPrint and OutputDebugString in real-time from guest VM?

**Research Options**:
- DebugView SDK/API (if accessible from host)
- ETW (Event Tracing for Windows) session from host listening to guest
- PowerShell Direct session running custom capture tool in guest
- WinDbg remote debugging protocol

**Research Criteria**:
- Host-side vs guest-side capture tradeoffs
- Real-time streaming vs polling
- Reliability for all driver types (KMDF, UMDF, WDM)
- Setup complexity and dependencies

**Decision Target**: Capture mechanism with implementation approach

#### R4: Logging Framework Selection (Resolved → tracing)
**Question**: tracing vs env_logger for structured logging with span contexts?

**Research Criteria**:
- Span/context support for async operations
- JSON output support for automation
- Performance overhead
- Ecosystem integration (e.g., with tokio if async chosen)

**Decision Target**: Logging framework selection

#### R5: Configuration File Format (Resolved → TOML)
**Question**: TOML vs JSON for persisting VM configuration and tool preferences?

**Research Criteria**:
- Human editability (TOML more readable, JSON machine-friendly)
- Serde ecosystem support
- File size and parsing performance
- Conventional choice for Rust CLIs

**Decision Target**: Config format with schema definition

#### R6: CLI Testing Framework (Resolved → assert_cmd stack)
**Question**: Best practices for testing CLI applications with assert_cmd vs alternatives?

**Research Criteria**:
- Snapshot testing support for help text and JSON output
- Temp directory management for test VMs
- Async test execution if needed
- Integration with cargo test

**Decision Target**: CLI testing approach and framework

#### R7: Windows Driver Detection Patterns (Resolved → Metadata + INF heuristic)
**Question**: Reliable heuristics for detecting driver type (KMDF/UMDF/WDM) from Cargo.toml and crate structure?

**Research Criteria**:
- Conventional crate metadata for driver types
- INF file parsing as fallback
- Difference between windows-drivers-rs and Windows-Rust-driver-samples structures

**Decision Target**: Detection algorithm specification

#### R8: Hyper-V PowerShell Interop Best Practices (Resolved → Ephemeral PS + JSON wrapper)
**Question**: Best approach for invoking PowerShell commands from Rust reliably?

**Research Options**:
- Direct process execution with JSON serialization
- PowerShell SDK via FFI
- Typed wrapper crates (e.g., winreg, windows-rs)

**Research Criteria**:
- Error handling and parsing robustness
- Version compatibility (PowerShell 5.1 vs 7+)
- Performance vs type safety

**Decision Target**: PowerShell interop pattern with example code

### Research Deliverable

**Output**: `research.md` containing:
- Decision for each research task (R1-R8)
- Rationale explaining selection criteria
- Alternatives considered and why rejected
- Code snippets/proof-of-concept for complex decisions
- Bibliography/links to documentation/benchmarks

**Validation**: All NEEDS CLARIFICATION markers resolved before Phase 1

## Phase 1: Design & Contracts

### Data Model (data-model.md)

**Entities from Specification**:

#### DriverPackage
- **Fields**: package_name, driver_type (KMDF/UMDF/WDM), version, architecture, build_output_path, inf_path, certificate_path
- **Validation**: INF must exist, certificate must be valid, version parseable
- **Relationships**: Detected from CargoWorkspace, deployed to TestVM
- **State**: Detected → Built → Validated → Deployed

#### TestVM
- **Fields**: vm_name, vm_id, state (Running/Stopped), configuration (memory, CPU, network), baseline_snapshot_id, installed_certificates[], loaded_drivers[]
- **Validation**: Hyper-V VM must exist, PowerShell Direct accessible
- **Relationships**: Hosts DriverPackage deployments, captures DebugOutputStream
- **State**: Creating → Configuring → Baseline → Ready → InUse → Cleanup

#### TestCertificate
- **Fields**: thumbprint, subject, expiration_date, installation_status (NotInstalled/Installed/Trusted)
- **Validation**: Not expired, valid for code signing
- **Relationships**: Required by DriverPackage, installed in TestVM
- **State**: Detected → Validated → Installed → Verified

#### DebugOutputStream
- **Fields**: message_text, timestamp, source (Driver/Application), level (Info/Warning/Error)
- **Validation**: Timestamp ordering, source filter
- **Relationships**: Emitted by DriverPackage or CompanionApplication in TestVM
- **State**: Captured → Filtered → Validated

#### CompanionApplication
- **Fields**: executable_path, expected_output_patterns[], interaction_scenarios[]
- **Validation**: Executable exists and is PE format
- **Relationships**: Packaged with DriverPackage, executed in TestVM
- **State**: Detected → Copied → Executed → OutputCaptured

#### TestConfiguration
- **Fields**: vm_identifier, repository_type (WindowsDriversRs/WindowsRustDriverSamples), validation_rules, output_capture_settings, timeout_config
- **Validation**: VM identifier must resolve to TestVM, timeouts > 0
- **Relationships**: Persisted across test runs, applied to TestVM operations
- **State**: Default → Loaded → Applied → Persisted

### API Contracts (contracts/)

**Note**: This is a CLI tool, not a service, so "API contracts" refer to:
1. **CLI Command Interface**: Documented command syntax, arguments, exit codes
2. **Internal Module Boundaries**: Public traits and types for testability
3. **External Process Contracts**: PowerShell command formats, expected outputs

#### CLI Command Contracts

**Command**: `driver-test`
- **Synopsis**: Detect, build, deploy, and verify driver in test VM
- **Arguments**: 
  - `--package-path <PATH>` (optional, default: current directory)
  - `--vm-name <NAME>` (optional, uses/creates default test VM)
  - `--revert-snapshot` (flag, revert to baseline before test)
  - `--rebuild-vm` (flag, force fresh VM creation)
  - `--capture-output` (flag, enable debug output capture)
  - `--json` (flag, output results as JSON)
  - `-v, -vv, -vvv` (verbosity levels)
- **Exit Codes**: 0 (pass), 1 (user error), 2 (system error)
- **Output**: Human-readable test report or JSON test results

**Command**: `driver-test setup`
- **Synopsis**: Create and configure test VM with baseline snapshot
- **Arguments**:
  - `--vm-name <NAME>` (required)
  - `--memory <MB>` (default: 2048)
  - `--cpu-count <N>` (default: 2)
  - `--disk-size <GB>` (default: 60)
- **Exit Codes**: 0 (created), 1 (validation error), 2 (Hyper-V error)
- **Output**: VM configuration summary

**Command**: `driver-test clean`
- **Synopsis**: Remove test VM and cleanup resources
- **Arguments**:
  - `--vm-name <NAME>` (optional, default test VM)
  - `--yes` (skip confirmation)
- **Exit Codes**: 0 (cleaned), 1 (not found)
- **Output**: Confirmation message

**Command**: `driver-test snapshot`
- **Synopsis**: Create or revert to baseline snapshot
- **Arguments**:
  - `--vm-name <NAME>` (optional)
  - `--create` (create new baseline)
  - `--revert` (revert to existing baseline)
- **Exit Codes**: 0 (success), 2 (Hyper-V error)
- **Output**: Snapshot operation result

#### Internal Module Contracts (Rust Traits)

**Trait**: `VmProvider`
```rust
trait VmProvider {
    fn create_vm(&self, config: VmConfiguration) -> Result<TestVM, VmError>;
    fn get_vm(&self, identifier: &str) -> Result<Option<TestVM>, VmError>;
    fn snapshot_vm(&self, vm: &TestVM, name: &str) -> Result<SnapshotId, VmError>;
    fn revert_to_snapshot(&self, vm: &TestVM, snapshot: &SnapshotId) -> Result<(), VmError>;
    fn execute_command(&self, vm: &TestVM, command: &str) -> Result<CommandOutput, VmError>;
    fn copy_file(&self, vm: &TestVM, source: &Path, dest: &Path) -> Result<(), VmError>;
}
```

**Trait**: `DriverDetector`
```rust
trait DriverDetector {
    fn detect_driver_package(&self, path: &Path) -> Result<Option<DriverPackage>, DetectionError>;
    fn detect_repository_type(&self, path: &Path) -> Result<RepositoryType, DetectionError>;
    fn locate_companion_application(&self, package: &DriverPackage) -> Result<Option<CompanionApplication>, DetectionError>;
}
```

**Trait**: `DebugOutputCapture`
```rust
trait DebugOutputCapture {
    fn start_capture(&mut self, vm: &TestVM) -> Result<CaptureSession, CaptureError>;
    fn read_messages(&self, session: &CaptureSession) -> Result<Vec<DebugMessage>, CaptureError>;
    fn stop_capture(&mut self, session: CaptureSession) -> Result<Vec<DebugMessage>, CaptureError>;
}
```

#### External Process Contracts

**PowerShell Command**: VM Creation
- **Command**: `New-VM -Name <name> -MemoryStartupBytes <bytes> -Generation 2`
- **Expected Output**: VM object with Id, State, Name properties
- **Error Patterns**: "already exists", "insufficient resources"

**PowerShell Command**: PowerShell Direct Execution
- **Command**: `Invoke-Command -VMName <name> -ScriptBlock {<command>} -Credential $cred`
- **Expected Output**: Command stdout/stderr, exit code
- **Error Patterns**: "PowerShell Direct not available", "VM not running"

**PowerShell Command**: File Copy
- **Command**: `Copy-VMFile -Name <vm> -SourcePath <src> -DestinationPath <dst> -FileSource Host`
- **Expected Output**: None on success
- **Error Patterns**: "file not found", "access denied", "VM integration services not running"

### Quickstart Guide (quickstart.md)

```markdown
# Driver Testing CLI - Quick Start

## Prerequisites

1. Windows 10/11 Pro or Enterprise (Hyper-V support required)
2. Hyper-V enabled: `Enable-WindowsOptionalFeature -Online -FeatureName Microsoft-Hyper-V -All`
3. Rust toolchain installed: https://rustup.rs
4. Cargo WDK installed: `cargo install cargo-wdk`
5. Admin privileges (required for Hyper-V operations)

## Installation

```bash
cargo install driver-test-cli
```

## First Run: Setup Test VM

Create a test VM with baseline snapshot:

```bash
driver-test setup --vm-name wdk-test-vm
```

This will:
- Create a new Hyper-V VM with 2GB RAM, 2 CPUs
- Install Windows (provide ISO path when prompted)
- Configure for driver testing (test signing enabled)
- Create baseline snapshot for fast resets

Expected time: ~15 minutes

## Test Your First Driver

Navigate to your driver package directory:

```bash
cd path/to/my-driver
driver-test
```

The tool will:
1. Detect driver type (KMDF/UMDF/WDM)
2. Build driver package if needed
3. Deploy to test VM
4. Install test certificate
5. Load driver
6. Verify version matches
7. Check PnP device status
8. Report results

Expected time: <5 minutes

## Capture Debug Output

Enable debug output monitoring:

```bash
driver-test --capture-output
```

## Clean VM State

Revert to baseline snapshot:

```bash
driver-test --revert-snapshot
```

## Advanced: Test with Companion App

For drivers with test applications (e.g., echo driver):

```bash
cd path/to/echo-driver
driver-test --capture-output
```

The tool automatically detects echo.exe and runs interaction tests.

## Troubleshooting

**Error: "Hyper-V not enabled"**
```bash
Enable-WindowsOptionalFeature -Online -FeatureName Microsoft-Hyper-V -All
# Reboot required
```

**Error: "PowerShell Direct unavailable"**
- Ensure VM is running
- Check integration services: `Get-VMIntegrationService -VMName wdk-test-vm`
- Enable Guest Service Interface if disabled

**Verbose Logging**
```bash
driver-test -vvv  # Trace-level logs
```

## Next Steps

- See [user guide](./user-guide.md) for complete command reference
- Review [troubleshooting guide](./troubleshooting.md) for common issues
- Check [examples](./examples/) for sample driver projects
```

### Agent Context Update

*This section will be populated after running update-agent-context.ps1 script*

## Phase 2: Integration & Reliability (In Progress)

### Completed Tasks
- [x] Robust pnputil enumeration parser (DriverInfo struct + parse_pnputil_enum_output)
  - Parser unit tests: single entry, multiple entries, missing fields
  - Documented parsing approach in docs/parser-notes.md
- [x] Structured deployment output (DeployResult + emit_deploy)
- [x] CLI deploy command integration with JSON output support
- [x] Dependency injection for DeployCommand (execute<D: DriverDeployer>)
  - Mock deployer via DRIVER_TEST_CLI_MOCK environment variable
  - Integration test scaffolding (tests/deploy_integration.rs)
- [x] Module exposure for testing (cli, output in src/lib.rs)
- [x] JSON failure output test (deploy_cli.rs)

### Active Focus Areas
1. ✅ Parsing robustness for pnputil output (multi-driver segmentation) — **COMPLETED**
2. ⏳ End-to-end integration tests on real Hyper-V VM — **SCAFFOLDED (mock path working)**
3. ⏳ UX polish (progress indicators, refined JSON outputs)
4. ⏳ Documentation expansion (user-guide.md, troubleshooting.md, examples/)

### Next Priorities
1. Real Hyper-V provider integration tests (live VM environment)
2. Progress indicators for long-running operations (VM creation, file transfer)
3. Enhanced PnP verification beyond simple version check
4. WMI-based metadata enrichment (Win32_PnPSignedDriver queries)
5. Warning cleanup (feature gates or actual usage of scaffolded modules)

## Artifacts Generated

- [x] plan.md (this file)
- [x] research.md (Phase 0 output - R1-R8 decisions)
- [x] data-model.md (Phase 1 output)
- [x] contracts/ directory (Phase 1 output)
- [x] quickstart.md (embedded in plan.md)
- [ ] Agent context updated (Phase 1)
- [x] **driver-test-cli crate** (scaffolded with all modules)
  - [x] Cargo.toml with dependencies
  - [x] src/main.rs (tracing init, CLI dispatch)
  - [x] src/cli.rs (clap commands with dependency injection)
  - [x] src/config.rs (TOML loader)
  - [x] src/vm.rs (VmProvider trait + HypervProvider impl)
  - [x] src/driver_detect.rs (detection algorithm with INF parsing)
  - [x] src/package.rs (package info struct)
  - [x] src/deploy.rs (cert/driver install + pnputil parser)
  - [x] src/debug.rs (message classifier + capture session)
  - [x] src/echo_test.rs (echo test harness)
  - [x] src/output.rs (JSON/human formatter + DeployResult)
  - [x] src/errors.rs (unified AppError taxonomy)
  - [x] src/ps.rs (PowerShell JSON wrapper)
  - [x] src/lib.rs (module exports including cli, output)
  - [x] tests/cli_help.rs (CLI help test)
  - [x] tests/detect_driver.rs (detection tests)
  - [x] tests/deployment.rs (deployment unit tests)
  - [x] tests/pnputil_parse.rs (parser correctness tests)
  - [x] tests/deploy_cli.rs (CLI JSON failure test)
  - [x] tests/deploy_integration.rs (mock deployer injection test)
  - [x] tests/vm_ops.rs (VM lifecycle tests)
  - [x] docs/parser-notes.md (pnputil parsing documentation)
  - [x] README.md (build/test instructions)

## Next Steps

1. Execute Phase 0 research tasks (R1-R8)
2. Consolidate findings in research.md
3. Proceed to Phase 1 design deliverables
4. Run agent context update script
5. Return to this plan for Phase 2 task breakdown

---

## Phase 1: Implementation Kickoff

### Module Boundaries
| Module        | Responsibility                                                  |
| ------------- | --------------------------------------------------------------- |
| cli           | Argument parsing & dispatch                                     |
| config        | Load/validate TOML, defaults merging                            |
| vm            | Hyper-V operations (create/start/snapshot/revert/copy/exec)     |
| driver_detect | Determine driver type & companion app                           |
| package       | Locate build artifacts (INF, SYS, cert), version metadata       |
| deploy        | Cert install, driver install/update, version & PnP verification |
| debug         | Launch & stream DebugView, classify, rotate logs                |
| echo_test     | End-to-end echo driver interaction                              |
| output        | Human vs JSON formatting, result structs                        |
| errors        | Unified error taxonomy                                          |

### Risk Register
| ID      | Risk                                  | Impact | Likelihood | Mitigation                                              |
| ------- | ------------------------------------- | ------ | ---------- | ------------------------------------------------------- |
| RISK-01 | Early boot prints missed by DebugView | Low    | Medium     | Document limitation; optional future ETW feature        |
| RISK-02 | INF absent pre-build                  | Medium | Medium     | Fallback to Cargo heuristics; warn; user override flag  |
| RISK-03 | PowerShell Direct transient failures  | Medium | High       | Exponential backoff, classify errors                    |
| RISK-04 | JSON output parse failures            | Medium | Low        | Validate schema; include raw stderr in error object     |
| RISK-05 | Metadata vs INF mismatch              | Low    | Low        | Prefer INF; emit warning; override flag `--driver-type` |
| RISK-06 | Snapshot restore slow                 | Medium | Low        | Benchmark & allow skip flag                             |
| RISK-07 | Log file unbounded growth             | Low    | Medium     | Implement rotation & size cap                           |

### Next Action Checklist
- [x] Scaffold crate & module directories
- [x] Implement tracing initialization & verbosity mapping
- [x] Implement PowerShell JSON wrapper (`run_ps_json`)
- [x] Implement driver detection (INF parser + heuristics)
- [x] Implement VM snapshot management & file copy
- [x] Implement initial deployment (certificate + pnputil install + enum parse)
- [x] Implement debug streaming & classification
- [x] Add baseline CLI tests (help text verified)
- [x] Add detection tests (metadata + INF parsing)
- [x] Add echo driver test skeleton
- [x] Add deployment tests (certificate install success, version mismatch)
- [x] Integrate deploy command into CLI
- [x] Unify error taxonomy (DeployError -> AppError)
- [x] Implement echo driver end-to-end test
- [x] Implement debug capture tests & rotation logic
- [x] Update artifacts list
- [x] Implement robust pnputil parser with unit tests
- [x] Add structured DeployResult and JSON output
- [x] Add dependency injection for DeployCommand
- [x] Add mock deployer integration test
- [x] Document parser approach (parser-notes.md)
- [ ] Implement real Hyper-V integration tests (live VM)
- [ ] Add progress indicators for long operations
- [ ] Enhance PnP verification (device enumeration)
- [ ] Add WMI metadata enrichment
- [ ] Expand documentation (user-guide, troubleshooting, examples)
- [ ] Address dead code warnings (feature gates or usage)

**Status**: Phase 2 partially delivered — parser robustness, structured outputs, and test injection mechanism complete. Next: real Hyper-V integration and UX enhancements.
**Blocking Items**: None
**Risk Level**: Managed per register
