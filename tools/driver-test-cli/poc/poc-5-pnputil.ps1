#Requires -RunAsAdministrator
<#
.SYNOPSIS
    POC-5: pnputil Driver Enumeration & XML Parsing
.DESCRIPTION
    Validates: pnputil /enum-drivers /format xml, XML parsing, version extraction.
    NOTE: Does NOT install a driver. Just validates enumeration and parsing of existing drivers.
#>
param([string]$VMName)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Write-Result($Name, $Pass, $Detail) {
    $icon = if ($Pass) { '[PASS]' } else { '[FAIL]' }
    Write-Host "$icon $Name" -ForegroundColor $(if ($Pass) { 'Green' } else { 'Red' })
    if ($Detail) { Write-Host "       $Detail" -ForegroundColor DarkGray }
}

Write-Host "`n=== POC-5: pnputil Enumeration & Parsing ===" -ForegroundColor Cyan

if (-not $VMName) {
    $vm = Get-VM | Where-Object { $_.State -eq 'Running' } | Select-Object -First 1
    if (-not $vm) { Write-Host "No running VMs found" -ForegroundColor Red; exit 1 }
    $VMName = $vm.Name
}
Write-Host "Using VM: $VMName" -ForegroundColor Yellow

# Credential setup
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

# --- Test 1: Check if /format xml is supported ---
try {
    $xmlOutput = Invoke-Guest -ScriptBlock {
        $result = pnputil /enum-drivers /format xml 2>&1
        @{
            Output = "$result"
            ExitCode = $LASTEXITCODE
        } | ConvertTo-Json
    }
    $parsed = $xmlOutput | ConvertFrom-Json

    if ($parsed.ExitCode -eq 0 -and $parsed.Output -match '^\s*<') {
        Write-Result "pnputil /format xml" $true "XML output supported!"

        # Try parsing the XML
        try {
            [xml]$xml = $parsed.Output
            $drivers = $xml.SelectNodes('//DriverPackage') ?? $xml.SelectNodes('//*[PublishedName]')
            $driverCount = if ($drivers) { $drivers.Count } else { 0 }
            Write-Result "XML Parse" ($driverCount -gt 0) "Found $driverCount driver package(s) in XML"

            if ($drivers -and $drivers.Count -gt 0) {
                $first = $drivers[0]
                Write-Host "  Sample driver:" -ForegroundColor DarkGray
                Write-Host "    Published: $($first.PublishedName ?? $first.SelectSingleNode('.//PublishedName')?.InnerText ?? 'N/A')" -ForegroundColor DarkGray
                Write-Host "    Version: $($first.DriverVersion ?? $first.SelectSingleNode('.//DriverVersion')?.InnerText ?? 'N/A')" -ForegroundColor DarkGray
            }
        } catch {
            Write-Result "XML Parse" $false "XML parsing failed: $($_.Exception.Message)"
            Write-Host "  First 500 chars of output:" -ForegroundColor DarkGray
            Write-Host "  $($parsed.Output.Substring(0, [Math]::Min(500, $parsed.Output.Length)))" -ForegroundColor DarkGray
        }
    } else {
        Write-Result "pnputil /format xml" $false "Not supported (exit=$($parsed.ExitCode)). Falling back to text."
        Write-Host "  Output: $($parsed.Output.Substring(0, [Math]::Min(200, $parsed.Output.Length)))" -ForegroundColor DarkGray
    }
} catch {
    Write-Result "pnputil /format xml" $false $_.Exception.Message
}

# --- Test 2: Text format fallback ---
try {
    $textOutput = Invoke-Guest -ScriptBlock {
        pnputil /enum-drivers 2>&1 | Out-String
    }
    $lineCount = ($textOutput -split "`n").Count
    $hasPublished = $textOutput -match 'Published Name|nombre publicado'
    Write-Result "pnputil text fallback" $true "$lineCount lines, has 'Published Name': $hasPublished"
} catch {
    Write-Result "pnputil text fallback" $false $_.Exception.Message
}

# --- Test 3: Get-CimInstance alternative ---
try {
    $cimOutput = Invoke-Guest -ScriptBlock {
        Get-CimInstance Win32_PnPSignedDriver |
            Where-Object { $_.InfName -like 'oem*' } |
            Select-Object DeviceName, DriverVersion, InfName, Manufacturer, Signer, IsSigned |
            ConvertTo-Json -Compress
    }
    $drivers = $cimOutput | ConvertFrom-Json
    if ($drivers -isnot [array]) { $drivers = @($drivers) }
    $drivers = @($drivers | Where-Object { $null -ne $_ })
    Write-Result "Get-CimInstance" $true "Found $($drivers.Count) OEM driver(s) via WMI"
    foreach ($d in $drivers | Select-Object -First 3) {
        Write-Host "    $($d.InfName): $($d.DeviceName) v$($d.DriverVersion)" -ForegroundColor DarkGray
    }
} catch {
    Write-Result "Get-CimInstance" $false $_.Exception.Message
}

Write-Host "`n=== POC-5 Complete ===" -ForegroundColor Cyan
