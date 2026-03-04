#Requires -RunAsAdministrator
<#
.SYNOPSIS
    POC-3: File Copy Host → Guest
.DESCRIPTION
    Validates: Copy-VMFile, Copy-Item -ToSession, Integration Services detection.
#>
param([string]$VMName)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Write-Result($Name, $Pass, $Detail) {
    $icon = if ($Pass) { '[PASS]' } else { '[FAIL]' }
    Write-Host "$icon $Name" -ForegroundColor $(if ($Pass) { 'Green' } else { 'Red' })
    if ($Detail) { Write-Host "       $Detail" -ForegroundColor DarkGray }
}

Write-Host "`n=== POC-3: File Copy Host → Guest ===" -ForegroundColor Cyan

# Auto-detect running VM
if (-not $VMName) {
    $vm = Get-VM | Where-Object { $_.State -eq 'Running' } | Select-Object -First 1
    if (-not $vm) { Write-Host "No running VMs found" -ForegroundColor Red; exit 1 }
    $VMName = $vm.Name
}
Write-Host "Using VM: $VMName" -ForegroundColor Yellow

# --- Test 1: Check Integration Services ---
try {
    $gsi = Get-VMIntegrationService -VMName $VMName | Where-Object { $_.Name -eq 'Guest Service Interface' }
    $gsiEnabled = $gsi.Enabled
    $gsiRunning = $gsi.PrimaryStatusDescription -eq 'OK'
    Write-Result "Guest Service Interface" ($gsiEnabled -and $gsiRunning) "Enabled=$gsiEnabled Status=$($gsi.PrimaryStatusDescription)"

    if (-not $gsiEnabled) {
        Write-Host "  Enabling Guest Service Interface..." -ForegroundColor Yellow
        Enable-VMIntegrationService -VMName $VMName -Name 'Guest Service Interface'
        Start-Sleep -Seconds 3
        Write-Result "Enable GSI" $true "Guest Service Interface enabled"
    }
} catch {
    Write-Result "Guest Service Interface" $false $_.Exception.Message
}

# --- Test 2: Copy-VMFile ---
$testFile = "$env:TEMP\poc3_test_$(Get-Random).txt"
$guestPath = "C:\poc-test\"
Set-Content -Path $testFile -Value "POC-3 test file created at $(Get-Date)"

try {
    Copy-VMFile -Name $VMName -SourcePath $testFile -DestinationPath $guestPath -FileSource Host -CreateFullPath -Force
    Write-Result "Copy-VMFile" $true "Copied $testFile → guest:$guestPath"
} catch {
    Write-Result "Copy-VMFile" $false $_.Exception.Message
    Write-Host "  NOTE: Copy-VMFile requires Guest Service Interface enabled" -ForegroundColor Yellow
}

# --- Test 3: Verify file arrived via PS Direct ---
# This tests both file copy and PS Direct together
try {
    $cred = $null
    # Try credentialless first
    try {
        $verifyResult = Invoke-Command -VMName $VMName -ScriptBlock {
            $path = "C:\poc-test\$(Split-Path $using:testFile -Leaf)"
            if (Test-Path $path) {
                @{ Exists = $true; Content = Get-Content $path -Raw; Size = (Get-Item $path).Length }
            } else {
                @{ Exists = $false }
            }
        } -ErrorAction Stop
    } catch {
        Write-Host "  Credentialless PS Direct failed, prompting for credentials..." -ForegroundColor Yellow
        $cred = Get-Credential -Message "Enter credentials for VM '$VMName'"
        $verifyResult = Invoke-Command -VMName $VMName -Credential $cred -ScriptBlock {
            $path = "C:\poc-test\$(Split-Path $using:testFile -Leaf)"
            if (Test-Path $path) {
                @{ Exists = $true; Content = Get-Content $path -Raw; Size = (Get-Item $path).Length }
            } else {
                @{ Exists = $false }
            }
        }
    }
    Write-Result "Verify File in Guest" $verifyResult.Exists "Size=$($verifyResult.Size) bytes"
} catch {
    Write-Result "Verify File in Guest" $false $_.Exception.Message
}

# --- Test 4: Copy-Item -ToSession alternative ---
try {
    $session = if ($cred) {
        New-PSSession -VMName $VMName -Credential $cred
    } else {
        New-PSSession -VMName $VMName
    }
    $testFile2 = "$env:TEMP\poc3_session_$(Get-Random).txt"
    Set-Content -Path $testFile2 -Value "Session copy test"

    Copy-Item -Path $testFile2 -Destination "C:\poc-test\" -ToSession $session -Force
    Write-Result "Copy-Item -ToSession" $true "Alternative file copy method works"

    Remove-PSSession $session
    Remove-Item $testFile2 -ErrorAction SilentlyContinue
} catch {
    Write-Result "Copy-Item -ToSession" $false $_.Exception.Message
}

# Cleanup
Remove-Item $testFile -ErrorAction SilentlyContinue

Write-Host "`n=== POC-3 Complete ===" -ForegroundColor Cyan
