Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Everything a consumer job needs: manifest validation, cache-key planning,
# payload verification, and the one local install path. Nothing in this
# module touches the network, WinGet or the PowerShell Gallery.

$script:Sha256Pattern = '^[0-9A-F]{64}$'
$script:IdPattern = '^[A-Za-z0-9][A-Za-z0-9._-]*$'
$script:LlvmVersionPattern = '^\d+\.\d+\.\d+$'
$script:KitsVersionPattern = '^10\.0\.\d{5}$'
$script:FullVersionPattern = '^\d+(\.\d+){1,3}$'
$script:Architectures = @('x64', 'arm64')
$script:LlvmDir = 'C:\Program Files\LLVM'
$script:NugetPackagesRoot = 'C:\packages'

function Get-CiManifestPath {
    Join-Path $PSScriptRoot 'manifest.json'
}

function Assert-ManifestKeys {
    param($Table, [string[]]$Required, [string]$Where, $Errors)
    if ($Table -isnot [System.Collections.IDictionary]) {
        $Errors.Add("$Where must be an object")
        return $false
    }
    foreach ($k in $Required) {
        if (-not $Table.Contains($k)) { $Errors.Add("$Where is missing '$k'") }
    }
    foreach ($k in $Table.Keys) {
        if ($k -notin $Required) { $Errors.Add("$Where has unexpected key '$k'") }
    }
    $true
}

function Assert-ManifestValue {
    param($Value, [string]$Pattern, [string]$Where, $Errors)
    if ($Value -isnot [string] -or $Value -cnotmatch $Pattern) {
        $Errors.Add("$Where '$Value' does not match $Pattern")
    }
}

function Assert-ManifestBytes {
    param($Value, [string]$Where, $Errors)
    if (($Value -isnot [int] -and $Value -isnot [long]) -or $Value -le 0) {
        $Errors.Add("$Where must be a positive integer")
    }
}

function Test-CiManifestSchema {
    param($Manifest, $Errors)

    if (-not (Assert-ManifestKeys $Manifest @('schemaVersion', 'generation', 'llvm', 'kits') 'manifest' $Errors)) { return }
    if ($Manifest['schemaVersion'] -ne 1) { $Errors.Add("Unsupported schemaVersion '$($Manifest['schemaVersion'])'") }
    if (($Manifest['generation'] -isnot [int] -and $Manifest['generation'] -isnot [long]) -or $Manifest['generation'] -lt 1) { $Errors.Add('generation must be a positive integer') }

    $seen = @{}
    foreach ($e in @($Manifest['llvm'])) {
        if (-not (Assert-ManifestKeys $e @('version', 'wingetId', 'installers') 'llvm entry' $Errors)) { continue }
        $where = "llvm $($e['version'])"
        Assert-ManifestValue $e['version'] $script:LlvmVersionPattern "$where version" $Errors
        Assert-ManifestValue $e['wingetId'] $script:IdPattern "$where wingetId" $Errors
        if ($seen.ContainsKey("llvm|$($e['version'])")) { $Errors.Add("Duplicate $where") }
        $seen["llvm|$($e['version'])"] = $true
        if (-not (Assert-ManifestKeys $e['installers'] $script:Architectures "$where installers" $Errors)) { continue }
        foreach ($arch in $script:Architectures) {
            if (-not (Assert-ManifestKeys $e['installers'][$arch] @('sha256', 'bytes') "$where $arch installer" $Errors)) { continue }
            Assert-ManifestValue $e['installers'][$arch]['sha256'] $script:Sha256Pattern "$where $arch sha256" $Errors
            Assert-ManifestBytes $e['installers'][$arch]['bytes'] "$where $arch bytes" $Errors
        }
    }

    foreach ($e in @($Manifest['kits'])) {
        if ($e -isnot [System.Collections.IDictionary] -or -not $e.Contains('source')) {
            $Errors.Add('kits entry must be an object with a source')
            continue
        }
        $where = "kits $($e['version'])"
        if ($seen.ContainsKey("kits|$($e['version'])")) { $Errors.Add("Duplicate $where") }
        $seen["kits|$($e['version'])"] = $true
        switch ($e['source']) {
            'layout' {
                if (-not (Assert-ManifestKeys $e @('version', 'source', 'components') $where $Errors)) { break }
                Assert-ManifestValue $e['version'] $script:KitsVersionPattern "$where version" $Errors
                if (-not (Assert-ManifestKeys $e['components'] @('sdk', 'wdk') "$where components" $Errors)) { break }
                foreach ($c in 'sdk', 'wdk') {
                    $comp = $e['components'][$c]
                    if (-not (Assert-ManifestKeys $comp @('wingetId', 'wingetVersion', 'bootstrapperSha256', 'layoutTreeSha256') "$where $c" $Errors)) { continue }
                    Assert-ManifestValue $comp['wingetId'] $script:IdPattern "$where $c wingetId" $Errors
                    Assert-ManifestValue $comp['wingetVersion'] $script:FullVersionPattern "$where $c wingetVersion" $Errors
                    Assert-ManifestValue $comp['bootstrapperSha256'] $script:Sha256Pattern "$where $c bootstrapperSha256" $Errors
                    Assert-ManifestValue $comp['layoutTreeSha256'] $script:Sha256Pattern "$where $c layoutTreeSha256" $Errors
                }
            }
            'nuget' {
                if (-not (Assert-ManifestKeys $e @('version', 'source', 'nugetVersion', 'roots', 'packages', 'installedTreeSha256') $where $Errors)) { break }
                Assert-ManifestValue $e['version'] $script:KitsVersionPattern "$where version" $Errors
                Assert-ManifestValue $e['nugetVersion'] '^10\.0\.\d{5}\.\d{1,5}$' "$where nugetVersion" $Errors
                Assert-ManifestValue $e['installedTreeSha256'] $script:Sha256Pattern "$where installedTreeSha256" $Errors
                if (-not ([string]$e['nugetVersion']).StartsWith("$($e['version']).")) {
                    $Errors.Add("$where nugetVersion must extend version")
                }
                foreach ($r in @($e['roots'])) { Assert-ManifestValue $r $script:IdPattern "$where root" $Errors }
                $ids = @{}
                foreach ($p in @($e['packages'])) {
                    if (-not (Assert-ManifestKeys $p @('id', 'version', 'sha256', 'bytes') "$where package" $Errors)) { continue }
                    Assert-ManifestValue $p['id'] $script:IdPattern "$where package id" $Errors
                    Assert-ManifestValue $p['version'] $script:FullVersionPattern "$where package version" $Errors
                    Assert-ManifestValue $p['sha256'] $script:Sha256Pattern "$where package sha256" $Errors
                    Assert-ManifestBytes $p['bytes'] "$where package bytes" $Errors
                    $ids["$($p['id'])|$($p['version'])".ToLowerInvariant()] = $true
                }
                foreach ($r in @($e['roots'])) {
                    if (-not $ids.ContainsKey("$r|$($e['nugetVersion'])".ToLowerInvariant())) {
                        $Errors.Add("$where root '$r' is not in packages at $($e['nugetVersion'])")
                    }
                }
            }
            default { $Errors.Add("$where has unknown source '$($e['source'])'") }
        }
    }
}

function Get-CiManifest {
    [CmdletBinding()]
    param([string]$Path = (Get-CiManifestPath))

    $manifest = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json -AsHashtable -Depth 20
    $errors = [System.Collections.Generic.List[string]]::new()
    Test-CiManifestSchema -Manifest $manifest -Errors $errors
    if ($errors.Count -gt 0) {
        throw "Invalid CI tool manifest '$Path':`n - $($errors -join "`n - ")"
    }
    $manifest
}

function Get-LlvmEntry {
    param([Parameter(Mandatory)]$Manifest, [Parameter(Mandatory)][string]$Version)
    $entry = @($Manifest['llvm'] | Where-Object { $_['version'] -eq $Version })
    if ($entry.Count -ne 1) {
        throw "LLVM $Version is not pinned in .github/ci-tools/manifest.json. Add it with .github/ci-tools/Update-Manifest.ps1."
    }
    $entry[0]
}

function Get-KitsEntry {
    param([Parameter(Mandatory)]$Manifest, [Parameter(Mandatory)][string]$Version)
    $entry = @($Manifest['kits'] | Where-Object { $_['version'] -eq $Version })
    if ($entry.Count -ne 1) {
        throw "WDK $Version is not pinned in .github/ci-tools/manifest.json. Add it with .github/ci-tools/Update-Manifest.ps1."
    }
    $entry[0]
}

function ConvertTo-ManifestArch {
    param([Parameter(Mandatory)][ValidateSet('amd64', 'arm64', 'x64')][string]$Arch)
    if ($Arch -eq 'arm64') { 'arm64' } else { 'x64' }
}

function Get-TextSha256 {
    param([Parameter(Mandatory)][string]$Text)
    [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($Text)))
}

# The cache key embeds a digest of every pinned field, so editing any pin
# yields a new key rather than silently reusing a stale entry.
function Get-LlvmPayload {
    param([Parameter(Mandatory)]$Manifest, [Parameter(Mandatory)][string]$Version, [Parameter(Mandatory)][string]$Arch)
    $entry = Get-LlvmEntry -Manifest $Manifest -Version $Version
    $arch = ConvertTo-ManifestArch $Arch
    $installer = $entry['installers'][$arch]
    $id = (Get-TextSha256 "llvm|$($entry['version'])|$($entry['wingetId'])|$arch|$($installer['sha256'])|$($installer['bytes'])").Substring(0, 16)
    [pscustomobject]@{
        Kind  = 'llvm'
        Name  = "llvm-$Version-$arch"
        Key   = "ci-tools-g$($Manifest['generation'])-llvm-$Version-$arch-$id"
        Path  = ".ci-tools/llvm/$Version-$arch"
        Entry = $entry
        Arch  = $arch
    }
}

function Get-KitsPayload {
    param([Parameter(Mandatory)]$Manifest, [Parameter(Mandatory)][string]$Version)
    $entry = Get-KitsEntry -Manifest $Manifest -Version $Version
    if ($entry['source'] -eq 'layout') {
        $parts = foreach ($c in 'sdk', 'wdk') {
            $comp = $entry['components'][$c]
            "$c|$($comp['wingetId'])|$($comp['wingetVersion'])|$($comp['bootstrapperSha256'])|$($comp['layoutTreeSha256'])"
        }
        $subject = "kits|$Version|layout|$($parts -join '|')"
    } else {
        $pkgs = @($entry['packages'] | ForEach-Object { "$($_['id'])@$($_['version'])=$($_['sha256']):$($_['bytes'])" } | Sort-Object)
        $subject = "kits|$Version|nuget|$($entry['nugetVersion'])|$(@($entry['roots'] | Sort-Object) -join ',')|$($pkgs -join ',')|$($entry['installedTreeSha256'])"
    }
    $id = (Get-TextSha256 $subject).Substring(0, 16)
    $kind = if ($entry['source'] -eq 'layout') { 'kits-layout' } else { 'kits-nuget' }
    [pscustomobject]@{
        Kind  = $kind
        Name  = "kits-$Version-$($entry['source'])"
        Key   = "ci-tools-g$($Manifest['generation'])-kits-$Version-$($entry['source'])-$id"
        Path  = ".ci-tools/kits/$Version-$($entry['source'])"
        Entry = $entry
        Arch  = $null
    }
}

function Get-AllPayloads {
    param([Parameter(Mandatory)]$Manifest)
    foreach ($e in $Manifest['llvm']) {
        foreach ($arch in $script:Architectures) {
            Get-LlvmPayload -Manifest $Manifest -Version $e['version'] -Arch $arch
        }
    }
    foreach ($e in $Manifest['kits']) {
        Get-KitsPayload -Manifest $Manifest -Version $e['version']
    }
}

# Every runner, LLVM, WDK and target combination a consumer can ask for. It is
# derived from the manifest so that pinning a new version also verifies it.
# The target only changes what a NuGet-sourced WDK installs, so layout-sourced
# kits get a single row per runner.
function Get-VerifyMatrix {
    param([Parameter(Mandatory)]$Manifest)
    $runners = @(
        @{ Name = 'windows-2025'; Arch = 'amd64' }
        @{ Name = 'windows-11-arm'; Arch = 'arm64' }
    )
    foreach ($r in $runners) {
        foreach ($llvm in $Manifest['llvm']) {
            foreach ($kits in $Manifest['kits']) {
                $targets = if ($kits['source'] -eq 'nuget') { @('amd64', 'arm64') } else { @($r.Arch) }
                foreach ($target in $targets) {
                    [pscustomobject]@{ runner = $r.Name; arch = $r.Arch; llvm = $llvm['version']; wdk = $kits['version']; target = $target }
                }
            }
        }
    }
}

function Get-TreeHash {
    param([Parameter(Mandatory)][string]$Path)

    $base = (Get-Item -LiteralPath $Path).FullName.TrimEnd('\') + '\'
    $files = @(Get-ChildItem -LiteralPath $base -Recurse -File -Force)
    $lines = $files | ForEach-Object -ThrottleLimit ([Environment]::ProcessorCount * 2) -Parallel {
        $b = $using:base
        $h = Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256
        '{0}|{1}|{2}' -f $_.FullName.Substring($b.Length).ToLowerInvariant(), $_.Length, $h.Hash
    }
    $sorted = [string[]]@($lines)
    [Array]::Sort($sorted, [StringComparer]::Ordinal)
    $total = [int64]0
    foreach ($f in $files) { $total += $f.Length }
    [pscustomobject]@{
        Files      = $sorted.Count
        Bytes      = $total
        TreeSha256 = Get-TextSha256 ($sorted -join "`n")
    }
}

function Assert-NoReparsePoints {
    param([Parameter(Mandatory)][string]$Path)
    $bad = @(Get-ChildItem -LiteralPath $Path -Recurse -Force | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint })
    if ($bad.Count -gt 0) {
        throw "Restored payload '$Path' contains reparse points: $(($bad | Select-Object -First 5 -ExpandProperty FullName) -join ', ')"
    }
}

function Assert-FileSha256 {
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][string]$Expected, [int64]$ExpectedBytes = -1)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "Missing payload file '$Path'." }
    $item = Get-Item -LiteralPath $Path
    if ($ExpectedBytes -ge 0 -and $item.Length -ne $ExpectedBytes) {
        throw "Size mismatch for '$Path': expected $ExpectedBytes bytes, found $($item.Length)."
    }
    $actual = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash
    if ($actual -ne $Expected) { throw "SHA256 mismatch for '$Path': expected $Expected, found $actual." }
}

function Assert-TreeSha256 {
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][string]$Expected)
    if (-not (Test-Path -LiteralPath $Path -PathType Container)) { throw "Missing payload directory '$Path'." }
    Assert-NoReparsePoints -Path $Path
    $actual = Get-TreeHash -Path $Path
    if ($actual.TreeSha256 -ne $Expected) {
        throw "Tree SHA256 mismatch for '$Path': expected $Expected, found $($actual.TreeSha256) ($($actual.Files) files, $($actual.Bytes) bytes)."
    }
}

function Assert-ExactFileSet {
    param([Parameter(Mandatory)][string]$Directory, [Parameter(Mandatory)][hashtable]$Expected)
    if (-not (Test-Path -LiteralPath $Directory -PathType Container)) { throw "Missing payload directory '$Directory'." }
    Assert-NoReparsePoints -Path $Directory
    $entries = @(Get-ChildItem -LiteralPath $Directory -Recurse -Force)
    $dirs = @($entries | Where-Object PSIsContainer)
    if ($dirs.Count -gt 0) { throw "Unexpected directories in '$Directory': $(($dirs | ForEach-Object Name) -join ', ')" }
    $actualNames = @($entries | ForEach-Object { $_.Name.ToLowerInvariant() } | Sort-Object)
    $expectedNames = @($Expected.Keys | ForEach-Object { $_.ToLowerInvariant() } | Sort-Object)
    if (($actualNames -join '|') -ne ($expectedNames -join '|')) {
        throw "Unexpected contents in '$Directory'. Expected: $($expectedNames -join ', '). Found: $($actualNames -join ', ')."
    }
}

function Set-CiEnv {
    param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][string]$Value)
    [Environment]::SetEnvironmentVariable($Name, $Value, 'Process')
    if ($env:GITHUB_ENV) { Add-Content -LiteralPath $env:GITHUB_ENV -Value "$Name=$Value" }
    Write-Host "Set $Name=$Value"
}

function Add-CiPath {
    param([Parameter(Mandatory)][string]$Directory)
    $env:PATH = "$Directory;$env:PATH"
    if ($env:GITHUB_PATH) { Add-Content -LiteralPath $env:GITHUB_PATH -Value $Directory }
    Write-Host "Prepended $Directory to PATH"
}

function Invoke-Installer {
    param(
        [Parameter(Mandatory)][string]$File,
        [Parameter(Mandatory)][string[]]$Arguments,
        [int[]]$OkExitCodes = @(0, 3010),
        [int]$TimeoutMinutes = 20
    )
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $p = Start-Process -FilePath $File -ArgumentList $Arguments -PassThru
    $null = $p.Handle
    if (-not $p.WaitForExit($TimeoutMinutes * 60000)) {
        Stop-Process -Id $p.Id -Force
        throw "Installer '$File $($Arguments -join ' ')' did not finish within $TimeoutMinutes minutes."
    }
    $sw.Stop()
    Write-Host ("{0} {1} exited {2} in {3:n1}s" -f (Split-Path $File -Leaf), ($Arguments -join ' '), $p.ExitCode, $sw.Elapsed.TotalSeconds)
    if ($p.ExitCode -notin $OkExitCodes) {
        throw "Installer '$File $($Arguments -join ' ')' failed with exit code $($p.ExitCode)."
    }
}

function Assert-PayloadVerified {
    [CmdletBinding()]
    param([Parameter(Mandatory)]$Payload, [Parameter(Mandatory)][string]$PayloadDir)

    $entry = $Payload.Entry
    switch ($Payload.Kind) {
        'llvm' {
            $info = $entry['installers'][$Payload.Arch]
            Assert-ExactFileSet -Directory $PayloadDir -Expected @{ 'installer.exe' = $info['sha256'] }
            Assert-FileSha256 -Path (Join-Path $PayloadDir 'installer.exe') -Expected $info['sha256'] -ExpectedBytes $info['bytes']
        }
        'kits-layout' {
            if (-not (Test-Path -LiteralPath $PayloadDir -PathType Container)) { throw "Missing payload directory '$PayloadDir'." }
            $top = @(Get-ChildItem -LiteralPath $PayloadDir -Force | ForEach-Object Name | Sort-Object)
            if (($top -join '|') -ne 'sdk|wdk') {
                throw "Unexpected contents in '$PayloadDir': $($top -join ', '). Expected only sdk and wdk."
            }
            foreach ($c in 'sdk', 'wdk') {
                Assert-TreeSha256 -Path (Join-Path $PayloadDir $c) -Expected $entry['components'][$c]['layoutTreeSha256']
            }
        }
        'kits-nuget' {
            $expected = @{}
            foreach ($p in $entry['packages']) { $expected["$($p['id']).$($p['version']).nupkg"] = $p }
            Assert-ExactFileSet -Directory $PayloadDir -Expected $expected
            foreach ($p in $entry['packages']) {
                Assert-FileSha256 -Path (Join-Path $PayloadDir "$($p['id']).$($p['version']).nupkg") -Expected $p['sha256'] -ExpectedBytes $p['bytes']
            }
        }
        default { throw "Unknown payload kind '$($Payload.Kind)'." }
    }
}

function Install-Llvm {
    [CmdletBinding()]
    param([Parameter(Mandatory)]$Payload, [Parameter(Mandatory)][string]$PayloadDir)

    $version = $Payload.Entry['version']

    Assert-PayloadVerified -Payload $Payload -PayloadDir $PayloadDir
    $installer = Join-Path $PayloadDir 'installer.exe'

    # The images ship an LLVM that may or may not be registered in Add/Remove
    # Programs, and overlaying onto it yields a mixed tree. Always start clean.
    $uninstaller = Join-Path $script:LlvmDir 'Uninstall.exe'
    if (Test-Path -LiteralPath $uninstaller) {
        Invoke-Installer -File $uninstaller -Arguments @('/S', "_?=$script:LlvmDir")
    }
    if (Test-Path -LiteralPath $script:LlvmDir) {
        Remove-Item -LiteralPath $script:LlvmDir -Recurse -Force
    }
    Invoke-Installer -File $installer -Arguments @('/S')

    $bin = Join-Path $script:LlvmDir 'bin'
    Add-CiPath $bin
    Set-CiEnv -Name 'LIBCLANG_PATH' -Value $bin
    Assert-LlvmReady -Version $version
}

function Assert-LlvmReady {
    param([Parameter(Mandatory)][string]$Version)
    $bin = Join-Path $script:LlvmDir 'bin'
    $resolved = (Get-Command clang -ErrorAction Stop).Source
    if ($resolved -ne (Join-Path $bin 'clang.exe')) { throw "clang resolves to '$resolved', expected '$bin\clang.exe'." }
    $clang = (& clang --version 2>&1 | Select-Object -First 1) -as [string]
    if ($clang -notmatch "^clang version $([regex]::Escape($Version))(\s|$)") { throw "Unexpected clang: '$clang', expected version $Version." }
    $libclang = (Get-Item -LiteralPath (Join-Path $bin 'libclang.dll')).VersionInfo.FileVersion
    if ($libclang -ne $Version) { throw "libclang.dll is $libclang, expected $Version." }
    Write-Host "LLVM ready: $clang; libclang $libclang"
}

function Get-ArpEntries {
    $paths = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*',
             'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*'
    foreach ($path in $paths) {
        foreach ($item in @(Get-ItemProperty -Path $path -ErrorAction SilentlyContinue)) {
            $props = $item.PSObject.Properties
            if ($props['DisplayName']) {
                [pscustomobject]@{
                    Name                 = $item.DisplayName
                    Version              = if ($props['DisplayVersion']) { $item.DisplayVersion } else { $null }
                    Key                  = $item.PSChildName
                    UninstallString      = if ($props['UninstallString']) { $item.UninstallString } else { $null }
                    QuietUninstallString = if ($props['QuietUninstallString']) { $item.QuietUninstallString } else { $null }
                }
            }
        }
    }
}

function Get-KitBundles {
    param([Parameter(Mandatory)][ValidateSet('sdk', 'wdk')][string]$Kind)
    $pattern = if ($Kind -eq 'sdk') { '^Windows Software Development Kit - Windows 10\.' } else { '^Windows Driver Kit - Windows 10\.' }
    @(Get-ArpEntries | Where-Object { $_.Name -match $pattern } | Sort-Object Version, Key)
}

function Split-CommandLine {
    param([Parameter(Mandatory)][string]$CommandLine)
    if ($CommandLine -match '^\s*"([^"]+)"\s*(.*)$') { return @($Matches[1], $Matches[2]) }
    if ($CommandLine -match '^\s*(\S+)\s*(.*)$') { return @($Matches[1], $Matches[2]) }
    throw "Cannot parse command line: $CommandLine"
}

# Runs each bundle's own registered uninstaller so no WinGet call is needed.
# Kits shipped on the image would otherwise win version selection in wdk-build.
function Uninstall-KitsFromRegistry {
    foreach ($kind in 'sdk', 'wdk') {
        foreach ($bundle in @(Get-KitBundles -Kind $kind)) {
            $cmd = if ($bundle.QuietUninstallString) { $bundle.QuietUninstallString } else { $bundle.UninstallString }
            if (-not $cmd) { throw "No uninstall command for '$($bundle.Name)' ($($bundle.Key))." }
            $exe, $argString = Split-CommandLine $cmd
            if ($exe -match '(?i)msiexec') { throw "Unexpected MSI uninstall for '$($bundle.Name)': $cmd" }
            if ($argString -notmatch '(?i)/(quiet|q)\b') { $argString = "$argString /quiet".Trim() }
            if (-not (Test-Path -LiteralPath $exe)) { throw "Uninstaller missing for '$($bundle.Name)': $exe" }
            Write-Host "Uninstalling $($bundle.Name) ($($bundle.Version))"
            Invoke-Installer -File $exe -Arguments @($argString) -OkExitCodes @(0, 3010, 1641)
        }
    }
    $remaining = @(Get-KitBundles -Kind sdk) + @(Get-KitBundles -Kind wdk)
    if ($remaining.Count -gt 0) { throw "Kit bundles remain after uninstall: $(($remaining | ForEach-Object Name) -join '; ')" }
}

function Install-KitsFromLayout {
    [CmdletBinding()]
    param([Parameter(Mandatory)]$Payload, [Parameter(Mandatory)][string]$PayloadDir)

    $version = $Payload.Entry['version']
    Assert-PayloadVerified -Payload $Payload -PayloadDir $PayloadDir
    Uninstall-KitsFromRegistry
    Invoke-Installer -File (Join-Path $PayloadDir 'sdk\winsdksetup.exe') -Arguments @('/q')
    Invoke-Installer -File (Join-Path $PayloadDir 'wdk\wdksetup.exe') -Arguments @('/q')

    Set-CiEnv -Name 'SDK_VERSION' -Value $version
    Set-CiEnv -Name 'Version_Number' -Value "$version.0"
    Assert-KitsInstalled -Version $version
}

function Assert-KitsInstalled {
    param([Parameter(Mandatory)][string]$Version)
    $kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10'
    foreach ($rel in "Include\$Version.0\km\wdm.h", "Lib\$Version.0\km\x64\ntoskrnl.lib") {
        if (-not (Test-Path -LiteralPath (Join-Path $kits $rel))) { throw "Expected '$rel' under '$kits' after installing WDK $Version." }
    }
}

function Install-NugetClosure {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]$Entry,
        [Parameter(Mandatory)][string]$FeedDir,
        [Parameter(Mandatory)][string]$PackagesRoot
    )

    if (Test-Path -LiteralPath $PackagesRoot) { Remove-Item -LiteralPath $PackagesRoot -Recurse -Force }

    # An empty config with <clear /> plus an explicit -Source keeps nuget from
    # consulting nuget.org or any machine-level feed.
    $config = Join-Path ($env:RUNNER_TEMP ?? $env:TEMP) 'ci-tools-nuget.config'
    Set-Content -LiteralPath $config -Encoding utf8 -Value '<?xml version="1.0" encoding="utf-8"?><configuration><packageSources><clear /></packageSources></configuration>'

    foreach ($root in $Entry['roots']) {
        Write-Host "Installing $root $($Entry['nugetVersion']) from the local feed"
        & nuget install $root -Version $Entry['nugetVersion'] -OutputDirectory $PackagesRoot -Source $FeedDir -ConfigFile $config -NoCache -DirectDownload -NonInteractive -Verbosity quiet
        if ($LASTEXITCODE -ne 0) { throw "nuget install $root $($Entry['nugetVersion']) failed with exit code $LASTEXITCODE." }
    }

    $installed = @(Get-ChildItem -LiteralPath $PackagesRoot -Directory | ForEach-Object { $_.Name.ToLowerInvariant() } | Sort-Object)
    $wanted = @($Entry['packages'] | ForEach-Object { "$($_['id']).$($_['version'])".ToLowerInvariant() } | Sort-Object)
    if (($installed -join '|') -ne ($wanted -join '|')) {
        throw "Installed packages differ from the manifest. Expected: $($wanted -join ', '). Installed: $($installed -join ', ')."
    }
}

function Install-KitsFromNugetFeed {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]$Payload,
        [Parameter(Mandatory)][string]$PayloadDir,
        [Parameter(Mandatory)][ValidateSet('amd64', 'arm64')][string]$HostArch,
        [Parameter(Mandatory)][ValidateSet('amd64', 'arm64')][string]$TargetArch,
        [string]$PackagesRoot = $script:NugetPackagesRoot,
        [switch]$VerifyInstalledTree
    )

    $entry = $Payload.Entry
    $sdkVersion = $entry['version']

    Assert-PayloadVerified -Payload $Payload -PayloadDir $PayloadDir
    Install-NugetClosure -Entry $entry -FeedDir $PayloadDir -PackagesRoot $PackagesRoot
    if ($VerifyInstalledTree) {
        Assert-TreeSha256 -Path $PackagesRoot -Expected $entry['installedTreeSha256']
    }

    $archToId = @{ amd64 = 'x64'; arm64 = 'ARM64' }
    $sdkPackage = @($entry['packages'] | Where-Object { $_['id'] -eq 'Microsoft.Windows.SDK.CPP' })[0]
    $hostPackage = @($entry['packages'] | Where-Object { $_['id'] -eq "Microsoft.Windows.WDK.$($archToId[$HostArch])" })[0]
    $targetPackage = @($entry['packages'] | Where-Object { $_['id'] -eq "Microsoft.Windows.WDK.$($archToId[$TargetArch])" })[0]
    foreach ($p in @($sdkPackage, $hostPackage, $targetPackage)) {
        if (-not $p) { throw "The manifest for WDK $sdkVersion lacks a package for host '$HostArch' or target '$TargetArch'." }
    }
    $sdkDir = Join-Path $PackagesRoot "$($sdkPackage['id']).$($sdkPackage['version'])"
    $hostDir = Join-Path $PackagesRoot "$($hostPackage['id']).$($hostPackage['version'])"
    $targetDir = Join-Path $PackagesRoot "$($targetPackage['id']).$($targetPackage['version'])"

    $envVars = [ordered]@{
        Version_Number    = "$sdkVersion.0"
        WindowsSdkBinPath = "$sdkDir\c\bin"
        WDKContentRoot    = "$targetDir\c\"
        WDKBinRoot        = "$hostDir\c\bin\$sdkVersion.0"
        WDKToolRoot       = "$hostDir\c\tools\$sdkVersion.0"
        NugetPackagesRoot = $PackagesRoot
        FullVersionNumber = $entry['nugetVersion']
    }
    foreach ($name in 'WindowsSdkBinPath', 'WDKContentRoot', 'WDKBinRoot', 'WDKToolRoot') {
        if (-not (Test-Path -LiteralPath $envVars[$name])) { throw "Critical path not found: $name = $($envVars[$name])" }
    }
    foreach ($name in 'wdm.h') {
        $header = Join-Path $targetDir "c\include\$sdkVersion.0\km\$name"
        if (-not (Test-Path -LiteralPath $header)) { throw "Expected '$header' after installing the NuGet closure." }
    }
    foreach ($kv in $envVars.GetEnumerator()) { Set-CiEnv -Name $kv.Key -Value $kv.Value }
}

function Write-CacheMissAdvice {
    param([Parameter(Mandatory)][object[]]$Payload)
    $repo = $env:GITHUB_REPOSITORY
    $server = $env:GITHUB_SERVER_URL ?? 'https://github.com'
    $runs = "$server/$repo/actions/workflows/cache-update.yaml"
    $lines = @($Payload | ForEach-Object { "The CI tool cache has no entry for '$($_.Name)' (key $($_.Key))." })
    $lines += @(
        'CI jobs never download tools themselves, so this job cannot continue.',
        "Cache Guard asks for a Cache Update run on the default branch when it is permitted to. Check $runs.",
        'When that run succeeds, re-run this job (or re-queue the pull request). Maintainers can also start Cache Update manually.'
    )
    $lines | ForEach-Object { Write-Host "::error title=CI tool cache miss::$_" }
}

function Install-Toolchain {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$LlvmVersion,
        [Parameter(Mandatory)][string]$KitsVersion,
        [Parameter(Mandatory)][ValidateSet('amd64', 'arm64')][string]$HostArch,
        [Parameter(Mandatory)][ValidateSet('amd64', 'arm64')][string]$TargetArch,
        [string]$Workspace = $env:GITHUB_WORKSPACE,
        [bool]$LlvmRestored = $true,
        [bool]$KitsRestored = $true
    )

    $manifest = Get-CiManifest
    $llvm = Get-LlvmPayload -Manifest $manifest -Version $LlvmVersion -Arch $HostArch
    $kits = Get-KitsPayload -Manifest $manifest -Version $KitsVersion

    $missing = @(@(@($llvm, $LlvmRestored), @($kits, $KitsRestored)) | Where-Object { -not $_[1] } | ForEach-Object { $_[0] })
    if ($missing.Count -gt 0) {
        Write-CacheMissAdvice -Payload $missing
        throw "CI tool cache miss for: $(($missing | ForEach-Object Name) -join ', ')."
    }

    Write-Host "::group::Install LLVM $LlvmVersion"
    Install-Llvm -Payload $llvm -PayloadDir (Join-Path $Workspace $llvm.Path)
    Write-Host '::endgroup::'

    Write-Host "::group::Install WDK $KitsVersion ($($kits.Entry['source']))"
    $kitsDir = Join-Path $Workspace $kits.Path
    if ($kits.Kind -eq 'kits-layout') {
        Install-KitsFromLayout -Payload $kits -PayloadDir $kitsDir
    } else {
        Install-KitsFromNugetFeed -Payload $kits -PayloadDir $kitsDir -HostArch $HostArch -TargetArch $TargetArch
    }
    Write-Host '::endgroup::'
}

Export-ModuleMember -Function @(
    'Get-CiManifest', 'Get-CiManifestPath', 'Get-LlvmEntry', 'Get-KitsEntry',
    'Get-LlvmPayload', 'Get-KitsPayload', 'Get-AllPayloads', 'Get-VerifyMatrix', 'ConvertTo-ManifestArch',
    'Get-TextSha256', 'Get-TreeHash',
    'Assert-FileSha256', 'Assert-TreeSha256', 'Assert-ExactFileSet', 'Assert-NoReparsePoints', 'Assert-PayloadVerified',
    'Set-CiEnv', 'Add-CiPath', 'Invoke-Installer',
    'Install-Llvm', 'Assert-LlvmReady',
    'Get-ArpEntries', 'Get-KitBundles', 'Split-CommandLine', 'Uninstall-KitsFromRegistry',
    'Install-KitsFromLayout', 'Install-NugetClosure', 'Install-KitsFromNugetFeed', 'Assert-KitsInstalled',
    'Write-CacheMissAdvice', 'Install-Toolchain'
)
