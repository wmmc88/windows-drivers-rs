# Windows Systems Expertise Review

**Reviewer**: Windows Systems / Hyper-V / Driver Development SME  
**Date**: 2025-07-11  
**Scope**: Requirements spec (`requirements.md`) and research decisions (`research.md`)  
**Verdict**: Design is sound at the architectural level, but several implementation assumptions need hardening before they hit real-world Windows environments. This review identifies concrete failure modes and recommends mitigations.

---

## 1. PowerShell Direct Reliability

### 1.1 How `Invoke-Command -VMName` Actually Works

PowerShell Direct uses the Hyper-V VMBus (not the network stack). The host's `vmicvmsession` integration service opens a channel to the guest's `vmicvmsession` service. This means:

- **No network required** — works even if the guest has zero NICs.
- **Requires the guest OS to be fully booted** — specifically, the `vmicvmsession` (Hyper-V PowerShell Direct Service) must be running inside the guest. This is a Windows service that starts *after* the OS reaches the desktop/login screen phase, not during early boot.
- **Requires Windows 10/Server 2016 or later** in the guest. Does not work with Windows 7/8.1 or Linux guests.
- **Requires PowerShell 5.1+ in the guest** for reliable operation. PowerShell 2.0 remoting in older guests is not supported over VMBus.

### 1.2 Timing After VM Start

| Scenario | Typical time until PS Direct available | Worst case |
|----------|---------------------------------------|------------|
| Cold start (from Off) | 30–90 seconds | 120+ seconds (slow disk, Windows Update pending) |
| Resume from Saved state | 5–15 seconds | 30 seconds |
| Snapshot revert (`Restore-VMSnapshot`) | 30–90 seconds | Same as cold start — restoring a snapshot effectively reboots the guest from the snapshot's saved state |
| Start after `Stop-VM -TurnOff` (hard power off) | 30–90 seconds | 120+ seconds (chkdsk may run) |

**Critical finding for the design**: Snapshot revert is *not* instant. `Restore-VMSnapshot` applies the snapshot and leaves the VM in "Off" or "Saved" state (depending on whether the snapshot included memory). The tool must then call `Start-VM` and wait for the guest to boot. The current spec (L1.2-6) says "revert to baseline snapshots" but does not account for this boot wait. **Recommendation**: After `Restore-VMSnapshot`, always follow with `Start-VM` and then a readiness probe loop.

### 1.3 Readiness Probe

The retry pattern in R8 (detecting `"A remote session might have ended"`) is necessary but not sufficient. A robust readiness check should:

```powershell
# Probe: attempt a trivial command; swallow errors until it works
$ready = $false
for ($i = 0; $i -lt 30; $i++) {
    try {
        Invoke-Command -VMName $VMName -Credential $cred -ScriptBlock { $true } -ErrorAction Stop
        $ready = $true
        break
    } catch {
        Start-Sleep -Seconds 5
    }
}
```

**Recommendation**: Add an explicit `wait_for_guest_ready()` step with configurable timeout (default 180s) that runs after every `Start-VM` or `Restore-VMSnapshot`.

### 1.4 Authentication Requirements

This is a **critical gap** in the current design. `Invoke-Command -VMName` requires `-Credential` with explicit credentials. The behavior depends on the account type:

| Account type | Works? | Notes |
|-------------|--------|-------|
| Local admin account with password | ✅ Yes | Most reliable for test VMs |
| Microsoft account (live.com) | ❌ No | PS Direct does not support Microsoft accounts |
| Azure AD account | ❌ No | Not supported over VMBus |
| Domain account | ✅ Yes | If guest is domain-joined |
| Local account without password | ❌ No | PS remoting requires a password |
| Built-in Administrator with blank password | ❌ No | Blocked by default UAC policy |

**The spec never mentions credentials**. The tool must either:
1. Accept credentials via CLI flag / config / environment variable / Windows Credential Manager.
2. Set up the test VM with a known local admin account during `setup` and store/reuse it.

**Recommendation**: During `setup`, create a local admin account (e.g., `DriverTest` / known password) and store the credential reference in `driver-test.toml`. This is a test VM — security of the password is low-priority. Consider using `Get-Credential` interactively on first run and caching via `Export-Clixml` (DPAPI-encrypted, host-user-bound).

### 1.5 Additional Failure Modes

| Failure | Symptom | Mitigation |
|---------|---------|------------|
| Guest in "Paused" state | `Invoke-Command` hangs indefinitely | Check `(Get-VM).State` before attempting PS Direct |
| Guest in "Saved" state | `Invoke-Command` fails immediately | Must `Start-VM` first |
| Integration Services disabled | `Invoke-Command` fails with "The operation cannot be performed while the object is in its current state" | Check `Get-VMIntegrationService -VMName $vm -Name 'Guest Service Interface'` |
| Guest firewall (irrelevant) | N/A — PS Direct does not use network | No action needed |
| Guest WinRM disabled | N/A — PS Direct does not use WinRM | No action needed |
| Multiple VMs with same name | Ambiguous target, unpredictable behavior | Validate VM name uniqueness in `setup` |
| Host Hyper-V service stopped | All operations fail | Check `Get-Service vmms` at startup |

---

## 2. pnputil Output Stability

### 2.1 Text Output Format

`pnputil /enum-drivers` produces key-value text output. The format has been **surprisingly stable** across Windows 10 1607 through Windows 11 24H2, but with important caveats:

```
Published Name:     oem42.inf
Original Name:      mydriver.inf
Provider Name:      Contoso
Class Name:         Sample
Class GUID:         {78A1C341-...}
Driver Version:     06/21/2024 1.0.0.0
Signer Name:        Contoso Test Certificate
```

**Stability assessment**:
- The field names (`Published Name`, `Original Name`, etc.) have not changed in English builds since Windows 10 RS1.
- The field order has occasionally varied between versions but is consistent within a version.
- The date format in `Driver Version` follows the system locale (MM/DD/YYYY in en-US, DD/MM/YYYY in others).

### 2.2 Localization — This Is a Real Problem

The key names are **fully localized**. This is not limited to Spanish:

| Language | "Published Name" becomes | "Provider Name" becomes |
|----------|------------------------|------------------------|
| English | Published Name | Provider Name |
| German | Veröffentlichter Name | Anbietername |
| French | Nom publié | Nom du fournisseur |
| Japanese | 公開名 | プロバイダー名 |
| Chinese (Simplified) | 发布的名称 | 提供程序名称 |
| Portuguese (Brazil) | Nome publicado | Nome do provedor |

**This means any text-parsing approach will fail on non-English Windows installations** unless the tool either:
1. Forces English output (not possible with pnputil).
2. Maintains a localization table for every supported language.
3. Uses a structured alternative.

### 2.3 Does `/format json` Exist?

**No.** As of Windows 11 24H2, `pnputil` does not support `/format json` or any structured output flag. The only output format is the localized text.

However, starting with **Windows 11 22H2**, `pnputil` added `/enum-drivers /class <GUID>` filtering, which is useful but still text-based.

### 2.4 Structured Alternatives

| Alternative | Pros | Cons |
|------------|------|------|
| `Get-WindowsDriver -Online -All` | PowerShell native, structured objects, not localized | Very slow (10-30s), designed for offline servicing, limited driver state info |
| `Get-CimInstance Win32_PnPSignedDriver` | Fast, structured, WMI standard, not localized, includes DeviceID and status | Only shows *signed* drivers with device nodes — won't show staged drivers without devices |
| `Get-PnpDevice` + `Get-PnpDeviceProperty` | Best for device-centric queries, structured, fast | Requires knowing device instance ID; doesn't enumerate all staged drivers |
| `pnputil /enum-drivers` + force-locale hack | Text output but forced to English | Fragile, requires `chcp 437` + `SetThreadUILanguage` — not reliable |

**Recommendation**: Use a **two-pronged approach**:

1. **For driver installation verification** (L2.3-1, L2.3-2): Use `pnputil /add-driver` for installation (its *exit code* is language-independent: 0 = success), then verify via `Get-CimInstance Win32_PnPSignedDriver | Where-Object { $_.InfName -eq 'oem42.inf' } | ConvertTo-Json`. This gives structured, locale-independent output including `DriverVersion`, `DriverProviderName`, `Signer`, `DeviceName`, and `DeviceID`.

2. **For staged driver enumeration** (finding the published name): Parse the stdout of `pnputil /add-driver` itself, which prints the published name (e.g., `Published name: oem42.inf`) on the same invocation. Capture it at install time rather than querying later.

3. **Fallback**: If `Win32_PnPSignedDriver` returns no results (driver staged but no device present), use `Get-WindowsDriver -Online` as a slow but reliable fallback.

### 2.5 Version String Parsing

The `DriverVersion` field in pnputil output combines date and version: `06/21/2024 1.0.0.0`. The date portion uses the **system locale date format**, making it unparseable without locale knowledge. `Win32_PnPSignedDriver.DriverVersion` returns just the version string (`1.0.0.0`), which is far more reliable for comparison (L2.3-3, L2.3-4).

---

## 3. DebugView Behavior Deep-Dive

### 3.1 Does `/k` (Kernel Capture) Require Debug Mode?

**No, with a caveat.**

- `DbgPrint` / `KdPrint` output is controlled by the `DbgPrintEx` filtering system (`Kd_DEFAULT_Mask` registry value), **not** by `bcdedit /debug on`.
- `bcdedit /debug on` enables *kernel debugger attachment* (WinDbg, etc.), which is a completely separate mechanism.
- DebugView's `/k` flag hooks into `DbgPrint` via a kernel-mode component (`Dbgv.sys`) that registers as a debug print callback.
- **However**: On modern Windows 10/11 with Secure Boot, the `Dbgv.sys` driver is **test-signed** by Sysinternals. If the guest VM has Secure Boot enabled (Gen 2 default), `Dbgv.sys` will fail to load unless:
  - Test signing is enabled: `bcdedit /set testsigning on`, OR
  - The Sysinternals certificate is in the Secure Boot DB (it isn't by default).

**This is a critical finding**: The design relies on DebugView for kernel capture, but **DebugView's own kernel driver may not load on a Secure Boot Gen 2 VM** without `bcdedit /set testsigning on`. This same setting is likely needed for the test-signed driver being tested anyway, but the spec should make this explicit.

**Additional note on DbgPrint filtering**: By default, Windows 10+ suppresses most `DbgPrint` output. To capture everything, the guest needs:
```
reg add "HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Debug Print Filter" /v DEFAULT /t REG_DWORD /d 0xFFFFFFFF /f
```
This should be part of the `setup` command's VM configuration.

### 3.2 Does `/g` (Global Win32) Capture UMDF OutputDebugString?

**Yes, but with important nuances.**

- UMDF drivers (v2) run inside a WUDFHost.exe service host process.
- `OutputDebugString` from within WUDFHost.exe is a standard Win32 call.
- DebugView's `/g` flag captures `OutputDebugString` from *all* sessions, including Session 0 (where services run).
- Without `/g`, DebugView only captures from the interactive user session, and would **miss** UMDF driver output entirely.

**Confirmed**: `/g` is required and correct for UMDF. The spec correctly includes it (L3.1-2).

**Caveat**: `OutputDebugString` in modern UMDF drivers is atypical. The recommended UMDF debug output mechanism is `TraceLoggingWrite` or WPP Tracing, which DebugView **cannot** capture. If the Rust WDK drivers use `OutputDebugString` (via the Rust `wdk` crate's `DbgPrint` macro in user-mode context), this should work. But if they use ETW/TraceLogging, DebugView will see nothing. **Verify which debug output API the Rust WDK crate uses for UMDF.**

### 3.3 What If Another DebugView Instance Is Already Running?

**DebugView is exclusive.** Only one instance can capture kernel debug output at a time because `Dbgv.sys` only allows a single client. If another instance is running:

- A second DebugView instance will start but display: "Another instance of DebugView is already running."
- The `/k` and `/g` capture flags silently fail — no output is captured, no error exit code.
- The log file is created but remains empty.

**Recommendation**: Before launching DebugView, check for and kill existing instances:
```powershell
Get-Process Dbgview -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 2  # Wait for Dbgv.sys to unload
```
Add this to the startup sequence (before L3.1-2).

### 3.4 Does DebugView Need Admin?

**Yes, for kernel capture.** Specifically:

| Mode | Admin required? |
|------|----------------|
| `/k` (kernel) | Yes — loading `Dbgv.sys` requires elevation |
| `/g` (global Win32) | Yes — capturing across sessions requires `SeDebugPrivilege` |
| User-mode only (no `/k` or `/g`) | No |

Since the design uses both `/k` and `/g`, DebugView must run elevated. PowerShell Direct sessions run as the specified credential user, which must be a local administrator. This is consistent with the credential requirement from Section 1.4.

### 3.5 Log File Output Format

DebugView's log file format (when using `/l`) is:

```
[00000000]	0.00000000	Message text here
[00000001]	0.00010234	Another message
[00000002]	5.12345678	[mydriver] DriverEntry called
```

Format: `[sequence_number]\t[elapsed_seconds]\t[message_text]`

- Sequence number: monotonically increasing, zero-padded 8-digit hex.
- Timestamp: seconds since DebugView started, with 8 decimal places (~100ns resolution).
- Message text: raw string from `DbgPrint` / `OutputDebugString`.
- Tab-separated (literal `\t` characters).
- **No PID/TID information** in the log file (unlike the GUI, which can show PID).
- **No source indicator** (kernel vs. user-mode) in the log file — both are interleaved.

**Recommendation for L3.1-6 (severity classification)**: The log format contains no inherent severity markers. Classification must be purely content-based (looking for strings like `ERROR:`, `WARNING:`, etc. in the message text). This is fragile. Consider defining a convention for Rust WDK driver messages (e.g., prefix with `[ERR]`, `[WRN]`, `[INF]`) and documenting it.

### 3.6 Race Condition: DebugView Startup vs. Driver Loading

**This is a real risk.** The sequence in the spec is:

1. Deploy DebugView
2. Start DebugView
3. Install driver (pnputil)
4. Driver loads, calls DriverEntry, emits DbgPrint

If steps 2-3 happen too quickly, DebugView may not have finished loading `Dbgv.sys` before the driver's first `DbgPrint`. Typical `Dbgv.sys` load time is 1-3 seconds.

**Recommendation**: After launching DebugView, wait for the log file to be created (indicating Dbgv.sys is loaded and capture is active) before proceeding with driver installation:

```powershell
# Launch DebugView
Start-Process -FilePath 'C:\Tools\Dbgview.exe' -ArgumentList '/k /g /t /q /accepteula /l C:\DriverLogs\debug.log'
# Wait for log file creation (indicates capture is active)
$timeout = 15
$sw = [System.Diagnostics.Stopwatch]::StartNew()
while (-not (Test-Path 'C:\DriverLogs\debug.log') -and $sw.Elapsed.TotalSeconds -lt $timeout) {
    Start-Sleep -Milliseconds 500
}
```

---

## 4. Copy-VMFile Prerequisites

### 4.1 Integration Services Requirements

`Copy-VMFile -FileSource Host` requires the **Guest Service Interface** integration service. This is:

- Available in Windows 10/Server 2016+ guests with Integration Services version **6.3.9600.16384** or later.
- The specific service is `vmicguestinterface` (Guest Service Interface), which is **disabled by default** in many Windows installations.
- Must be enabled on both sides:
  - Host: `Enable-VMIntegrationService -VMName $vm -Name 'Guest Service Interface'`
  - Guest: The corresponding service must be running (auto-starts when enabled from host).

**Critical finding**: If the `setup` command creates a VM and installs Windows, the Guest Service Interface is likely disabled by default. The setup must explicitly enable it. **Add this to L1.2-1 or create a new setup requirement.**

### 4.2 Gen 1 vs. Gen 2 VMs

| Aspect | Gen 1 | Gen 2 |
|--------|-------|-------|
| `Copy-VMFile` support | ✅ Yes | ✅ Yes |
| Integration Services | Installed via `vmguest.iso` | Built into Windows 10+ |
| Secure Boot | ❌ Not available | ✅ Enabled by default |
| UEFI | ❌ BIOS only | ✅ UEFI only |

Both generations support `Copy-VMFile`. The spec mandates Gen 2 (L1.2-1), which is correct for modern driver testing (UEFI, Secure Boot testing). However, see Section 5 on Secure Boot implications.

### 4.3 File Size Limits

There is **no documented hard file size limit** for `Copy-VMFile`. However:

- Transfers are synchronous and block the PowerShell pipeline.
- Very large files (>1GB) can time out if the tool has per-operation timeouts (L1.1-4).
- The VMBus transport is not optimized for bulk data; expect ~50-100 MB/s for large files.
- Driver packages are typically small (<10MB), so this is not a practical concern.

**Practical limit**: The guest's disk must have sufficient free space, and the transfer must complete within the tool's operation timeout.

### 4.4 Login Screen vs. No User Logged In

**`Copy-VMFile` works regardless of user login state.** It communicates via Integration Services (VMBus), which runs as a SYSTEM service. No user session is required. This is a significant advantage over network-based file copy.

However, there is one subtlety: if the guest is at the "Press Ctrl+Alt+Del" screen or in OOBE (Out-of-Box Experience), the Guest Service Interface service may not yet be started. The service typically starts during the `winlogon` phase but before user login.

### 4.5 Alternative: Copy-Item -ToSession

The research (R3) mentions `Copy-Item -ToSession` as an alternative. This uses PowerShell Direct's remoting session and has different trade-offs:

| Aspect | `Copy-VMFile` | `Copy-Item -ToSession` |
|--------|--------------|----------------------|
| Requires Guest Service Interface | ✅ Yes | ❌ No (uses PS Direct session) |
| Requires credentials | ❌ No | ✅ Yes (session requires auth) |
| Works at login screen | ✅ Yes (if services running) | ✅ Yes (if PS Direct available) |
| Speed | Faster for large files | Slower (serialized over PS remoting) |
| Progress reporting | None | PowerShell progress stream available |
| Creates directories | `-CreateFullPath` flag | Must create manually |

**Recommendation**: Since the tool already depends on PowerShell Direct (which requires credentials), `Copy-Item -ToSession` is a viable alternative that avoids the Guest Service Interface dependency entirely. Consider using it as the primary mechanism with `Copy-VMFile` as fallback:

```powershell
$session = New-PSSession -VMName $VMName -Credential $cred
Copy-Item -Path $SourcePath -Destination $DestPath -ToSession $session -Force
```

This eliminates the need to enable Guest Service Interface during setup.

---

## 5. Certificate Installation and Test Signing

### 5.1 Certificate Stores Are Necessary But Not Sufficient

The spec (L2.2-1) says:
> Install test signing certificates in guest VM's TrustedPeople and Root stores.

This is **necessary but not sufficient** for test-signed driver loading. The full requirements are:

1. **Certificate in Root store**: Establishes the cert as a trusted root CA. ✅ Spec covers this.
2. **Certificate in TrustedPeople store**: Trusts the specific certificate for code signing. ✅ Spec covers this.
3. **`bcdedit /set testsigning on`**: Tells the Windows kernel to accept test-signed drivers. ❌ **Not mentioned in the spec.**
4. **Reboot after enabling test signing**: The `testsigning` BCD flag only takes effect after reboot. ❌ **Not mentioned in the spec.**

Without `bcdedit /set testsigning on`, the kernel will refuse to load any driver signed with a test certificate, regardless of certificate store configuration. The driver will fail to start with `STATUS_INVALID_IMAGE_HASH`.

**This is a critical gap.** The `setup` command must:
```powershell
bcdedit /set testsigning on
# Reboot required for this to take effect
Restart-Computer -Force
```

### 5.2 Secure Boot in Gen 2 VMs

This is the most significant design tension in the spec. Gen 2 VMs have Secure Boot **enabled by default**. Secure Boot and test signing are **mutually exclusive**:

- Secure Boot validates all boot-time code against the UEFI Secure Boot database (Microsoft certificates).
- `bcdedit /set testsigning on` is **silently ignored** when Secure Boot is enabled. The system boots normally but test-signed drivers will **not** load.
- DebugView's `Dbgv.sys` (also test-signed) will also fail to load.

**Resolution options**:

| Option | Pros | Cons |
|--------|------|------|
| **Disable Secure Boot on test VM** | Simple, enables test signing, enables Dbgv.sys | Doesn't test Secure Boot scenarios |
| Use UEFI Secure Boot DB enrollment | Tests the "real" deployment path | Complex, requires MOK or custom PK/KEK — overkill for dev testing |
| Use attestation-signed drivers | Production-like | Requires EV cert + Microsoft Hardware Dashboard — not for dev iteration |

**Recommendation**: The `setup` command should **disable Secure Boot** on the test VM:
```powershell
Set-VMFirmware -VMName $VMName -EnableSecureBoot Off
```

This must happen before the first boot (or when the VM is off). Document this as a deliberate trade-off. Add a `--secure-boot` flag for future scenarios where users want to test with Secure Boot enabled (which would require a different signing strategy).

### 5.3 Complete Setup Sequence for Test Signing

The `setup` command should execute (in this order):

1. Create Gen 2 VM with Secure Boot **disabled**.
2. Install Windows.
3. Enable Guest Service Interface integration service.
4. Create local admin account for PowerShell Direct.
5. Run inside guest via PS Direct:
   ```powershell
   bcdedit /set testsigning on
   reg add "HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Debug Print Filter" /v DEFAULT /t REG_DWORD /d 0xFFFFFFFF /f
   ```
6. Reboot guest.
7. Take baseline snapshot.

This ensures the snapshot includes a fully configured test environment.

---

## 6. Alternative Approaches

### 6.1 pnputil Text Parsing → WMI/CIM Structured Queries

**Strongly recommended.** Replace `pnputil /enum-drivers` parsing (L2.3-2) with:

```powershell
# After pnputil /add-driver installs the driver, verify via WMI:
$driver = Get-CimInstance Win32_PnPSignedDriver |
    Where-Object { $_.InfName -like 'oem*.inf' -and $_.DriverProviderName -eq 'YourProvider' } |
    Select-Object DeviceName, DriverVersion, InfName, Signer, DriverProviderName, DeviceID |
    ConvertTo-Json -Compress
```

**Benefits**: Locale-independent, structured, includes device status. Keep `pnputil /add-driver` for installation (its exit code is reliable) but use WMI for all enumeration and verification.

For device node status (L2.3-5), use:

```powershell
$device = Get-PnpDevice | Where-Object { $_.FriendlyName -match 'YourDriver' }
$device.Status  # "OK", "Error", "Degraded", "Unknown"
$device | Get-PnpDeviceProperty -KeyName 'DEVPKEY_Device_ProblemCode' |
    Select-Object -ExpandProperty Data  # 0 = no problem
```

### 6.2 DebugView → ETW/TraceLogging

For future consideration (not blocking for v1). The current DebugView approach is pragmatic but has limitations (see Section 3). A more robust alternative for v2:

| Component | Tool | Notes |
|-----------|------|-------|
| KMDF/WDM kernel debug prints | `DbgPrint` → captured by DebugView ✅ | Works today |
| UMDF structured tracing | WPP / TraceLogging → `tracelog.exe` + `tracefmt.exe` from WDK | More reliable than OutputDebugString |
| Unified capture | `xperf` or `wpr` (Windows Performance Recorder) | Captures ETW from all sources |

**For v1**: DebugView is acceptable. But document its limitations (Section 3) and plan for ETW support as a future enhancement. The Rust `wdk` crate should be checked to see whether its debug output macros use `DbgPrint` (captured by DebugView) or WPP/ETW (not captured).

### 6.3 Copy-VMFile → Shared Folder

**Not recommended** as primary. Shared folders (`Set-VMHost -VirtualHardDiskPath` or SMB shares) require:
- Network connectivity in the guest (PS Direct's advantage is no network needed).
- SMB configuration, firewall rules, credentials.
- More attack surface and setup complexity.

**Recommendation**: Use `Copy-Item -ToSession` (Section 4.5) as the primary file transfer mechanism since it piggybacks on the PS Direct session already required by the design. Fall back to `Copy-VMFile` only if PS Direct session copy is too slow for large files.

### 6.4 DebugView Log Tailing → PS Direct Session Streaming

The current design uses `Get-Content -Wait` over PS Direct to stream the DebugView log. An alternative that avoids the log file entirely:

```powershell
# Register for debug output events directly (user-mode only):
# Not practical for kernel messages.
```

**For kernel messages, there is no good alternative to the log file approach.** However, the tailing implementation should handle:
- File locked by DebugView (use `-ReadCount 0` with retry).
- Log file not yet created (wait loop, see Section 3.6).
- DebugView crashing or being killed (detect missing process, restart).

### 6.5 PowerShell Process Spawning → PowerShell SDK / Runspace

The current design spawns `powershell.exe` for each operation. An alternative is using the PowerShell SDK via .NET interop (through `windows-rs` or similar):

**Not recommended for v1.** The process-spawning approach is:
- Simpler to implement in Rust.
- Easier to debug (visible command lines, captured stdout/stderr).
- More robust against PowerShell host crashes (isolated processes).

The ~200ms overhead per process spawn is acceptable given that operations take seconds.

---

## 7. Summary of Critical Findings

| # | Finding | Severity | Spec Gap | Recommendation |
|---|---------|----------|----------|---------------|
| 1 | **Credentials not addressed** | 🔴 Critical | L1.1-5 doesn't mention `-Credential` | Add credential management to setup and config |
| 2 | **`bcdedit /set testsigning on` missing** | 🔴 Critical | L2.2-1 only covers cert stores | Add test signing enablement to setup |
| 3 | **Secure Boot blocks test-signed drivers and Dbgv.sys** | 🔴 Critical | L1.2-1 creates Gen 2 (Secure Boot on by default) | Disable Secure Boot in setup |
| 4 | **Snapshot revert requires Start-VM + boot wait** | 🟡 High | L1.2-6 implies instant revert | Add post-revert boot wait with readiness probe |
| 5 | **pnputil output is fully localized** | 🟡 High | L2.3-2 assumes parseable text | Switch to WMI/CIM for verification |
| 6 | **DebugView exclusive instance** | 🟡 High | L3.1-2 doesn't handle existing instances | Kill existing Dbgview before launch |
| 7 | **DbgPrint filtering suppresses output by default** | 🟡 High | L3.1-3 assumes DbgPrint works | Set `Debug Print Filter` registry during setup |
| 8 | **DebugView race condition with driver load** | 🟠 Medium | L3.1-2 doesn't specify ordering guarantee | Wait for log file creation before installing driver |
| 9 | **Guest Service Interface disabled by default** | 🟠 Medium | L1.3-1 assumes it works | Enable during setup, or switch to `Copy-Item -ToSession` |
| 10 | **No PID/source info in DebugView logs** | 🟡 Low | L3.1-6 plans severity classification | Document log format limitations; define message conventions |
| 11 | **DebugView log has no kernel vs. user indicator** | 🟡 Low | L3.1-3/L3.1-4 distinguish kernel/user | Cannot distinguish in log file; document limitation |

---

## 8. Recommended Setup Sequence (Complete)

Based on all findings, the `setup` command should execute:

```
1.  Verify Hyper-V role enabled and vmms service running
2.  Verify sufficient resources (RAM, disk)
3.  Create Gen 2 VM with specified resources
4.  Disable Secure Boot: Set-VMFirmware -EnableSecureBoot Off
5.  Attach Windows ISO, boot, install Windows
6.  Enable Guest Service Interface integration service
7.  Wait for guest OS boot completion (readiness probe)
8.  Create local admin account via PS Direct
9.  Store credentials in config (or credential store)
10. Configure guest via PS Direct:
    a. bcdedit /set testsigning on
    b. Set DbgPrint filter: reg add "HKLM\...\Debug Print Filter" /v DEFAULT /d 0xFFFFFFFF
    c. Disable Windows Update (optional, prevents surprise reboots)
    d. Disable automatic restart after BSOD (for debugging)
11. Reboot guest (required for testsigning)
12. Wait for guest readiness after reboot
13. Take baseline snapshot: Checkpoint-VM -SnapshotName "baseline-driver-env"
14. Report setup complete with connection details
```

---

*End of review. All findings are based on documented Windows behavior and practical experience with Hyper-V driver testing environments. Test verification on a representative Windows 11 24H2 guest VM is recommended before finalizing the design.*
