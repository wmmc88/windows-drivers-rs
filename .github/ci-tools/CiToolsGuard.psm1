<#
.SYNOPSIS
Blocks outbound network traffic from everything except the Actions runner, so a verification job can prove that installing from the cache needs no network.

.DESCRIPTION
Used only by the verification jobs in Cache Update. It sets the Windows Firewall
default outbound action to Block and allows the runner's own binaries (which
log streaming and the cache service need) plus the Node.js runtime the runner
bundles for JavaScript actions such as actions/cache. pwsh, nuget, winget and
every installer stay blocked, so any attempt by them to reach WinGet, the
PowerShell Gallery or nuget.org fails the job.
#>

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:RulePrefix = 'ci-tools-allow'
$script:FirewallLog = Join-Path $env:SystemRoot 'System32\LogFiles\Firewall\ci-tools.log'

function Get-RunnerProgramPath {
    [CmdletBinding()]
    param()

    $worker = @(Get-Process -Name 'Runner.Worker' -ErrorAction SilentlyContinue | Where-Object { $_.Path }) | Select-Object -First 1
    if (-not $worker) { throw 'Runner.Worker is not running; refusing to block outbound traffic.' }

    # The runner keeps its binaries and its bundled Node.js in sibling
    # directories whose names may carry a version suffix.
    $binDir = Split-Path -Parent $worker.Path
    $root = Split-Path -Parent $binDir
    $paths = [Collections.Generic.List[string]]::new()
    Get-ChildItem -LiteralPath $binDir -Filter 'Runner.*.exe' -File | ForEach-Object { $paths.Add($_.FullName) }
    foreach ($dir in Get-ChildItem -LiteralPath $root -Directory -Filter 'externals*') {
        Get-ChildItem -LiteralPath $dir.FullName -Filter 'node.exe' -File -Recurse -Depth 3 | ForEach-Object { $paths.Add($_.FullName) }
    }

    if (-not ($paths | Where-Object { $_ -like '*\node.exe' })) { throw "No bundled node.exe found under '$root'; actions/cache would be blocked." }
    $paths | Sort-Object -Unique
}

function Enable-OfflineGuard {
    [CmdletBinding()]
    param()

    $programs = @(Get-RunnerProgramPath)
    foreach ($p in $programs) {
        New-NetFirewallRule -DisplayName "$script:RulePrefix-$([IO.Path]::GetFileName($p))" -Direction Outbound -Action Allow -Program $p -Profile Any | Out-Null
    }
    # Azure's wire server and metadata endpoint keep the VM healthy; DNS is
    # needed by the allowed programs.
    New-NetFirewallRule -DisplayName "$script:RulePrefix-azure" -Direction Outbound -Action Allow -RemoteAddress '168.63.129.16', '169.254.169.254' -Profile Any | Out-Null
    New-NetFirewallRule -DisplayName "$script:RulePrefix-dns-udp" -Direction Outbound -Action Allow -Protocol UDP -RemotePort 53 -Profile Any | Out-Null
    New-NetFirewallRule -DisplayName "$script:RulePrefix-dns-tcp" -Direction Outbound -Action Allow -Protocol TCP -RemotePort 53 -Profile Any | Out-Null

    Remove-Item -LiteralPath $script:FirewallLog -Force -ErrorAction SilentlyContinue
    Set-NetFirewallProfile -Profile Domain, Private, Public -Enabled True -LogBlocked True -LogFileName $script:FirewallLog -LogMaxSizeKilobytes 32767 -DefaultOutboundAction Block
}

function Disable-OfflineGuard {
    [CmdletBinding()]
    param()

    Set-NetFirewallProfile -Profile Domain, Private, Public -DefaultOutboundAction Allow -LogBlocked False -ErrorAction SilentlyContinue
    Get-NetFirewallRule -DisplayName "$script:RulePrefix-*" -ErrorAction SilentlyContinue | Remove-NetFirewallRule
}

# pwsh is not an allowed program, so a request it cannot make proves the guard
# is in force for everything that is not the runner.
function Test-OfflineGuard {
    [CmdletBinding()]
    param()

    foreach ($url in 'https://www.powershellgallery.com', 'https://download.microsoft.com', 'https://api.nuget.org') {
        $blocked = $false
        try { Invoke-WebRequest -Uri $url -Method Head -TimeoutSec 5 -UseBasicParsing | Out-Null } catch { $blocked = $true }
        if (-not $blocked) { throw "The offline guard is not blocking $url." }
    }
}

# Windows Firewall log lines are space separated:
# date time action protocol src-ip dst-ip src-port dst-port size ...
function Get-BlockedSummary {
    [CmdletBinding()]
    param([string]$LogPath = $script:FirewallLog)

    if (-not (Test-Path -LiteralPath $LogPath)) { return @() }
    Get-Content -LiteralPath $LogPath |
        Where-Object { $_ -match '\sDROP\s' } |
        ForEach-Object {
            $f = $_ -split '\s+'
            if ($f.Count -ge 8) { [pscustomobject]@{ Proto = $f[3]; Dst = $f[5]; Port = $f[7] } }
        } |
        Where-Object { $_.Port -ne '53' -and $_.Dst -notmatch '^(127\.|::1|ff|224\.|239\.|255\.)' } |
        Group-Object Proto, Dst, Port | Sort-Object Count -Descending |
        ForEach-Object { [pscustomobject]@{ Flow = $_.Name; Count = $_.Count } }
}

Export-ModuleMember -Function @(
    'Get-RunnerProgramPath', 'Enable-OfflineGuard', 'Disable-OfflineGuard', 'Test-OfflineGuard', 'Get-BlockedSummary'
)
