# Windows Expert Review: Design Reality Check

**Reviewer**: Windows Driver & Internals Expert
**Date**: 2025-11-12
**Scope**: Verification of specific Windows behaviors against the proposed design.

## 1. Driver Installation & Device Creation

**Findings:**
- `pnputil /add-driver <inf> /install` **does not create device nodes** for software-only drivers (like the `echo` sample). It only stages the driver and installs it on *existing* matching device nodes.
- For root-enumerated drivers (software drivers), you must manually create the device node.
- **Impact**: The current design will fail for the `echo` sample because the driver will be added to the store but never loaded, as no device exists to bind to.

**Recommendation:**
- Detect if the driver is a root-enumerated software driver (e.g., via `[Standard.NT$ARCH]` section in INF lacking hardware ID matching, or explicit config).
- For these drivers, use `devcon install <inf> <hwid>` instead of `pnputil`.
- Alternatively, on Windows 10/11, use `pnputil /add-device <inf> /install` (if available and applicable) or script the device creation via `swprv`.
- **Simplest Fix**: Add a `provision-device` step in `driver-test.toml` allowing users to specify a hardware ID to create via `devcon` before/during installation.

## 2. Debug Output on Modern Windows

**Findings:**
- **DbgPrint Filtering**: As suspected, Vista+ filters virtually all DbgPrint output by default. `Dbgview.exe` cannot bypass this filter; the filter happens in the kernel before the debugger/listener sees it.
- **Registry Key**: You **must** set the mask.
  - Path: `HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Debug Print Filter`
  - Value: `DEFAULT` (DWORD) = `0xFFFFFFFF` (to capture everything).
- **UMDF**: `OutputDebugString` (used by WDK crate macros for UMDF) is captured by `Dbgview.exe /g`. This is correct.
- **Secure Boot**: `Dbgv.sys` (DebugView's driver) is signed by Sysinternals but often blocked by Secure Boot policies on strict Gen 2 VMs unless explicitly allowed or Secure Boot is disabled.

**Recommendation:**
- The `setup` command **MUST** set the `Debug Print Filter` registry key. Without this, you will get zero kernel logs.
- The `setup` command **MUST** disable Secure Boot (`Set-VMFirmware -EnableSecureBoot Off`) to allow `Dbgv.sys` and the test-signed driver to load.

## 3. The Test Certificate Chain

**Findings:**
- **Mechanism**: `cargo-wdk` (specifically `package_task.rs`) generates a self-signed root certificate named `WDRLocalTestCert` in a local store `WDRTestCertStore`.
- **Chain**: Self-signed Root (`CN=WDRLocalTestCert`) -> Driver (`.sys`, `.cat`). There is no intermediate CA.
- **Artifacts**: The public key is exported to `WDRLocalTestCert.cer` in the package folder.
- **VM Requirements**:
  1.  **Store**: The `.cer` must be imported to **LocalMachine\Root** (Trusted Root) AND **LocalMachine\TrustedPeople**.
  2.  **BCD**: `bcdedit /set testsigning on` is **MANDATORY**. Installing the cert is not enough.
  3.  **Secure Boot**: MUST be **OFF**. A self-signed root is not in the UEFI DB, and `testsigning` is ignored if Secure Boot is active.

**Recommendation:**
- Update `deploy` step to run `bcdedit /set testsigning on` and reboot if not enabled (or enforce in `setup`).
- Enforce Secure Boot disablement in `setup`.

## 4. PnPUtil Structured Output

**Findings:**
- **Correction**: `pnputil /enum-drivers /format xml` **IS SUPPORTED** on modern Windows (tested on Windows 11).
- **Output**: Returns valid XML with `<DriverName>`, `<ProviderName>`, `<DriverVersion>`, etc.
- **Localization**: Tag names (e.g., `<OriginalName>`) are stable and not localized. Values (e.g., date in `DriverVersion`) *might* be localized, but the structure is reliable.

**Recommendation:**
- **Do not parse text.** Use `pnputil /enum-drivers /format xml` and parse the XML.
- This is significantly faster than `Get-WindowsDriver` and more robust than text scraping.
- **Caveat**: `DriverVersion` field in XML is `MM/DD/YYYY H.L.V.B`. You will still need to handle date format variations, or prefer `Get-CimInstance Win32_PnPSignedDriver` for the pure version string if XML date parsing proves flaky.

## Summary of Actionable Changes

1.  **Add `devcon` support** or a device creation strategy for the `echo` driver.
2.  **Add Registry Write** for `Debug Print Filter` in VM setup.
3.  **Disable Secure Boot** in VM setup.
4.  **Enable Test Signing** (`bcdedit`) in VM setup.
5.  **Switch to XML parsing** for `pnputil` output.
