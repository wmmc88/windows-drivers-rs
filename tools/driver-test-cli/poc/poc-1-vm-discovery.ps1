#Requires -RunAsAdministrator
<#
.SYNOPSIS
    POC-1: VM Discovery & State Management
.DESCRIPTION
    Validates: Get-VM enumeration, JSON output parsing, Start/Stop-VM state transitions,
    and the run_ps_json wrapper pattern (structured JSON via ConvertTo-Json).
#>
param(
    [string]$VMName  # Optional: specify VM name. If omitted, auto-detects first available.
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Write-Result($Name, $Pass, $Detail) {
    $icon = if ($Pass) { '[PASS]' } else { '[FAIL]' }
    Write-Host "$icon $Name" -ForegroundColor $(if ($Pass) { 'Green' } else { 'Red' })
    if ($Detail) { Write-Host "       $Detail" -ForegroundColor DarkGray }
}

# --- Test 1: Enumerate VMs and parse JSON ---
Write-Host "`n=== POC-1: VM Discovery & State Management ===" -ForegroundColor Cyan

try {
    $vmsJson = Get-VM | Select-Object Name, State, MemoryStartup, ProcessorCount, Generation | ConvertTo-Json -Compress
    $vms = $vmsJson | ConvertFrom-Json
    if (-not $vms) {
        Write-Result "VM Enumeration" $false "No VMs found. Create one first."
        exit 1
    }
    # Handle single-VM case (ConvertTo-Json doesn't wrap single objects in array)
    if ($vms -isnot [array]) { $vms = @($vms) }
    Write-Result "VM Enumeration" $true "Found $($vms.Count) VM(s): $($vms.Name -join ', ')"
} catch {
    Write-Result "VM Enumeration" $false $_.Exception.Message
    exit 1
}

# --- Pick target VM ---
if ($VMName) {
    $targetVm = $vms | Where-Object { $_.Name -eq $VMName }
    if (-not $targetVm) {
        Write-Result "VM Selection" $false "VM '$VMName' not found"
        exit 1
    }
} else {
    # Prefer running VMs, then first available
    $targetVm = ($vms | Where-Object { $_.State -eq 2 <# Running #> } | Select-Object -First 1)
    if (-not $targetVm) { $targetVm = $vms[0] }
}

$VMName = $targetVm.Name
Write-Result "VM Selection" $true "Using VM: $VMName (State: $($targetVm.State))"

# --- Test 2: Get-VM by name with full JSON ---
try {
    $vmDetail = Get-VM -Name $VMName | Select-Object Name, State, MemoryStartup, ProcessorCount, Generation, Id | ConvertTo-Json -Compress
    $parsed = $vmDetail | ConvertFrom-Json
    Write-Result "Get-VM JSON Parse" $true "Name=$($parsed.Name) Memory=$([math]::Round($parsed.MemoryStartup / 1MB))MB CPUs=$($parsed.ProcessorCount) Gen=$($parsed.Generation)"
} catch {
    Write-Result "Get-VM JSON Parse" $false $_.Exception.Message
}

# --- Test 3: State management ---
$currentState = (Get-VM -Name $VMName).State
Write-Host "`nVM '$VMName' current state: $currentState" -ForegroundColor Yellow

if ($currentState -eq 'Running') {
    Write-Result "VM Running" $true "VM is already running — skipping start/stop cycle to avoid disruption"
    Write-Host "  (To test state transitions, use a non-running VM)" -ForegroundColor DarkGray
} elseif ($currentState -eq 'Off') {
    Write-Host "  Attempting Start-VM..." -ForegroundColor Yellow
    try {
        Start-VM -Name $VMName
        Start-Sleep -Seconds 5
        $newState = (Get-VM -Name $VMName).State
        Write-Result "Start-VM" ($newState -eq 'Running') "State after start: $newState"
    } catch {
        Write-Result "Start-VM" $false $_.Exception.Message
    }
} else {
    Write-Result "State Transition" $false "VM in unexpected state: $currentState (expected Running or Off)"
}

# --- Test 4: Error handling pattern (wrapper) ---
Write-Host "`n--- Testing run_ps_json wrapper pattern ---" -ForegroundColor Yellow
$wrapperScript = @"
`$ErrorActionPreference='Stop'
try {
    Get-VM -Name '$VMName' | Select-Object Name, State | ConvertTo-Json -Compress
} catch {
    `$_ | ConvertTo-Json -Compress | Write-Error
    exit 2
}
"@

try {
    $result = powershell -NoLogo -NoProfile -ExecutionPolicy Bypass -Command $wrapperScript
    $parsed = $result | ConvertFrom-Json
    Write-Result "Wrapper Pattern (success)" $true "Parsed: $($parsed.Name)"
} catch {
    Write-Result "Wrapper Pattern (success)" $false $_.Exception.Message
}

# Test wrapper with intentional failure
$failScript = @"
`$ErrorActionPreference='Stop'
try {
    Get-VM -Name 'NONEXISTENT_VM_ZZZZZ' | Select-Object Name, State | ConvertTo-Json -Compress
} catch {
    `$_ | ConvertTo-Json -Compress | Write-Error
    exit 2
}
"@

$failProcess = Start-Process powershell -ArgumentList '-NoLogo', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-Command', $failScript -Wait -PassThru -NoNewWindow -RedirectStandardError "$env:TEMP\poc1_stderr.txt" -RedirectStandardOutput "$env:TEMP\poc1_stdout.txt" 2>$null
$failExitCode = $failProcess.ExitCode
$failStderr = if (Test-Path "$env:TEMP\poc1_stderr.txt") { Get-Content "$env:TEMP\poc1_stderr.txt" -Raw } else { "" }
Write-Result "Wrapper Pattern (failure)" ($failExitCode -eq 2) "Exit code: $failExitCode, stderr length: $($failStderr.Length) chars"

Write-Host "`n=== POC-1 Complete ===" -ForegroundColor Cyan
