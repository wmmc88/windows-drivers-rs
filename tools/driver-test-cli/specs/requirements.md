# driver-test-cli v2 — Requirements Specification

## Overview

A Rust CLI tool that automates the full Windows driver test cycle — from detecting a driver package to verifying it runs correctly in a Hyper-V test VM — replacing a manual multi-step workflow with a single command.

**Target**: `cargo build` → driver verified in VM in <5 minutes (excluding first-time VM setup).

## Layered Requirements

Requirements are organized into four layers reflecting their dependency chain. Each layer only depends on the layers below it.

---

### Layer 1: Infrastructure

*Capabilities for talking to the OS and Hyper-V. These have zero knowledge of drivers.*

#### L1.1 — PowerShell Execution

| ID | Requirement |
|----|-------------|
| L1.1-1 | System MUST execute PowerShell commands and return structured (JSON) output with stdout, stderr, and exit code separation. |
| L1.1-2 | System MUST classify PowerShell errors as transient (retry-eligible) or fatal, based on known error message patterns. |
| L1.1-3 | System MUST retry transient errors with exponential backoff (configurable max retries, default 3). |
| L1.1-4 | System MUST enforce per-operation timeouts and kill hung processes. |
| L1.1-5 | System MUST use PowerShell Direct (`Invoke-Command -VMName`) as the sole channel for guest command execution (FR-032). |
| L1.1-6 | System MUST fail with a clear, actionable error if PowerShell Direct is unavailable (FR-032). |

#### L1.2 — VM Lifecycle

| ID | Requirement |
|----|-------------|
| L1.2-1 | System MUST create Hyper-V Generation 2 VMs with configurable memory, CPU count, and disk size (FR-003). |
| L1.2-2 | System MUST detect and reuse existing VMs by name, avoiding duplicate creation (FR-004). |
| L1.2-3 | System MUST persist VM configuration for reuse across test runs (FR-005). |
| L1.2-4 | System MUST manage VM state: query current state, start if stopped, ensure running before operations. |
| L1.2-5 | System MUST create named baseline snapshots (`Checkpoint-VM`) for clean-state testing (FR-031). |
| L1.2-6 | System MUST revert to baseline snapshots (`Restore-VMSnapshot`) before test runs when requested (FR-031). |
| L1.2-7 | System MUST preserve VM state when errors occur to enable manual debugging (FR-026). |
| L1.2-8 | System MUST validate sufficient system resources before VM creation (FR-024). |
| L1.2-9 | System MUST provide clear errors when Hyper-V or virtualization support is missing (FR-023). |

#### L1.3 — File Transfer

| ID | Requirement |
|----|-------------|
| L1.3-1 | System MUST copy files from host to guest VM via `Copy-VMFile -FileSource Host` (FR-006). |
| L1.3-2 | System MUST create destination directories in the guest if they don't exist. |
| L1.3-3 | System MUST detect and report Integration Services failures with actionable guidance. |

---

### Layer 2: Driver Operations

*Capabilities for working with driver packages. Depends on Layer 1 for VM access.*

#### L2.1 — Driver Detection

| ID | Requirement |
|----|-------------|
| L2.1-1 | System MUST detect driver type (KMDF, UMDF, WDM) from `[package.metadata.wdk.driver-model]` in Cargo.toml (FR-001). |
| L2.1-2 | System MUST fall back to INF/INX section scanning (`[KMDF]`, `[UMDF]`, `[Version]`) when metadata is absent (FR-001). |
| L2.1-3 | System MUST fall back to kernel-like heuristics (`panic = "abort"` + `no_std`) as last resort for WDM classification. |
| L2.1-4 | System MUST allow manual override of detected driver type via CLI flag. |
| L2.1-5 | System MUST locate driver build output (INF, SYS, catalog, certificate) from cargo build artifacts (FR-002). |
| L2.1-6 | System MUST extract driver version from INF `DriverVer` directive. |
| L2.1-7 | System MUST adapt detection for `windows-drivers-rs` repository layout (FR-021). |
| L2.1-8 | System MUST adapt detection for `Windows-Rust-driver-samples` repository layout (FR-022). |
| L2.1-9 | System MUST validate architecture compatibility between host, VM, and driver (FR-029). |

#### L2.2 — Certificate Management

| ID | Requirement |
|----|-------------|
| L2.2-1 | System MUST install test signing certificates in guest VM's TrustedPeople and Root stores (FR-007). |
| L2.2-2 | System MUST skip certificate installation if already present and trusted (FR-008). |
| L2.2-3 | System MUST provide actionable guidance when test signing setup fails (FR-028). |

#### L2.3 — Driver Installation & Verification

| ID | Requirement |
|----|-------------|
| L2.3-1 | System MUST install drivers via `pnputil /add-driver <inf> /install` in the guest VM (FR-009). |
| L2.3-2 | System MUST parse `pnputil /enum-drivers` output to extract installed driver metadata (published name, version, provider, class, signer). |
| L2.3-3 | System MUST verify loaded driver version matches the exact built version (FR-010). |
| L2.3-4 | System MUST report version mismatches with both expected and actual values (FR-027). |
| L2.3-5 | System MUST query PnP manager to confirm device node is present and started (FR-011). |
| L2.3-6 | System MUST capture and report driver installation failures with Windows diagnostic info (FR-025). |
| L2.3-7 | System MUST offer to unload existing driver versions when deploying a new version (FR-030). |
| L2.3-8 | System MAY enrich driver metadata via WMI `Win32_PnPSignedDriver` queries (optional). |

---

### Layer 3: Observability

*Capabilities for capturing and validating runtime output. Depends on Layer 1 for VM access.*

#### L3.1 — Debug Output Capture

| ID | Requirement |
|----|-------------|
| L3.1-1 | System MUST deploy DebugView (`Dbgview.exe`) to the guest VM, downloading from `live.sysinternals.com` if not present. |
| L3.1-2 | System MUST launch DebugView with kernel capture (`/k`), global Win32 capture (`/g`), tray mode (`/t`), quiet mode (`/q`), EULA acceptance (`/accepteula`), and file logging (`/l <path>`). |
| L3.1-3 | System MUST capture `DbgPrint` output from kernel-mode drivers (KMDF, WDM) (FR-012). |
| L3.1-4 | System MUST capture `OutputDebugString` output from user-mode drivers (UMDF) via global Win32 capture (FR-013). |
| L3.1-5 | System MUST stream captured debug output to the host in near-real-time via log file tailing (FR-014). |
| L3.1-6 | System MUST classify messages by severity level (Info, Warning, Error, Verbose) based on content keywords. |
| L3.1-7 | System MUST implement log rotation with configurable maximum message count to prevent unbounded memory use. |

#### L3.2 — Pattern Validation

| ID | Requirement |
|----|-------------|
| L3.2-1 | System MUST validate debug output against expected message patterns from pattern files or defaults (FR-015). |
| L3.2-2 | System MUST report which expected patterns were found and which were missing. |

---

### Layer 4: Orchestration

*High-level workflows that compose layers 1-3. User-facing CLI surface.*

#### L4.1 — Companion Application Testing

| ID | Requirement |
|----|-------------|
| L4.1-1 | System MUST detect companion applications (e.g., echo.exe) from Cargo binary targets or conventional directories (FR-016). |
| L4.1-2 | System MUST copy companion applications to the guest VM (FR-017). |
| L4.1-3 | System MUST execute companion applications in the VM and capture stdout, stderr, exit code (FR-018). |
| L4.1-4 | System MUST validate companion output against expected patterns (FR-020). |
| L4.1-5 | System MUST correlate driver debug output with companion application output (FR-019). |

#### L4.2 — CLI Commands

| ID | Requirement |
|----|-------------|
| L4.2-1 | `test` command: detect, build, deploy, verify driver; optionally capture debug output and run companion app. |
| L4.2-2 | `setup` command: create and configure a test VM with baseline snapshot. |
| L4.2-3 | `snapshot` command: create or revert to baseline snapshot. |
| L4.2-4 | `deploy` command: deploy driver + certificate, verify installation. |
| L4.2-5 | `clean` command: remove test VM with confirmation. |
| L4.2-6 | System MUST support `--json` flag for CI-friendly structured output. |
| L4.2-7 | System MUST support `-v/-vv/-vvv` verbosity levels (WARN → INFO → DEBUG → TRACE). |
| L4.2-8 | System MUST support `--vm-name` global override with TOML config file defaults. |
| L4.2-9 | System MUST use exit codes: 0 (success), 1 (user error), 2 (system error). |
| L4.2-10 | System MUST provide actionable error messages with remediation guidance (FR-023, FR-028). |

#### L4.3 — Configuration

| ID | Requirement |
|----|-------------|
| L4.3-1 | System MUST load configuration from `driver-test.toml` with VM defaults, verbosity, and timeout settings. |
| L4.3-2 | CLI flags MUST override config file values. |
| L4.3-3 | System MUST work without a config file using sensible defaults. |

---

## Success Criteria

| ID | Metric | Target |
|----|--------|--------|
| SC-1 | Build-to-verified-loaded cycle time | <5 min (excludes VM creation) |
| SC-2 | First-time VM creation time | <15 min |
| SC-3 | VM reuse detection accuracy | 100% |
| SC-4 | Version mismatch detection accuracy | 100% |
| SC-5 | Debug capture reliability (all 3 driver types) | ≥95% |
| SC-6 | Echo test cycle time (after VM ready) | <3 min |
| SC-7 | Cross-repo detection accuracy | 100% (both repos, no config) |
| SC-8 | Actionable error coverage | 100% of failure scenarios |
| SC-9 | Manual step reduction | ≥80% vs current workflow |

---

## Architectural Decisions (carried from v1 research, subject to review)

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Runtime | Synchronous (no async) | PowerShell process execution dominates; async adds complexity for <1% benefit |
| VM channel | PowerShell Direct only | Simple, fast, no network config; fail-fast if unavailable |
| Debug capture | DebugView in guest + log file streaming | Unified kernel + user capture without kernel debugger |
| Config format | TOML | Human-editable, Rust convention, comment support |
| CLI framework | clap v4 (derive) | Ecosystem standard |
| PS interop | Ephemeral process + JSON wrapping | No FFI/COM, structured error parsing |
| Detection | Metadata → INF → heuristics | >98% accuracy, works pre-build |

---

## FR Traceability

Every original FR is covered:

| Original FR | v2 Requirement |
|-------------|---------------|
| FR-001 | L2.1-1, L2.1-2, L2.1-3 |
| FR-002 | L2.1-5 |
| FR-003 | L1.2-1 |
| FR-004 | L1.2-2 |
| FR-005 | L1.2-3 |
| FR-006 | L1.3-1 |
| FR-007 | L2.2-1 |
| FR-008 | L2.2-2 |
| FR-009 | L2.3-1 |
| FR-010 | L2.3-3 |
| FR-011 | L2.3-5 |
| FR-012 | L3.1-3 |
| FR-013 | L3.1-4 |
| FR-014 | L3.1-5 |
| FR-015 | L3.2-1 |
| FR-016 | L4.1-1 |
| FR-017 | L4.1-2 |
| FR-018 | L4.1-3 |
| FR-019 | L4.1-5 |
| FR-020 | L4.1-4 |
| FR-021 | L2.1-7 |
| FR-022 | L2.1-8 |
| FR-023 | L1.2-9, L4.2-10 |
| FR-024 | L1.2-8 |
| FR-025 | L2.3-6 |
| FR-026 | L1.2-7 |
| FR-027 | L2.3-4 |
| FR-028 | L2.2-3, L4.2-10 |
| FR-029 | L2.1-9 |
| FR-030 | L2.3-7 |
| FR-031 | L1.2-5, L1.2-6 |
| FR-032 | L1.1-5, L1.1-6 |
