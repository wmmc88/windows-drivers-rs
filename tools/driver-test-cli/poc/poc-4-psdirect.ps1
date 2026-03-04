#Requires -RunAsAdministrator
<#
.SYNOPSIS
    POC-4: PowerShell Direct Command Execution
.DESCRIPTION
    Validates: Invoke-Command -VMName, stdout/stderr/exit code capture, timeout, credential modes.
#>
param([string]$VMName)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Write-Result($Name, $Pass, $Detail) {
    $icon = if ($Pass) { '[PASS]' } else { '[FAIL]' }
    Write-Host "$icon $Name" -ForegroundColor $(if ($Pass) { 'Green' } else { 'Red' })
    if ($Detail) { Write-Host "       $Detail" -ForegroundColor DarkGray }
}

Write-Host "`n=== POC-4: PowerShell Direct Command Execution ===" -ForegroundColor Cyan

if (-not $VMName) {
    $vm = Get-VM | Where-Object { $_.State -eq 'Running' } | Select-Object -First 1
    if (-not $vm) { Write-Host "No running VMs found" -ForegroundColor Red; exit 1 }
    $VMName = $vm.Name
}
Write-Host "Using VM: $VMName" -ForegroundColor Yellow

# Determine credential mode
$cred = $null
$credMode = "credentialless"
try {
    Invoke-Command -VMName $VMName -ScriptBlock { 1 } -ErrorAction Stop | Out-Null
} catch {
    Write-Host "  Credentialless failed, prompting for credentials..." -ForegroundColor Yellow
    $cred = Get-Credential -Message "Enter credentials for VM '$VMName'"
    $credMode = "explicit"
}
Write-Result "Credential Mode" $true "Using: $credMode"

function Invoke-Guest {
    param([scriptblock]$ScriptBlock)
    if ($cred) {
        Invoke-Command -VMName $VMName -Credential $cred -ScriptBlock $ScriptBlock
    } else {
        Invoke-Command -VMName $VMName -ScriptBlock $ScriptBlock
    }
}

# --- Test 1: Basic command with stdout ---
try {
    $result = Invoke-Guest -ScriptBlock { hostname }
    Write-Result "Basic stdout" $true "Guest hostname: $result"
} catch {
    Write-Result "Basic stdout" $false $_.Exception.Message
}

# --- Test 2: JSON structured output ---
try {
    $result = Invoke-Guest -ScriptBlock {
        @{
            Hostname = $env:COMPUTERNAME
            OS = (Get-CimInstance Win32_OperatingSystem).Caption
            PSVersion = $PSVersionTable.PSVersion.ToString()
            Architecture = $env:PROCESSOR_ARCHITECTURE
        } | ConvertTo-Json -Compress
    }
    $parsed = $result | ConvertFrom-Json
    Write-Result "JSON output" $true "OS=$($parsed.OS) PS=$($parsed.PSVersion) Arch=$($parsed.Architecture)"
} catch {
    Write-Result "JSON output" $false $_.Exception.Message
}

# --- Test 3: Exit code capture ---
try {
    $result = Invoke-Guest -ScriptBlock {
        cmd /c "exit 42"
        $LASTEXITCODE
    }
    Write-Result "Exit code capture" ($result -eq 42) "Got exit code: $result"
} catch {
    Write-Result "Exit code capture" $false $_.Exception.Message
}

# --- Test 4: Stderr capture ---
try {
    $result = Invoke-Guest -ScriptBlock {
        $errOut = cmd /c "echo this is stderr 1>&2" 2>&1
        @{
            HasError = ($errOut | Where-Object { $_ -is [System.Management.Automation.ErrorRecord] }).Count -gt 0
            Output = "$errOut"
        } | ConvertTo-Json -Compress
    }
    $parsed = $result | ConvertFrom-Json
    Write-Result "Stderr capture" $true "HasError=$($parsed.HasError) Output=$($parsed.Output)"
} catch {
    Write-Result "Stderr capture" $false $_.Exception.Message
}

# --- Test 5: Timeout behavior ---
try {
    $job = if ($cred) {
        Invoke-Command -VMName $VMName -Credential $cred -ScriptBlock { Start-Sleep -Seconds 60 } -AsJob
    } else {
        Invoke-Command -VMName $VMName -ScriptBlock { Start-Sleep -Seconds 60 } -AsJob
    }
    $completed = $job | Wait-Job -Timeout 5
    if (-not $completed) {
        Stop-Job $job
        Remove-Job $job -Force
        Write-Result "Timeout (5s)" $true "Job correctly timed out and was killed"
    } else {
        Remove-Job $job -Force
        Write-Result "Timeout (5s)" $false "Job completed unexpectedly (should have timed out)"
    }
} catch {
    Write-Result "Timeout (5s)" $false $_.Exception.Message
}

# --- Test 6: PS session reuse performance ---
try {
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $session = if ($cred) {
        New-PSSession -VMName $VMName -Credential $cred
    } else {
        New-PSSession -VMName $VMName
    }
    $sessionCreateMs = $sw.ElapsedMilliseconds

    $sw.Restart()
    $r1 = Invoke-Command -Session $session -ScriptBlock { 1 + 1 }
    $firstCallMs = $sw.ElapsedMilliseconds

    $sw.Restart()
    $r2 = Invoke-Command -Session $session -ScriptBlock { 2 + 2 }
    $secondCallMs = $sw.ElapsedMilliseconds

    Remove-PSSession $session
    Write-Result "Session Reuse" $true "Create=${sessionCreateMs}ms First=${firstCallMs}ms Reuse=${secondCallMs}ms"
} catch {
    Write-Result "Session Reuse" $false $_.Exception.Message
}

Write-Host "`n=== POC-4 Complete ===" -ForegroundColor Cyan
