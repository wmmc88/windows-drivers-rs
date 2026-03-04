#Requires -RunAsAdministrator
<#
.SYNOPSIS
    POC-6: Certificate Installation
.DESCRIPTION
    Validates: Import-Certificate to TrustedPeople/Root, idempotency, verification.
    Uses a self-signed test cert generated on the fly.
#>
param([string]$VMName)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Write-Result($Name, $Pass, $Detail) {
    $icon = if ($Pass) { '[PASS]' } else { '[FAIL]' }
    Write-Host "$icon $Name" -ForegroundColor $(if ($Pass) { 'Green' } else { 'Red' })
    if ($Detail) { Write-Host "       $Detail" -ForegroundColor DarkGray }
}

Write-Host "`n=== POC-6: Certificate Installation ===" -ForegroundColor Cyan

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

# --- Test 1: Generate a test certificate on host ---
$certPath = "$env:TEMP\poc6_test_cert.cer"
try {
    $cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject "CN=POC6-TestCert" -CertStoreLocation Cert:\CurrentUser\My -NotAfter (Get-Date).AddDays(1)
    Export-Certificate -Cert $cert -FilePath $certPath | Out-Null
    $thumbprint = $cert.Thumbprint
    Write-Result "Generate Test Cert" $true "Thumbprint: $thumbprint"
    # Remove from host store (we only need the .cer file)
    Remove-Item "Cert:\CurrentUser\My\$thumbprint" -ErrorAction SilentlyContinue
} catch {
    Write-Result "Generate Test Cert" $false $_.Exception.Message
    exit 1
}

# --- Test 2: Copy cert to guest ---
try {
    Copy-VMFile -Name $VMName -SourcePath $certPath -DestinationPath "C:\poc-test\" -FileSource Host -CreateFullPath -Force
    Write-Result "Copy Cert to Guest" $true "Copied to C:\poc-test\"
} catch {
    Write-Result "Copy Cert to Guest" $false $_.Exception.Message
    exit 1
}

# --- Test 3: Install to TrustedPeople ---
try {
    $result = Invoke-Guest -ScriptBlock {
        $certFile = "C:\poc-test\poc6_test_cert.cer"
        $imported = Import-Certificate -FilePath $certFile -CertStoreLocation Cert:\LocalMachine\TrustedPeople
        @{
            Thumbprint = $imported.Thumbprint
            Subject = $imported.Subject
            Store = "TrustedPeople"
        } | ConvertTo-Json -Compress
    }
    $parsed = $result | ConvertFrom-Json
    Write-Result "Install TrustedPeople" ($parsed.Thumbprint -eq $thumbprint) "Subject=$($parsed.Subject)"
} catch {
    Write-Result "Install TrustedPeople" $false $_.Exception.Message
}

# --- Test 4: Install to Root ---
try {
    $result = Invoke-Guest -ScriptBlock {
        $certFile = "C:\poc-test\poc6_test_cert.cer"
        $imported = Import-Certificate -FilePath $certFile -CertStoreLocation Cert:\LocalMachine\Root
        @{
            Thumbprint = $imported.Thumbprint
            Store = "Root"
        } | ConvertTo-Json -Compress
    }
    $parsed = $result | ConvertFrom-Json
    Write-Result "Install Root" ($parsed.Thumbprint -eq $thumbprint) "Installed to Root store"
} catch {
    Write-Result "Install Root" $false $_.Exception.Message
}

# --- Test 5: Idempotency (install again) ---
try {
    Invoke-Guest -ScriptBlock {
        Import-Certificate -FilePath "C:\poc-test\poc6_test_cert.cer" -CertStoreLocation Cert:\LocalMachine\TrustedPeople | Out-Null
    }
    Write-Result "Idempotency" $true "Re-import succeeded without error"
} catch {
    Write-Result "Idempotency" $false $_.Exception.Message
}

# --- Test 6: Verify cert exists ---
try {
    $result = Invoke-Guest -ScriptBlock {
        param($tp)
        $found = Get-ChildItem "Cert:\LocalMachine\TrustedPeople\$tp" -ErrorAction SilentlyContinue
        @{ Found = ($null -ne $found) } | ConvertTo-Json -Compress
    } -ArgumentList $thumbprint
    # Note: -ArgumentList doesn't work with Invoke-Guest wrapper. Use $using: instead.
    # Retry with $using:
    $result = Invoke-Guest -ScriptBlock {
        $found = Get-ChildItem "Cert:\LocalMachine\TrustedPeople" | Where-Object { $_.Thumbprint -eq $using:thumbprint }
        @{ Found = ($null -ne $found); Count = @($found).Count } | ConvertTo-Json -Compress
    }
    $parsed = $result | ConvertFrom-Json
    Write-Result "Verify in Guest" $parsed.Found "Found=$($parsed.Found) Count=$($parsed.Count)"
} catch {
    Write-Result "Verify in Guest" $false $_.Exception.Message
}

# --- Cleanup ---
try {
    Invoke-Guest -ScriptBlock {
        Get-ChildItem "Cert:\LocalMachine\TrustedPeople" | Where-Object { $_.Subject -eq "CN=POC6-TestCert" } | Remove-Item -Force
        Get-ChildItem "Cert:\LocalMachine\Root" | Where-Object { $_.Subject -eq "CN=POC6-TestCert" } | Remove-Item -Force
        Remove-Item "C:\poc-test\poc6_test_cert.cer" -ErrorAction SilentlyContinue
    }
    Write-Result "Cleanup" $true "Removed test certs from guest"
} catch {
    Write-Result "Cleanup" $false $_.Exception.Message
}

Remove-Item $certPath -ErrorAction SilentlyContinue

Write-Host "`n=== POC-6 Complete ===" -ForegroundColor Cyan
