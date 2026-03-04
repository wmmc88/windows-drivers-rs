# POC Scripts

Run each script in an **elevated PowerShell terminal** (Admin required for Hyper-V).

Scripts auto-detect the test VM. Results are printed inline.

## Running

```powershell
cd tools\driver-test-cli\poc
.\poc-1-vm-discovery.ps1
.\poc-2-snapshot.ps1
.\poc-3-filecopy.ps1
.\poc-4-psdirect.ps1
.\poc-5-pnputil.ps1
.\poc-6-cert.ps1
.\poc-7-debugview.ps1
```

POC-8 (detection) is a Rust test, not a PowerShell script.

## Prerequisites

- Hyper-V enabled with at least one test VM running Windows
- WDK installed (for devcon in POC-5)
- Admin privileges
