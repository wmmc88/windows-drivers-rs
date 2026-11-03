<#
.SYNOPSIS
Adds or refreshes pins in manifest.json by downloading the real artifacts and recording their digests.

.DESCRIPTION
Run this from a Windows machine with WinGet and nuget on PATH when a matrix
entry needs a new pin, then review the resulting manifest diff in the pull
request. The recorded digests are what Cache Update and every consumer job
verify against, so they are trusted on first use here and must be reviewed.

.EXAMPLE
.\Update-Manifest.ps1 -AddLlvm 19.1.7

.EXAMPLE
.\Update-Manifest.ps1 -RefreshNuget 10.0.26100

.EXAMPLE
.\Update-Manifest.ps1 -RefreshNuget 10.0.26100 -NugetVersion 10.0.26100.6584 -OutPath $env:TEMP\candidate.json
#>
[CmdletBinding()]
param(
    # LLVM x.y.z to pin for both architectures.
    [string]$AddLlvm,
    # Kit version (10.0.NNNNN) whose NuGet closure should be re-resolved.
    [string]$RefreshNuget,
    # Exact NuGet version for -RefreshNuget. Defaults to the newest QFE on nuget.org.
    [string]$NugetVersion,
    # Rewrites the manifest in canonical formatting without any network access.
    [switch]$Format,
    # Increments the cache generation, which invalidates every cache key.
    [switch]$BumpGeneration,
    [string]$ManifestPath = (Join-Path $PSScriptRoot 'manifest.json'),
    [string]$OutPath = $ManifestPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (-not ($AddLlvm -or $RefreshNuget -or $Format -or $BumpGeneration)) {
    throw 'Nothing to do. Pass -AddLlvm, -RefreshNuget, -BumpGeneration or -Format.'
}
if ($NugetVersion -and -not $RefreshNuget) { throw '-NugetVersion needs -RefreshNuget.' }

Import-Module (Join-Path $PSScriptRoot 'CiTools.psm1') -Force -DisableNameChecking
Import-Module (Join-Path $PSScriptRoot 'CiToolsAcquire.psm1') -Force -DisableNameChecking

$manifest = Get-Content -LiteralPath $ManifestPath -Raw | ConvertFrom-Json -Depth 20

if ($BumpGeneration) { $manifest.generation = [int]$manifest.generation + 1 }

if ($AddLlvm) {
    if ($AddLlvm -notmatch '^\d+\.\d+\.\d+$') { throw "Invalid LLVM version '$AddLlvm'." }
    $existing = @($manifest.llvm | Where-Object { $_.version -eq $AddLlvm })
    $wingetId = if ($existing.Count -eq 1) { $existing[0].wingetId } else { 'LLVM.LLVM' }

    $installers = [ordered]@{}
    foreach ($arch in 'x64', 'arm64') {
        $wingetArch = if ($arch -eq 'arm64') { 'Arm64' } else { 'X64' }
        $work = Get-WorkDirectory "update-llvm-$AddLlvm-$arch"
        $exe = Export-WinGetInstaller -Id $wingetId -Version $AddLlvm -Architecture $wingetArch -Directory $work
        $installers[$arch] = [ordered]@{
            sha256 = (Get-FileHash -LiteralPath $exe.FullName -Algorithm SHA256).Hash
            bytes  = $exe.Length
        }
        Write-Host "LLVM $AddLlvm $arch : $($installers[$arch].sha256) ($($exe.Length) bytes)"
    }

    $entry = [ordered]@{ version = $AddLlvm; wingetId = $wingetId; installers = $installers }
    $others = @($manifest.llvm | Where-Object { $_.version -ne $AddLlvm })
    $manifest.llvm = @(@($others) + $entry | Sort-Object { [version]$_.version })
}

if ($RefreshNuget) {
    if ($RefreshNuget -notmatch '^10\.0\.\d{5}$') { throw "Invalid kit version '$RefreshNuget'." }
    $roots = @('Microsoft.Windows.WDK.x64', 'Microsoft.Windows.WDK.ARM64')

    if (-not $NugetVersion) {
        $index = Invoke-RestMethod -Uri 'https://api.nuget.org/v3-flatcontainer/microsoft.windows.wdk.x64/index.json'
        $newest = @($index.versions | Where-Object { $_ -match "^$([regex]::Escape($RefreshNuget))\.\d+$" } | Sort-Object { [version]$_ } -Descending)
        if ($newest.Count -eq 0) { throw "nuget.org has no Microsoft.Windows.WDK.x64 version for $RefreshNuget." }
        $NugetVersion = $newest[0]
    }
    if ($NugetVersion -notmatch "^$([regex]::Escape($RefreshNuget))\.\d+$") { throw "NuGet version '$NugetVersion' does not belong to kit $RefreshNuget." }
    Write-Host "Resolving NuGet closure for $NugetVersion"

    $feed = Get-WorkDirectory "update-feed-$RefreshNuget"
    $found = Save-NugetFeed -Roots $roots -NugetVersion $NugetVersion -Destination $feed

    $packages = @($found | ForEach-Object { [ordered]@{ id = $_.id; version = $_.version; sha256 = $_.sha256; bytes = [int64]$_.bytes } })

    # Hashing an install made from the local feed is what ties the pinned
    # nupkgs to the exact tree that consumer jobs will produce.
    $installRoot = Join-Path (Get-WorkDirectory "update-install-$RefreshNuget") 'packages'
    Install-NugetClosure -Entry @{ roots = $roots; nugetVersion = $NugetVersion; packages = $packages } -FeedDir $feed -PackagesRoot $installRoot
    $tree = Get-TreeHash -Path $installRoot
    Write-Host "Installed tree: $($tree.TreeSha256) ($($tree.Files) files, $($tree.Bytes) bytes)"

    $entry = [ordered]@{
        version             = $RefreshNuget
        source              = 'nuget'
        nugetVersion        = $NugetVersion
        roots               = $roots
        packages            = $packages
        installedTreeSha256 = $tree.TreeSha256
    }
    $others = @($manifest.kits | Where-Object { $_.version -ne $RefreshNuget })
    $manifest.kits = @(@($others) + $entry | Sort-Object { [version]$_.version })
}

$json = ($manifest | ConvertTo-Json -Depth 20) + "`n"
$candidate = Join-Path ([IO.Path]::GetTempPath()) "ci-tools-manifest-$([guid]::NewGuid().ToString('N')).json"
try {
    [IO.File]::WriteAllText($candidate, $json, [Text.UTF8Encoding]::new($false))
    $null = Get-CiManifest -Path $candidate
    Copy-Item -LiteralPath $candidate -Destination $OutPath -Force
} finally {
    Remove-Item -LiteralPath $candidate -ErrorAction SilentlyContinue
}
Write-Host "Wrote $OutPath"
