# Design Consensus Document

**Date**: 2026-03-04
**Input**: 10 reviews across 6 AI models (Claude Opus 4.5, Opus 4.6, Sonnet 4, GPT 5.1, GPT 5.2, Gemini 3 Pro)
**Perspectives**: Correctness, Security, UX/Ergonomics, Windows Systems Expertise

---

## Consensus: Blocking Issues (Must Fix Before Implementation)

These issues had agreement across 2+ models and would cause the tool to fail in practice.

### B1: Test Signing Mode Not Addressed
**Consensus**: UNANIMOUS (all Windows reviewers + correctness)

The requirements assume installing certificates is sufficient. In reality, the test VM **must** have:
1. `bcdedit /set testsigning on` (required for kernel-mode test-signed drivers)
2. Secure Boot **disabled** (Gen 2 VMs have it on by default; blocks test-signed drivers AND DebugView's kernel driver `Dbgv.sys`)
3. Reboot after these changes

**Resolution**: Add to L1.2 (VM Lifecycle) a `setup` requirement that configures the guest OS for test signing. This is a one-time operation during VM provisioning, not per-deployment.

### B2: pnputil Text Parsing Is Fragile
**Consensus**: UNANIMOUS (all Windows reviewers)

`pnputil /enum-drivers` output is fully localized (German, French, Japanese, etc.) and the text format changes across Windows versions. Two alternatives identified:

- **Gemini found**: `pnputil /enum-drivers /format xml` exists and provides stable XML output
- **All agreed**: `Get-CimInstance Win32_PnPSignedDriver` provides structured PowerShell objects

**Resolution**: Use `pnputil /enum-drivers /format xml` as primary (validate in POC). Fall back to `Get-CimInstance Win32_PnPSignedDriver` via PowerShell Direct. Drop text parsing entirely.

### B3: PowerShell Direct Requires Credentials
**Consensus**: STRONG (Opus 4.6, GPT 5.1, GPT 5.2)

`Invoke-Command -VMName` requires explicit `-Credential` on most setups unless running as a domain admin with specific delegation. CI service accounts and local accounts both need credentials.

**Resolution**: Add credential management to L1.1. Support: (a) `-Credential (Get-Credential)` for interactive, (b) stored credential from config/environment for CI. Never log or echo credentials. Consider `New-PSSession` reuse for performance.

### B4: Driver Installation ≠ Device Creation
**Consensus**: STRONG (Gemini, Sonnet, Opus 4.6)

`pnputil /add-driver /install` stages the driver in the driver store but does **not** create a device node for software-only drivers (like the echo sample). You need `devcon install <inf> <hwid>` or `pnputil /add-device` to actually instantiate the device.

**Resolution**: L2.3-1 must distinguish between staging and device creation. For drivers that match existing hardware, staging + scan is sufficient. For software drivers (echo, etc.), explicit device creation via devcon or `pnputil /add-device` is required. The detection layer should identify which approach is needed.

### B5: DbgPrint Filtering on Modern Windows
**Consensus**: STRONG (Gemini, Opus 4.6, GPT 5.1)

Since Windows Vista, `DbgPrint` output is filtered by default. DebugView's `/k` flag alone is NOT sufficient. The guest VM must have the Debug Print Filter registry key set:

```
HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Debug Print Filter
    DEFAULT = DWORD:0xFFFFFFFF
```

**Resolution**: Add to VM setup requirements. This is a one-time guest configuration alongside test signing.

### B6: Command Injection Risk
**Consensus**: STRONG (GPT 5.1, Opus 4.6)

The tool constructs PowerShell scripts with user-controlled strings (VM names, file paths). String interpolation into PowerShell is an injection vector.

**Resolution**: All PowerShell invocations MUST use parameterized arguments (separate `-ArgumentList` parameters) or validated/escaped inputs. Never use string interpolation of user input into PowerShell script blocks. Validate VM names against `^[a-zA-Z0-9_-]+$` pattern.

---

## Consensus: High-Priority Issues (Should Fix)

### H1: DebugView Deployment & Trust
**Consensus**: MODERATE (Opus 4.6 + GPT 5.1)

Downloading Dbgview.exe from the internet into a guest VM that then runs it with kernel privileges is a supply chain risk. The binary should be hash-verified after download.

**Resolution**: Pin a known-good SHA256 hash. Verify after download, before execution. Document the hash in the tool's config/constants. Consider bundling DebugView separately or requiring pre-installation.

### H2: DebugView Race Condition with Driver Loading
**Consensus**: MODERATE (Sonnet, Opus 4.6)

If DebugView starts after the driver loads, early `DbgPrint` calls during `DriverEntry` are lost.

**Resolution**: Start DebugView BEFORE driver installation in the workflow. Document this as a design constraint. For `DriverEntry` debugging, recommend kernel debugger as the alternative.

### H3: Detection Fails on Complex Workspaces
**Consensus**: MODERATE (Sonnet, GPT 5.1)

Both target repos use complex workspace structures. The detection algorithm assumes simple single-package layouts. Examples in `windows-drivers-rs` are excluded from the workspace and must be treated as standalone crates.

**Resolution**: Detection should first check if the current directory is a Cargo package (has `Cargo.toml` with `[package]`). If not, search for workspace members. The `--package-path` flag is the escape hatch. POC-8 must validate against real repo layouts.

### H4: Companion App Needs Device Interface Ready
**Consensus**: MODERATE (Sonnet, Gemini)

The echo test app communicates with the driver via a device interface. There's a timing gap between driver installation and the device interface becoming available. The companion app may fail if run too early.

**Resolution**: After driver installation and device creation, poll for device readiness (device interface registered, PnP state = started) before executing the companion app. Add a configurable readiness timeout.

### H5: Missing Build Step in Requirements
**Consensus**: SPLIT (Opus 4.5 says gap; GPT 5.1 says intentional)

The requirements don't explicitly require building the driver before deployment. The `test` command implies it, but it's not a formal requirement.

**Resolution**: The `test` command orchestration (L4.2-1) should include "build if needed" as part of the workflow. The `deploy` command assumes pre-built artifacts. This is an orchestration concern, not a new layer.

### H6: Certificate Cleanup Not Specified
**Consensus**: MODERATE (GPT 5.1 security, Opus 4.6 security)

The tool installs root certificates but never removes them. Over time, the VM accumulates test certs.

**Resolution**: The `clean` command should optionally remove test certificates. The `setup` command should reuse the same cert across runs rather than generating new ones each time.

---

## Consensus: UX Decisions

### U1: Default Subcommand Behavior
**Consensus**: Show contextual help with auto-detected driver info (Sonnet recommended, GPT agreed).

### U2: Error Message Format
**Consensus**: Consistent format with severity, message, and ACTION line:
```
ERROR [deploy]: Driver version mismatch: expected 1.0.0.0, loaded 1.0.0.1
  ACTION: Rebuild driver with matching version or use --skip-version-check
```

### U3: Progress Reporting
**Consensus**: Use stderr for progress (spinners/status) so stdout remains clean for JSON. Detect TTY vs pipe and adjust accordingly.

### U4: Additional Commands Suggested
- `driver-test-cli status` — show VM state, installed drivers, readiness (GPT 5.2)
- `driver-test-cli doctor` — check prerequisites: Hyper-V, admin, PS version, VM exists (Sonnet + GPT 5.2)

---

## Consensus: Architectural Decision Updates

| Original Decision | Review Finding | Updated Decision |
|---|---|---|
| pnputil text parsing | Fragile, localized, breaks on non-English | Use `pnputil /enum-drivers /format xml` (validate in POC) |
| DebugView `/k /t /q` | Missing `/g` for UMDF; needs DbgPrint filter registry key | Add `/g`; setup must configure Debug Print Filter registry |
| PowerShell Direct credentialless | Won't work for most setups | Must support explicit credentials (interactive + stored) |
| Certificate install = ready | Need testsigning + Secure Boot off | Setup command must configure guest OS for test signing |
| `pnputil /add-driver /install` = loaded | Only stages for software drivers | Must use devcon or pnputil /add-device for software drivers |
| Single command for everything | Build step implicit | `test` command includes build; `deploy` assumes pre-built |

---

## Open Questions for User

1. **VM provisioning scope**: Should the tool handle Windows OS installation, or assume a pre-installed Windows VM? (All reviewers noted this gap; most recommended assuming pre-installed.)

2. **Credential storage**: For CI, should credentials come from environment variables, a config file, or Windows Credential Manager?

3. **devcon dependency**: devcon is part of the WDK. Should the tool assume it's available in the guest, or deploy it?

4. **`doctor` command**: Should we add a prerequisite-checking command? (2 reviewers recommended it.)
