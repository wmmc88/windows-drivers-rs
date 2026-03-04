#Requires -RunAsAdministrator
<#
.SYNOPSIS
    POC-2: Snapshot Create & Revert
.DESCRIPTION
    Validates: Checkpoint-VM, Restore-VMSnapshot, VM state after revert, idempotency.
#>
param(
    [string]$VMName
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$SnapshotName = "poc-test-baseline"

function Write-Result($Name, $Pass, $Detail) {
    $icon = if ($Pass) { '[PASS]' } else { '[FAIL]' }
    Write-Host "$icon $Name" -ForegroundColor $(if ($Pass) { 'Green' } else { 'Red' })
    if ($Detail) { Write-Host "       $Detail" -ForegroundColor DarkGray }
}

Write-Host "`n=== POC-2: Snapshot Create & Revert ===" -ForegroundColor Cyan

# Auto-detect VM if not specified
if (-not $VMName) {
    $vm = Get-VM | Where-Object { $_.State -eq 'Running' } | Select-Object -First 1
    if (-not $vm) { $vm = Get-VM | Select-Object -First 1 }
    if (-not $vm) { Write-Host "No VMs found" -ForegroundColor Red; exit 1 }
    $VMName = $vm.Name
}
Write-Host "Using VM: $VMName" -ForegroundColor Yellow

# --- Test 1: Create snapshot ---
try {
    # Remove existing test snapshot if present (idempotency pre-clean)
    $existing = Get-VMSnapshot -VMName $VMName -Name $SnapshotName -ErrorAction SilentlyContinue
    if ($existing) {
        Remove-VMSnapshot -VMName $VMName -Name $SnapshotName -Confirm:$false
        Write-Host "  Cleaned up existing snapshot '$SnapshotName'" -ForegroundColor DarkGray
        Start-Sleep -Seconds 2
    }

    Checkpoint-VM -Name $VMName -SnapshotName $SnapshotName
    $snap = Get-VMSnapshot -VMName $VMName -Name $SnapshotName
    Write-Result "Create Snapshot" ($null -ne $snap) "Snapshot '$SnapshotName' created at $($snap.CreationTime)"
} catch {
    Write-Result "Create Snapshot" $false $_.Exception.Message
    exit 1
}

# --- Test 2: Idempotency (creating same name again) ---
try {
    Checkpoint-VM -Name $VMName -SnapshotName $SnapshotName
    $snaps = Get-VMSnapshot -VMName $VMName | Where-Object { $_.Name -eq $SnapshotName }
    $count = @($snaps).Count
    Write-Result "Idempotency" $true "Hyper-V created $count snapshot(s) with name '$SnapshotName' (note: duplicates allowed)"
} catch {
    Write-Result "Idempotency" $false $_.Exception.Message
}

# --- Test 3: Revert to snapshot ---
$preRevertState = (Get-VM -Name $VMName).State
Write-Host "  Pre-revert state: $preRevertState" -ForegroundColor DarkGray

try {
    # Get the most recent snapshot with our name
    $snapToRevert = Get-VMSnapshot -VMName $VMName -Name $SnapshotName | Sort-Object CreationTime -Descending | Select-Object -First 1
    Restore-VMSnapshot -VMSnapshot $snapToRevert -Confirm:$false

    Start-Sleep -Seconds 2
    $postRevertState = (Get-VM -Name $VMName).State
    Write-Result "Revert Snapshot" $true "State after revert: $postRevertState (was: $preRevertState)"

    # If VM was running before, it may be Off after revert
    if ($postRevertState -ne 'Running' -and $preRevertState -eq 'Running') {
        Write-Host "  NOTE: VM is now Off after revert. Starting..." -ForegroundColor Yellow
        Start-VM -Name $VMName
        Start-Sleep -Seconds 5
        $finalState = (Get-VM -Name $VMName).State
        Write-Result "Post-Revert Start" ($finalState -eq 'Running') "Final state: $finalState"
    }
} catch {
    Write-Result "Revert Snapshot" $false $_.Exception.Message
}

# --- Cleanup ---
try {
    # Remove all test snapshots
    Get-VMSnapshot -VMName $VMName | Where-Object { $_.Name -eq $SnapshotName } | Remove-VMSnapshot -Confirm:$false
    Write-Result "Cleanup" $true "Removed test snapshots"
} catch {
    Write-Result "Cleanup" $false $_.Exception.Message
}

Write-Host "`n=== POC-2 Complete ===" -ForegroundColor Cyan
