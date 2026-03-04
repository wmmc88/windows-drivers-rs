# UX and Ergonomics Review: driver-test-cli

**Review Date**: 2025-01-06
**Requirements Version**: v2 spec from requirements.md  
**Focus**: User experience, CLI design patterns, and first-run ergonomics

## Executive Summary

The proposed 5-command structure is well-conceived but needs refinement in default behaviors, error messaging consistency, and first-run experience. The tool could benefit from more intelligent defaults and better alignment with modern CLI conventions.

## 1. CLI Interface Design Review

### Current 5-Command Structure Analysis

| Command | Purpose | Granularity Assessment |
|---------|---------|----------------------|
| `test` | Full end-to-end test cycle | ✅ **Correct** - Core user workflow |
| `setup` | Create and configure VM | ✅ **Correct** - Long-running one-time setup |
| `snapshot` | Manage baseline snapshots | ⚠️ **Consider** - May be too granular |
| `deploy` | Deploy driver without testing | ✅ **Correct** - Useful for iterative dev |
| `clean` | Remove test VM | ✅ **Correct** - Essential maintenance |

### Recommendations

#### ✅ Keep Current Structure
The 5-command structure aligns well with established CLI patterns:
- **cargo**: `build`, `test`, `run`, `clean`, `check`
- **docker**: `run`, `build`, `ps`, `rm`, `logs`
- **kubectl**: `apply`, `get`, `delete`, `logs`, `exec`

#### 🔄 Consider Command Consolidation
**Option 1**: Merge `snapshot` into `setup` as a subcommand:
```bash
driver-test-cli setup --create-snapshot [name]
driver-test-cli setup --restore-snapshot [name]
```

**Option 2**: Keep separate but add aliases:
```bash
driver-test-cli snapshot create baseline  # explicit
driver-test-cli snap baseline             # alias
```

#### 📝 Flag Naming Review
Current flags appear intuitive but need consistency:
- `--vm-name` ✅ Clear and descriptive
- `--json` ✅ Standard across CLI tools
- `-v/-vv/-vvv` ✅ Unix convention
- Recommend adding `--dry-run` for `test` command
- Recommend adding `--force` for `clean` command

## 2. Default Behavior Design

### Proposed Default Behaviors

#### 2.1 No Arguments: `driver-test-cli`
**Recommendation**: Display contextual help + auto-detection summary
```bash
$ driver-test-cli
driver-test-cli v2.1.0 - Windows Driver Testing Tool

Detected: KMDF driver 'echo' v1.0.0.0 in current directory
VM: test-vm-echo (ready)

Common commands:
  test     Run full driver test cycle
  deploy   Deploy driver to test VM
  setup    Create or configure test VM

Run 'driver-test-cli test' to test your driver.
Run 'driver-test-cli --help' for all options.
```

**Rationale**: Like `cargo` without args, provides useful context rather than just help.

#### 2.2 Current Directory: `driver-test-cli .`  
**Recommendation**: Explicit syntax not needed - current directory is implicit
```bash
$ driver-test-cli test    # Already tests current directory
$ driver-test-cli test .  # Redundant but harmless
```

#### 2.3 Should `test` be default?
**Recommendation**: No, explicit commands are better
- Safety: Prevents accidental long-running operations
- Clarity: Makes intent explicit in CI scripts
- Convention: Most CLI tools require explicit subcommands

## 3. Error Messages Design

### Consistent Error Format
```
Error: <Error Category>: <Brief Description>

<Detailed explanation of what went wrong>

Suggested actions:
• <Action 1>
• <Action 2>
• <Action 3>

For more help: driver-test-cli <command> --help
```

### Concrete Error Message Templates

#### 3.1 Missing Hyper-V
```
Error: Hyper-V Unavailable: Virtualization support not detected

Windows Hyper-V is required but not available on this system.

Suggested actions:
• Enable Hyper-V in Windows Features (requires Windows 10 Pro/Enterprise)
• Run: Enable-WindowsOptionalFeature -Online -FeatureName Microsoft-Hyper-V -All
• Ensure hardware virtualization is enabled in BIOS/UEFI
• Verify you're not running in a VM (nested virtualization may not be supported)

For more help: driver-test-cli setup --help
```

#### 3.2 VM Not Found
```
Error: VM Missing: Test VM 'test-vm-echo' not found

The required test VM does not exist or may have been deleted.

Suggested actions:
• Run: driver-test-cli setup --vm-name test-vm-echo
• Check existing VMs: Get-VM | Select Name, State
• Use different VM name: driver-test-cli test --vm-name <name>

For more help: driver-test-cli setup --help
```

#### 3.3 PowerShell Direct Unavailable
```
Error: PowerShell Direct Failed: Cannot connect to VM 'test-vm-echo'

PowerShell Direct connection failed. This is required for all VM operations.

Suggested actions:
• Ensure VM is running: driver-test-cli setup --start-vm
• Check Integration Services are installed and running in the VM
• Verify the VM has completed boot process (wait 2-3 minutes)
• Try restarting VM: Stop-VM test-vm-echo; Start-VM test-vm-echo

For more help: https://docs.microsoft.com/en-us/windows-server/virtualization/hyper-v/manage/manage-hyper-v-integration-services
```

#### 3.4 Certificate Installation Failed
```
Error: Certificate Installation Failed: Test signing certificate rejected

The driver certificate could not be installed in the test VM.

Suggested actions:
• Ensure test signing is enabled: bcdedit /set testsigning on
• Check certificate is valid: certutil -verify <cert-file>
• Try manual certificate installation in the VM
• Recreate VM with fresh Windows installation: driver-test-cli clean && driver-test-cli setup

For more help: driver-test-cli deploy --help
```

#### 3.5 Version Mismatch
```
Error: Version Mismatch: Driver version conflict detected

Expected driver version 1.2.3.4 but found 1.2.3.0 installed in VM.

Suggested actions:
• Build latest version: cargo build
• Force reinstall: driver-test-cli deploy --force
• Revert to clean snapshot: driver-test-cli snapshot restore baseline
• Check Cargo.toml version matches INF DriverVer

For more help: driver-test-cli test --help
```

#### 3.6 No Driver Package Found
```
Error: Driver Detection Failed: No driver package found in current directory

Could not locate a Windows driver package (INF, Cargo.toml with WDK metadata, or build artifacts).

Suggested actions:
• Ensure you're in a driver project directory
• Run 'cargo build' to generate driver artifacts
• Check Cargo.toml contains [package.metadata.wdk] section
• Specify driver path explicitly: driver-test-cli test --driver-path <path>

For more help: driver-test-cli test --help
```

## 4. Progress Reporting Design

### Interactive Terminal (TTY)
**Spinners + Status Lines**:
```
⠋ Creating VM 'test-vm-echo'...
✓ Creating VM 'test-vm-echo' (2.3s)
⠋ Installing Windows (step 3/8)... [████████████░░░░] 75% (11m 32s remaining)
✓ VM setup complete (14m 12s)

⠋ Deploying driver package...
  • Copying files to VM... ✓ (1.2s)
  • Installing certificate... ✓ (0.8s)
  • Installing driver... ⠋ (3.4s)
```

### CI/Piped Output (Non-TTY)
**Timestamped Log Lines**:
```
2025-01-06T14:30:15Z [INFO] Starting VM creation: test-vm-echo
2025-01-06T14:30:18Z [INFO] VM created successfully (2.3s)
2025-01-06T14:30:18Z [INFO] Installing Windows (step 3/8, 75% complete)
2025-01-06T14:44:30Z [INFO] VM setup complete (14m 12s)
2025-01-06T14:44:30Z [INFO] Starting driver deployment
2025-01-06T14:44:31Z [INFO] Files copied to VM (1.2s)
2025-01-06T14:44:32Z [INFO] Certificate installed (0.8s)
2025-01-06T14:44:35Z [INFO] Driver installed successfully (3.4s)
```

### Progress Indicators by Operation

| Operation | Duration | Interactive | CI Output |
|-----------|----------|-------------|-----------|
| VM Creation | 15min | Progress bar + ETA | Milestone logging |
| Windows Install | 10min | Progress bar + steps | Step completion logs |
| Driver Deploy | 5min | Spinner + substeps | Timestamped progress |
| Debug Capture | Ongoing | Live message count | Message rate stats |
| File Transfer | 30s | Transfer rate | Size + duration |

## 5. JSON Output Schema

### Success Schema
```json
{
  "status": "success",
  "timestamp": "2025-01-06T14:45:12Z",
  "duration_seconds": 287,
  "driver": {
    "name": "echo",
    "version": "1.0.0.0",
    "type": "KMDF",
    "architecture": "x64",
    "inf_path": "target/debug/echo.inf",
    "sys_path": "target/debug/echo.sys"
  },
  "vm": {
    "name": "test-vm-echo",
    "state": "Running",
    "snapshot": "baseline-2025-01-06"
  },
  "installation": {
    "published_name": "oem42.inf",
    "installed_version": "1.0.0.0",
    "device_status": "started",
    "certificate_trusted": true
  },
  "debug_output": {
    "messages_captured": 15,
    "patterns_matched": ["driver_init", "device_connected"],
    "patterns_missing": [],
    "log_file": "C:\\temp\\driver-debug.log"
  },
  "companion_app": {
    "executed": true,
    "exit_code": 0,
    "output": "Echo test successful: 5 messages processed",
    "duration_seconds": 12
  }
}
```

### Failure Schema
```json
{
  "status": "failure",
  "timestamp": "2025-01-06T14:32:45Z",
  "duration_seconds": 42,
  "error": {
    "category": "driver_installation",
    "code": "version_mismatch",
    "message": "Driver version conflict detected",
    "details": {
      "expected_version": "1.2.3.4",
      "installed_version": "1.2.3.0",
      "vm_name": "test-vm-echo"
    }
  },
  "driver": {
    "name": "echo",
    "version": "1.2.3.4",
    "type": "KMDF",
    "architecture": "x64"
  },
  "vm": {
    "name": "test-vm-echo",
    "state": "Running"
  },
  "suggested_actions": [
    "Build latest version: cargo build",
    "Force reinstall: driver-test-cli deploy --force",
    "Revert to clean snapshot: driver-test-cli snapshot restore baseline"
  ]
}
```

## 6. First-Run Experience Design

### Optimal First-Run Flow

#### Step 1: User Discovery (0 commands)
```bash
$ git clone https://github.com/microsoft/windows-drivers-rs.git
$ cd windows-drivers-rs/crates/sample-kmdf-driver
$ ls
# User sees: Cargo.toml, src/, echo.inx, etc.
```

#### Step 2: Check Prerequisites (1 command)
```bash
$ driver-test-cli
driver-test-cli v2.1.0 - Windows Driver Testing Tool

⚠️  First-time setup required:
   • Hyper-V: Available ✓
   • Test VM: Not created
   • Driver package: KMDF driver 'echo' detected ✓

Run 'driver-test-cli setup' to create your test VM (~15 minutes).
```

#### Step 3: One-Time Setup (1 command)
```bash
$ driver-test-cli setup
⠋ Creating VM 'test-vm-echo'...
⠋ Downloading Windows 11 Dev VM image... [████████░░] 80% (2m 15s remaining)
⠋ Installing Windows (step 6/8)... [███████████░░] 85% (3m 42s remaining)
✓ VM setup complete (14m 23s)
✓ Baseline snapshot created

Your test VM is ready! Run 'driver-test-cli test' to test your driver.
```

#### Step 4: First Test (1 command) 
```bash
$ driver-test-cli test
⠋ Building driver package...
✓ Building driver package (12.3s)
⠋ Deploying to test VM...
✓ Deploying to test VM (4.7s)
⠋ Running driver tests...
✓ Running driver tests (8.2s)

✅ Test passed! 
   Driver 'echo' v1.0.0.0 installed and verified in 'test-vm-echo'
   Debug output: 12 messages captured, all expected patterns found
   Companion app: echo test successful (5 messages processed)
```

### Alternative: Express Setup
For experienced users who want to minimize interaction:
```bash
$ driver-test-cli test --setup-if-needed
⚠️  Test VM not found. Setting up automatically...
[... VM creation progress ...]
⠋ Building and testing driver...
✅ Test passed!
```

### Success Metrics for First-Run
- **Total commands**: ≤3 (check, setup, test)
- **First success time**: <20 minutes (setup: 15min + test: 5min)
- **Error recovery**: Clear next steps on any failure
- **Confidence building**: Show progress and explain what's happening

## Recommendations Summary

### High Priority
1. **Implement contextual default behavior** for `driver-test-cli` (no args)
2. **Design consistent error message format** with actionable guidance
3. **Add progress indicators** that work in both interactive and CI contexts
4. **Define complete JSON schemas** for programmatic consumption

### Medium Priority  
5. **Consider consolidating `snapshot` into `setup`** subcommands
6. **Add `--dry-run` flag** to `test` command for validation
7. **Implement express setup option** (`--setup-if-needed`)

### Low Priority
8. **Add command aliases** (`snap` for `snapshot`)
9. **Enhance VM status reporting** in default behavior
10. **Add completion times** to all JSON output

This design prioritizes user success on first use while maintaining power-user flexibility and CI automation capabilities.