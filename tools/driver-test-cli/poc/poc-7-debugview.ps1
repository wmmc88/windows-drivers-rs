#Requires -RunAsAdministrator
<#
.SYNOPSIS
    POC-7: DebugView Deployment & Log Capture
.DESCRIPTION
    Validates: Dbgview.exe download, deployment to guest, launch with /k /g /t /q,
    log file creation, debug output capture, DbgPrint filter registry key.
#>
param([string]$VMName)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Write-Result($Name, $Pass, $Detail) {
    $icon = if ($Pass) { '[PASS]' } else { '[FAIL]' }
    Write-Host "$icon $Name" -ForegroundColor $(if ($Pass) { 'Green' } else { 'Red' })
    if ($Detail) { Write-Host "       $Detail" -ForegroundColor DarkGray }
}

Write-Host "`n=== POC-7: DebugView Deployment & Log Capture ===" -ForegroundColor Cyan

if (-not $VMName) {
    $vm = Get-VM | Where-Object { $_.State -eq 'Running' } | Select-Object -First 1
    if (-not $vm) { Write-Host "No running VMs found" -ForegroundColor Red; exit 1 }
    $VMName = $vm.Name
}
Write-Host "Using VM: $VMName" -ForegroundColor Yellow

$cred = $null
try {
    Invoke-Command -VMName $VMName -ScriptBlock { 1 } -ErrorAction Stop | Out-Null
} catch {
    $cred = Get-Credential -Message "Enter credentials for VM '$VMName'"
}

function Invoke-Guest {
    param([scriptblock]$ScriptBlock)
    if ($cred) {
        Invoke-Command -VMName $VMName -Credential $cred -ScriptBlock $ScriptBlock
    } else {
        Invoke-Command -VMName $VMName -ScriptBlock $ScriptBlock
    }
}

# --- Test 1: Download DebugView on host ---
$dbgViewPath = "$env:TEMP\Dbgview.exe"
$dbgViewUrl = "https://live.sysinternals.com/Dbgview.exe"

if (Test-Path $dbgViewPath) {
    Write-Result "DebugView Present" $true "Already downloaded: $dbgViewPath"
} else {
    try {
        Write-Host "  Downloading from $dbgViewUrl..." -ForegroundColor Yellow
        Invoke-WebRequest -Uri $dbgViewUrl -OutFile $dbgViewPath -UseBasicParsing
        $size = (Get-Item $dbgViewPath).Length
        Write-Result "Download DebugView" ($size -gt 100000) "Size: $([math]::Round($size/1KB)) KB"
    } catch {
        Write-Result "Download DebugView" $false $_.Exception.Message
        Write-Host "  NOTE: If behind a proxy, manually download Dbgview.exe to $dbgViewPath" -ForegroundColor Yellow
        exit 1
    }
}

# Hash verification (informational — pin this in production)
$hash = (Get-FileHash $dbgViewPath -Algorithm SHA256).Hash
Write-Host "  SHA256: $hash" -ForegroundColor DarkGray
Write-Host "  (Pin this hash in production for supply-chain safety)" -ForegroundColor DarkGray

# --- Test 2: Copy DebugView to guest ---
try {
    Copy-VMFile -Name $VMName -SourcePath $dbgViewPath -DestinationPath "C:\Tools\" -FileSource Host -CreateFullPath -Force
    Write-Result "Copy to Guest" $true "Copied to C:\Tools\Dbgview.exe"
} catch {
    Write-Result "Copy to Guest" $false $_.Exception.Message
    exit 1
}

# --- Test 3: Configure DbgPrint filter (required for modern Windows) ---
try {
    $result = Invoke-Guest -ScriptBlock {
        $regPath = "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Debug Print Filter"
        if (-not (Test-Path $regPath)) {
            New-Item -Path $regPath -Force | Out-Null
        }
        Set-ItemProperty -Path $regPath -Name "DEFAULT" -Value 0xFFFFFFFF -Type DWord
        $val = Get-ItemProperty -Path $regPath -Name "DEFAULT" -ErrorAction SilentlyContinue
        @{ Value = $val.DEFAULT; Path = $regPath } | ConvertTo-Json -Compress
    }
    $parsed = $result | ConvertFrom-Json
    Write-Result "DbgPrint Filter" ($parsed.Value -eq -1 <# 0xFFFFFFFF as signed int #>) "DEFAULT=0x$($parsed.Value.ToString('X8'))"
} catch {
    Write-Result "DbgPrint Filter" $false $_.Exception.Message
}

# --- Test 4: Launch DebugView ---
$logPath = "C:\DriverLogs\dbwin.log"
try {
    Invoke-Guest -ScriptBlock {
        # Kill any existing DebugView
        Get-Process Dbgview -ErrorAction SilentlyContinue | Stop-Process -Force
        Start-Sleep -Seconds 1

        # Create log directory
        New-Item -ItemType Directory -Path "C:\DriverLogs" -Force | Out-Null

        # Launch DebugView
        # /k = kernel capture, /g = global Win32, /t = tray, /q = quiet, /l = log to file
        Start-Process "C:\Tools\Dbgview.exe" -ArgumentList '/k', '/g', '/t', '/q', '/accepteula', '/l', 'C:\DriverLogs\dbwin.log' -WindowStyle Hidden
        Start-Sleep -Seconds 3

        $proc = Get-Process Dbgview -ErrorAction SilentlyContinue
        @{
            Running = ($null -ne $proc)
            PID = if ($proc) { $proc.Id } else { $null }
            LogExists = Test-Path "C:\DriverLogs\dbwin.log"
        } | ConvertTo-Json -Compress
    }
    $parsed = $result | ConvertFrom-Json
    Write-Result "Launch DebugView" $parsed.Running "PID=$($parsed.PID) LogExists=$($parsed.LogExists)"
} catch {
    Write-Result "Launch DebugView" $false $_.Exception.Message
}

# --- Test 5: Generate test debug output (user-mode) ---
try {
    $result = Invoke-Guest -ScriptBlock {
        # Use OutputDebugString via .NET to generate test messages
        Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class DebugOutput {
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
    public static extern void OutputDebugString(string message);
}
"@
        for ($i = 1; $i -le 5; $i++) {
            [DebugOutput]::OutputDebugString("POC7_TEST_MESSAGE_$i")
            Start-Sleep -Milliseconds 100
        }
        Start-Sleep -Seconds 2  # Wait for DebugView to flush

        # Read the log file
        $logContent = if (Test-Path "C:\DriverLogs\dbwin.log") {
            Get-Content "C:\DriverLogs\dbwin.log" -Raw -ErrorAction SilentlyContinue
        } else { "" }

        @{
            MessagesSent = 5
            LogSize = $logContent.Length
            ContainsTestMsg = $logContent -match "POC7_TEST_MESSAGE"
            SampleLines = ($logContent -split "`n" | Select-Object -Last 10) -join "`n"
        } | ConvertTo-Json -Compress
    }
    $parsed = $result | ConvertFrom-Json
    Write-Result "Debug Output Capture" $parsed.ContainsTestMsg "Sent=$($parsed.MessagesSent) LogSize=$($parsed.LogSize) Found=$($parsed.ContainsTestMsg)"
    if ($parsed.SampleLines) {
        Write-Host "  Last 10 lines:" -ForegroundColor DarkGray
        $parsed.SampleLines -split "`n" | ForEach-Object { Write-Host "    $_" -ForegroundColor DarkGray }
    }
} catch {
    Write-Result "Debug Output Capture" $false $_.Exception.Message
}

# --- Test 6: Tail log file from host (simulates streaming) ---
try {
    $tailResult = Invoke-Guest -ScriptBlock {
        if (Test-Path "C:\DriverLogs\dbwin.log") {
            $lines = Get-Content "C:\DriverLogs\dbwin.log" -Tail 5
            @{ Lines = $lines; Count = $lines.Count } | ConvertTo-Json -Compress
        } else {
            @{ Lines = @(); Count = 0 } | ConvertTo-Json -Compress
        }
    }
    $parsed = $tailResult | ConvertFrom-Json
    Write-Result "Tail Log (last 5)" ($parsed.Count -gt 0) "Got $($parsed.Count) line(s)"
} catch {
    Write-Result "Tail Log" $false $_.Exception.Message
}

# --- Cleanup ---
try {
    Invoke-Guest -ScriptBlock {
        Get-Process Dbgview -ErrorAction SilentlyContinue | Stop-Process -Force
        Remove-Item "C:\DriverLogs\dbwin.log" -ErrorAction SilentlyContinue
    }
    Write-Result "Cleanup" $true "DebugView stopped, log removed"
} catch {
    Write-Result "Cleanup" $false $_.Exception.Message
}

Write-Host "`n=== POC-7 Complete ===" -ForegroundColor Cyan
