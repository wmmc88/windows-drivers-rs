#Requires -Modules @{ ModuleName = 'Pester'; ModuleVersion = '5.0.0' }

BeforeAll {
    Import-Module (Join-Path $PSScriptRoot 'CiTools.psm1') -Force -DisableNameChecking

    function script:New-TestManifest {
        Get-Content -LiteralPath (Get-CiManifestPath) -Raw | ConvertFrom-Json -AsHashtable -Depth 20
    }

    function script:Test-Schema($Manifest) {
        $errors = [System.Collections.Generic.List[string]]::new()
        InModuleScope CiTools -Parameters @{ Manifest = $Manifest; Errors = $errors } {
            Test-CiManifestSchema -Manifest $Manifest -Errors $Errors
        }
        , $errors
    }
}

Describe 'manifest schema' {
    It 'accepts the checked-in manifest' {
        { Get-CiManifest } | Should -Not -Throw
    }

    It 'rejects a malformed sha256' {
        $m = New-TestManifest
        $m['llvm'][0]['installers']['x64']['sha256'] = 'abc'
        (Test-Schema $m) | Should -Not -BeNullOrEmpty
    }

    It 'rejects lowercase digests' {
        $m = New-TestManifest
        $m['llvm'][0]['installers']['x64']['sha256'] = $m['llvm'][0]['installers']['x64']['sha256'].ToLowerInvariant()
        (Test-Schema $m) | Should -Not -BeNullOrEmpty
    }

    It 'rejects unknown keys so typos cannot silently skip a check' {
        $m = New-TestManifest
        $m['llvm'][0]['installers']['x64']['sha265'] = 'x'
        (Test-Schema $m) | Should -Not -BeNullOrEmpty
    }

    It 'rejects values that could inject into a command line' {
        $m = New-TestManifest
        $m['llvm'][0]['version'] = '17.0.6"; calc; "'
        (Test-Schema $m) | Should -Not -BeNullOrEmpty
    }

    It 'rejects a nuget root that is not a listed package' {
        $m = New-TestManifest
        $kit = $m['kits'] | Where-Object { $_['source'] -eq 'nuget' }
        $kit['roots'] = @('Microsoft.Windows.WDK.Missing')
        (Test-Schema $m) | Should -Not -BeNullOrEmpty
    }

    It 'rejects a nuget version that does not extend the kit version' {
        $m = New-TestManifest
        $kit = $m['kits'] | Where-Object { $_['source'] -eq 'nuget' }
        $kit['nugetVersion'] = '10.0.22621.1'
        (Test-Schema $m) | Should -Not -BeNullOrEmpty
    }

    It 'rejects duplicate entries' {
        $m = New-TestManifest
        $m['llvm'] = @($m['llvm'][0], $m['llvm'][0])
        (Test-Schema $m) | Should -Not -BeNullOrEmpty
    }
}

Describe 'cache keys' {
    BeforeAll { $script:Manifest = New-TestManifest }

    It 'are stable for an unchanged manifest' {
        (Get-LlvmPayload -Manifest $Manifest -Version '17.0.6' -Arch amd64).Key |
            Should -Be (Get-LlvmPayload -Manifest $Manifest -Version '17.0.6' -Arch amd64).Key
    }

    It 'map amd64 to x64 and keep per-architecture keys distinct' {
        $x64 = Get-LlvmPayload -Manifest $Manifest -Version '17.0.6' -Arch amd64
        $arm = Get-LlvmPayload -Manifest $Manifest -Version '17.0.6' -Arch arm64
        $x64.Key | Should -Match '-x64-'
        $x64.Key | Should -Not -Be $arm.Key
    }

    It 'change when any installer digest changes' {
        $before = (Get-LlvmPayload -Manifest $Manifest -Version '17.0.6' -Arch amd64).Key
        $changed = New-TestManifest
        $changed['llvm'][0]['installers']['x64']['sha256'] = ('0' * 64)
        (Get-LlvmPayload -Manifest $changed -Version '17.0.6' -Arch amd64).Key | Should -Not -Be $before
    }

    It 'change when a layout tree digest changes' {
        $before = (Get-KitsPayload -Manifest $Manifest -Version '10.0.22621').Key
        $changed = New-TestManifest
        $kit = $changed['kits'] | Where-Object { $_['version'] -eq '10.0.22621' }
        $kit['components']['wdk']['layoutTreeSha256'] = ('0' * 64)
        (Get-KitsPayload -Manifest $changed -Version '10.0.22621').Key | Should -Not -Be $before
    }

    It 'change when a nupkg digest changes' {
        $before = (Get-KitsPayload -Manifest $Manifest -Version '10.0.26100').Key
        $changed = New-TestManifest
        $kit = $changed['kits'] | Where-Object { $_['version'] -eq '10.0.26100' }
        $kit['packages'][0]['sha256'] = ('0' * 64)
        (Get-KitsPayload -Manifest $changed -Version '10.0.26100').Key | Should -Not -Be $before
    }

    It 'change when the generation is bumped' {
        $before = (Get-KitsPayload -Manifest $Manifest -Version '10.0.26100').Key
        $changed = New-TestManifest
        $changed['generation'] = 2
        (Get-KitsPayload -Manifest $changed -Version '10.0.26100').Key | Should -Not -Be $before
    }

    It 'fail with a pointer to the manifest for an unpinned version' {
        { Get-LlvmPayload -Manifest $Manifest -Version '1.2.3' -Arch amd64 } | Should -Throw '*manifest.json*'
        { Get-KitsPayload -Manifest $Manifest -Version '10.0.99999' } | Should -Throw '*manifest.json*'
    }

    It 'use unique keys and paths across all payloads' {
        $all = @(Get-AllPayloads -Manifest $Manifest)
        ($all.Key | Sort-Object -Unique).Count | Should -Be $all.Count
        ($all.Path | Sort-Object -Unique).Count | Should -Be $all.Count
    }
}

Describe 'payload verification' {
    BeforeEach {
        $script:Dir = Join-Path $TestDrive ([guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path "$Dir\sub" | Out-Null
        Set-Content -LiteralPath "$Dir\a.txt" -Value 'alpha'
        Set-Content -LiteralPath "$Dir\sub\b.txt" -Value 'beta'
    }

    It 'hashes a tree deterministically' {
        (Get-TreeHash -Path $Dir).TreeSha256 | Should -Be (Get-TreeHash -Path $Dir).TreeSha256
    }

    It 'detects a modified file' {
        $before = (Get-TreeHash -Path $Dir).TreeSha256
        Set-Content -LiteralPath "$Dir\sub\b.txt" -Value 'tampered'
        (Get-TreeHash -Path $Dir).TreeSha256 | Should -Not -Be $before
    }

    It 'detects an added file' {
        $before = (Get-TreeHash -Path $Dir).TreeSha256
        Set-Content -LiteralPath "$Dir\extra.txt" -Value 'x'
        (Get-TreeHash -Path $Dir).TreeSha256 | Should -Not -Be $before
    }

    It 'detects a renamed file' {
        $before = (Get-TreeHash -Path $Dir).TreeSha256
        Rename-Item -LiteralPath "$Dir\a.txt" -NewName 'c.txt'
        (Get-TreeHash -Path $Dir).TreeSha256 | Should -Not -Be $before
    }

    It 'asserts a matching tree and rejects a mismatch' {
        $good = (Get-TreeHash -Path $Dir).TreeSha256
        { Assert-TreeSha256 -Path $Dir -Expected $good } | Should -Not -Throw
        { Assert-TreeSha256 -Path $Dir -Expected ('0' * 64) } | Should -Throw '*mismatch*'
    }

    It 'rejects junctions in a restored payload' {
        $target = Join-Path $TestDrive 'elsewhere'
        New-Item -ItemType Directory -Path $target | Out-Null
        New-Item -ItemType Junction -Path "$Dir\link" -Target $target | Out-Null
        { Assert-NoReparsePoints -Path $Dir } | Should -Throw '*reparse*'
    }

    It 'verifies file size and digest' {
        $hash = (Get-FileHash -LiteralPath "$Dir\a.txt" -Algorithm SHA256).Hash
        $size = (Get-Item -LiteralPath "$Dir\a.txt").Length
        { Assert-FileSha256 -Path "$Dir\a.txt" -Expected $hash -ExpectedBytes $size } | Should -Not -Throw
        { Assert-FileSha256 -Path "$Dir\a.txt" -Expected $hash -ExpectedBytes ($size + 1) } | Should -Throw '*Size mismatch*'
        { Assert-FileSha256 -Path "$Dir\a.txt" -Expected ('0' * 64) } | Should -Throw '*SHA256 mismatch*'
        { Assert-FileSha256 -Path "$Dir\missing.txt" -Expected $hash } | Should -Throw '*Missing*'
    }

    It 'requires the exact set of files and no subdirectories' {
        $flat = Join-Path $TestDrive 'flat'
        New-Item -ItemType Directory -Path $flat | Out-Null
        Set-Content -LiteralPath "$flat\One.nupkg" -Value '1'
        { Assert-ExactFileSet -Directory $flat -Expected @{ 'one.nupkg' = 1 } } | Should -Not -Throw
        { Assert-ExactFileSet -Directory $flat -Expected @{ 'one.nupkg' = 1; 'two.nupkg' = 2 } } | Should -Throw '*Unexpected contents*'
        Set-Content -LiteralPath "$flat\extra.nupkg" -Value '2'
        { Assert-ExactFileSet -Directory $flat -Expected @{ 'one.nupkg' = 1 } } | Should -Throw '*Unexpected contents*'
        Remove-Item -LiteralPath "$flat\extra.nupkg"
        New-Item -ItemType Directory -Path "$flat\nested" | Out-Null
        { Assert-ExactFileSet -Directory $flat -Expected @{ 'one.nupkg' = 1 } } | Should -Throw '*Unexpected directories*'
    }
}

Describe 'Split-CommandLine' {
    It 'splits a quoted executable' {
        $exe, $rest = Split-CommandLine '"C:\ProgramData\Package Cache\{id}\winsdksetup.exe" /uninstall /quiet'
        $exe | Should -Be 'C:\ProgramData\Package Cache\{id}\winsdksetup.exe'
        $rest | Should -Be '/uninstall /quiet'
    }

    It 'splits an unquoted executable' {
        $exe, $rest = Split-CommandLine 'C:\x\setup.exe /S'
        $exe | Should -Be 'C:\x\setup.exe'
        $rest | Should -Be '/S'
    }
}
