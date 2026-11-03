Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Network-facing half of the tool cache. Only the Cache Update workflow and
# Update-Manifest.ps1 import this module; consumer jobs never do. Each
# function downloads from the real source, then the result is checked with the
# same Assert-PayloadVerified that consumers run after restoring the cache.

Import-Module (Join-Path $PSScriptRoot 'CiTools.psm1') -DisableNameChecking

function Get-WorkDirectory {
    param([Parameter(Mandatory)][string]$Name)
    $dir = Join-Path ($env:RUNNER_TEMP ?? $env:TEMP) $Name
    if (Test-Path -LiteralPath $dir) { Remove-Item -LiteralPath $dir -Recurse -Force }
    New-Item -ItemType Directory -Path $dir | Out-Null
    $dir
}

function Assert-MicrosoftSignature {
    param([Parameter(Mandatory)][string]$Path)
    $sig = Get-AuthenticodeSignature -LiteralPath $Path
    if ($sig.Status -ne 'Valid') { throw "Authenticode signature of '$Path' is $($sig.Status), expected Valid." }
    $subject = $sig.SignerCertificate.Subject
    if ($subject -notmatch '(^|,\s*)CN=Microsoft Corporation(,|$)') { throw "'$Path' is signed by '$subject', expected Microsoft Corporation." }
}

function Export-WinGetInstaller {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$Id,
        [Parameter(Mandatory)][string]$Version,
        [Parameter(Mandatory)][ValidateSet('X64', 'X86', 'Arm64')][string]$Architecture,
        [Parameter(Mandatory)][string]$Directory
    )

    Import-Module Microsoft.WinGet.Client -ErrorAction Stop
    $attempt = 0
    while ($true) {
        $attempt++
        if (Test-Path -LiteralPath $Directory) { Remove-Item -LiteralPath $Directory -Recurse -Force }
        New-Item -ItemType Directory -Path $Directory | Out-Null
        try {
            $result = Export-WinGetPackage -Id $Id -MatchOption Equals -Version $Version -Architecture $Architecture -DownloadDirectory $Directory -Source winget -ErrorAction Stop
            if ("$($result.Status)" -ne 'Ok') { throw "Status=$($result.Status)" }
            break
        } catch {
            if ($attempt -ge 3) { throw "Export-WinGetPackage $Id $Version $Architecture failed after $attempt attempts: $($_.Exception.Message)" }
            Write-Warning "Export attempt $attempt failed: $($_.Exception.Message)"
            Start-Sleep -Seconds 15
        }
    }

    $exes = @(Get-ChildItem -LiteralPath $Directory -Filter *.exe -File)
    if ($exes.Count -ne 1) {
        throw "Expected exactly one installer for $Id $Version in '$Directory' but found: $((Get-ChildItem -LiteralPath $Directory -Recurse -File | ForEach-Object Name) -join ', ')"
    }
    $exes[0]
}

function Save-LlvmInstaller {
    [CmdletBinding()]
    param([Parameter(Mandatory)]$Payload, [Parameter(Mandatory)][string]$Destination)

    $entry = $Payload.Entry
    $wingetArch = if ($Payload.Arch -eq 'arm64') { 'Arm64' } else { 'X64' }
    $work = Get-WorkDirectory "llvm-$($entry['version'])-$($Payload.Arch)"
    $exe = Export-WinGetInstaller -Id $entry['wingetId'] -Version $entry['version'] -Architecture $wingetArch -Directory $work

    $sig = Get-AuthenticodeSignature -LiteralPath $exe.FullName
    Write-Host "LLVM installer signature: $($sig.Status) $($sig.SignerCertificate.Subject)"

    New-Item -ItemType Directory -Path $Destination -Force | Out-Null
    Copy-Item -LiteralPath $exe.FullName -Destination (Join-Path $Destination 'installer.exe')
}

function Save-KitLayout {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][ValidateSet('sdk', 'wdk')][string]$Component,
        [Parameter(Mandatory)][string]$WingetId,
        [Parameter(Mandatory)][string]$WingetVersion,
        [string]$ExpectedBootstrapperSha256,
        [Parameter(Mandatory)][string]$Destination
    )

    $work = Get-WorkDirectory "kit-$Component"
    # The bootstrapper is 32-bit and its layout is identical on every host
    # architecture, so one layout serves x64 and ARM64 consumers.
    $exe = Export-WinGetInstaller -Id $WingetId -Version $WingetVersion -Architecture X86 -Directory (Join-Path $work 'dl')
    $sha = (Get-FileHash -LiteralPath $exe.FullName -Algorithm SHA256).Hash
    if ($ExpectedBootstrapperSha256 -and $sha -ne $ExpectedBootstrapperSha256) {
        throw "$Component bootstrapper SHA256 is $sha, expected $ExpectedBootstrapperSha256. WinGet is serving different content for $WingetId $WingetVersion."
    }
    Assert-MicrosoftSignature -Path $exe.FullName

    New-Item -ItemType Directory -Path $Destination -Force | Out-Null
    $log = Join-Path $work 'layout.log'
    Invoke-Installer -File $exe.FullName -Arguments @('/quiet', '/layout', "`"$Destination`"", '/log', "`"$log`"") -OkExitCodes @(0) -TimeoutMinutes 40
    [pscustomobject]@{ BootstrapperSha256 = $sha }
}

function Get-NupkgIdentity {
    param([Parameter(Mandatory)][string]$Path)
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($Path)
    try {
        $nuspec = @($zip.Entries | Where-Object { $_.FullName -notmatch '[\\/]' -and $_.Name -like '*.nuspec' })
        if ($nuspec.Count -ne 1) { throw "Expected one root .nuspec in '$Path', found $($nuspec.Count)." }
        $reader = [IO.StreamReader]::new($nuspec[0].Open())
        try { $xml = [xml]$reader.ReadToEnd() } finally { $reader.Dispose() }
    } finally {
        $zip.Dispose()
    }
    [pscustomobject]@{ Id = $xml.package.metadata.id; Version = $xml.package.metadata.version }
}

function Save-NugetFeed {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string[]]$Roots,
        [Parameter(Mandatory)][string]$NugetVersion,
        [Parameter(Mandatory)][string]$Destination
    )

    $work = Get-WorkDirectory 'nuget-harvest'
    $config = Join-Path $work 'nuget.config'
    Set-Content -LiteralPath $config -Encoding utf8 -Value '<?xml version="1.0" encoding="utf-8"?><configuration><packageSources><clear /><add key="nuget.org" value="https://api.nuget.org/v3/index.json" protocolVersion="3" /></packageSources></configuration>'

    $installDir = Join-Path $work 'packages'
    foreach ($root in $Roots) {
        Write-Host "Resolving $root $NugetVersion from nuget.org"
        & nuget install $root -Version $NugetVersion -OutputDirectory $installDir -ConfigFile $config -NoCache -DirectDownload -NonInteractive -Verbosity quiet
        if ($LASTEXITCODE -ne 0) { throw "nuget install $root $NugetVersion failed with exit code $LASTEXITCODE." }
    }

    New-Item -ItemType Directory -Path $Destination -Force | Out-Null
    $packages = foreach ($file in @(Get-ChildItem -LiteralPath $installDir -Recurse -Filter *.nupkg -File)) {
        $identity = Get-NupkgIdentity -Path $file.FullName

        & nuget verify -Signatures -All $file.FullName
        if ($LASTEXITCODE -ne 0) { throw "nuget verify failed for $($file.Name) with exit code $LASTEXITCODE." }

        $target = Join-Path $Destination "$($identity.Id).$($identity.Version).nupkg"
        Copy-Item -LiteralPath $file.FullName -Destination $target
        [pscustomobject]@{
            id      = $identity.Id
            version = $identity.Version
            sha256  = (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash
            bytes   = (Get-Item -LiteralPath $target).Length
        }
    }
    @($packages | Sort-Object id, version)
}

function Invoke-PayloadAcquire {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]$Manifest,
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][string]$Workspace
    )

    $candidates = @(Get-AllPayloads -Manifest $Manifest | Where-Object { $_.Name -eq $Name })
    if ($candidates.Count -ne 1) { throw "'$Name' does not identify exactly one payload in the manifest." }
    $payload = $candidates[0]
    $dir = Join-Path $Workspace $payload.Path
    if (Test-Path -LiteralPath $dir) { Remove-Item -LiteralPath $dir -Recurse -Force }

    switch ($payload.Kind) {
        'llvm' {
            Save-LlvmInstaller -Payload $payload -Destination $dir
        }
        'kits-layout' {
            foreach ($c in 'sdk', 'wdk') {
                $spec = $payload.Entry['components'][$c]
                Save-KitLayout -Component $c -WingetId $spec['wingetId'] -WingetVersion $spec['wingetVersion'] `
                    -ExpectedBootstrapperSha256 $spec['bootstrapperSha256'] -Destination (Join-Path $dir $c) | Out-Null
            }
        }
        'kits-nuget' {
            $found = Save-NugetFeed -Roots $payload.Entry['roots'] -NugetVersion $payload.Entry['nugetVersion'] -Destination $dir
            $listed = @($found | ForEach-Object { "$($_.id)|$($_.version)".ToLowerInvariant() } | Sort-Object)
            $pinned = @($payload.Entry['packages'] | ForEach-Object { "$($_['id'])|$($_['version'])".ToLowerInvariant() } | Sort-Object)
            if (($listed -join ',') -ne ($pinned -join ',')) {
                throw "nuget.org resolved a different closure for $($payload.Entry['nugetVersion']). Pinned: $($pinned -join ', '). Resolved: $($listed -join ', ')."
            }
        }
        default { throw "Unknown payload kind '$($payload.Kind)'." }
    }

    Assert-PayloadVerified -Payload $payload -PayloadDir $dir
    Write-Host "Payload $Name verified against the manifest ($($payload.Path))."
    $payload
}

Export-ModuleMember -Function @(
    'Get-WorkDirectory', 'Assert-MicrosoftSignature', 'Export-WinGetInstaller',
    'Save-LlvmInstaller', 'Save-KitLayout', 'Get-NupkgIdentity', 'Save-NugetFeed',
    'Invoke-PayloadAcquire'
)
