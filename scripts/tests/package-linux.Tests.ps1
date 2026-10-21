# Hermetic packaging regressions: scratch files only, no build or installation.
# Run on Linux: pwsh ./scripts/tests/package-linux.Tests.ps1
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $IsLinux) { throw 'These packaging tests require Linux' }
$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$package = Join-Path $repoRoot 'scripts/package-linux.ps1'
$collect = Join-Path $repoRoot 'scripts/collect-release-artifacts.ps1'
$scratch = Join-Path ([System.IO.Path]::GetTempPath()) ("rimmerge package checks " + [guid]::NewGuid().ToString('N'))
$target = Join-Path $scratch 'target'
$mockBin = Join-Path $scratch 'bin'
$version = '9.8.7'
$realTar = (Get-Command tar).Source
$realChmod = (Get-Command chmod).Source
$sha256sum = (Get-Command sha256sum).Source
$savedPath = $env:PATH
$testVariables = @('RIMMERGE_TEST_TAR_MODE', 'RIMMERGE_TEST_TAR_COUNT', 'RIMMERGE_TEST_DEB', 'RIMMERGE_TEST_CHMOD_FAIL')
$savedVariables = @{}
foreach ($name in $testVariables) { $savedVariables[$name] = [Environment]::GetEnvironmentVariable($name) }
$passed = 0

function Assert-That([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}
function Invoke-Script([string]$Script, [string[]]$Arguments, [bool]$Success, [string]$ErrorContains) {
    $output = & pwsh -NoProfile -File $Script @Arguments 2>&1 | Out-String
    $code = $LASTEXITCODE
    Assert-That (($code -eq 0) -eq $Success) "unexpected exit $code`: $output"
    if ($ErrorContains) { Assert-That ($output.Contains($ErrorContains)) "wrong failure: $output" }
}
function Pass([string]$Name) {
    $script:passed++
    Write-Output "PASS: $Name"
}
function Save-Hashes([string[]]$Files) {
    $hashes = @{}
    foreach ($file in $Files) { $hashes[$file] = (Get-FileHash -LiteralPath $file).Hash }
    return $hashes
}
function Assert-Unchanged($Hashes) {
    foreach ($file in $Hashes.Keys) {
        Assert-That (Test-Path -LiteralPath $file -PathType Leaf) "input disappeared: $file"
        Assert-That ((Get-FileHash -LiteralPath $file).Hash -eq $Hashes[$file]) "input changed: $file"
    }
}
function Package-Names {
    @("Rimmerge-$version-linux-x64-portable.tar.gz", "rimmerge-cli-$version-x86_64-unknown-linux-gnu.tar.gz", "Rimmerge_${version}_amd64.deb")
}

try {
    New-Item -ItemType Directory -Force -Path $mockBin, (Join-Path $target 'bundle/deb') | Out-Null
    Set-Content (Join-Path $target 'rimmerge-desktop') 'synthetic desktop'
    Set-Content (Join-Path $target 'rimmerge') 'synthetic cli'
    $env:RIMMERGE_TEST_DEB = Join-Path $target "bundle/deb/Rimmerge_${version}_amd64.deb"
    Set-Content $env:RIMMERGE_TEST_DEB 'synthetic deb'
    $packageInputs = Save-Hashes @((Join-Path $target 'rimmerge-desktop'), (Join-Path $target 'rimmerge'), $env:RIMMERGE_TEST_DEB)
    $env:RIMMERGE_TEST_TAR_COUNT = Join-Path $scratch 'tar-count'
    $tarMock = @'
#!/bin/sh
n=$(cat "$RIMMERGE_TEST_TAR_COUNT")
n=$((n + 1))
printf '%s' "$n" > "$RIMMERGE_TEST_TAR_COUNT"
if { [ "$RIMMERGE_TEST_TAR_MODE" = first ] && [ "$n" = 1 ]; } || { [ "$RIMMERGE_TEST_TAR_MODE" = second ] && [ "$n" = 2 ]; }; then
  printf partial > "$2"
  exit 42
fi
if [ "$RIMMERGE_TEST_TAR_MODE" = silent ] && [ "$n" = 1 ]; then exit 0; fi
if [ "$RIMMERGE_TEST_TAR_MODE" = missing-deb ] && [ "$n" = 2 ]; then mv "$RIMMERGE_TEST_DEB" "$RIMMERGE_TEST_DEB.removed"; fi
exec __TAR__ "$@"
'@
    $chmodMock = @'
#!/bin/sh
if [ "$RIMMERGE_TEST_CHMOD_FAIL" = yes ]; then exit 43; fi
exec __CHMOD__ "$@"
'@
    Set-Content (Join-Path $mockBin 'tar') ($tarMock.Replace('__TAR__', $realTar)) -Encoding utf8NoBOM
    Set-Content (Join-Path $mockBin 'chmod') ($chmodMock.Replace('__CHMOD__', $realChmod)) -Encoding utf8NoBOM
    & $realChmod '+x' (Join-Path $mockBin 'tar') (Join-Path $mockBin 'chmod')
    Assert-That ($LASTEXITCODE -eq 0) 'mock setup chmod failed'
    $env:PATH = "$mockBin`:$savedPath"

    foreach ($mode in @('first', 'second', 'silent', 'missing-deb', 'chmod')) {
        Set-Content $env:RIMMERGE_TEST_TAR_COUNT '0'
        $env:RIMMERGE_TEST_TAR_MODE = $mode
        $env:RIMMERGE_TEST_CHMOD_FAIL = if ($mode -eq 'chmod') { 'yes' } else { 'no' }
        $out = Join-Path $scratch "failure-$mode"
        New-Item -ItemType Directory -Path $out | Out-Null
        Set-Content (Join-Path $out 'unrelated.txt') 'keep'
        Invoke-Script $package @('-Version', $version, '-TargetDir', $target, '-OutDir', $out) $false
        foreach ($name in (Package-Names)) { Assert-That (-not (Test-Path (Join-Path $out $name))) "partial output remains: $name" }
        Assert-That ((Get-Content (Join-Path $out 'unrelated.txt')) -eq 'keep') 'unrelated file removed'
        if (Test-Path "$($env:RIMMERGE_TEST_DEB).removed") { Move-Item "$($env:RIMMERGE_TEST_DEB).removed" $env:RIMMERGE_TEST_DEB }
        Assert-Unchanged $packageInputs
        Pass "packaging failure $mode rejects success and cleans partial outputs"
    }

    $env:RIMMERGE_TEST_CHMOD_FAIL = 'no'
    $env:RIMMERGE_TEST_TAR_MODE = 'success'
    Set-Content $env:RIMMERGE_TEST_TAR_COUNT '0'
    $out = Join-Path $scratch 'success'
    Invoke-Script $package @('-Version', $version, '-TargetDir', $target, '-OutDir', $out) $true
    Assert-That (@(Get-ChildItem $out -File).Count -eq 3) 'expected all three Linux packages'
    $extract = Join-Path $scratch 'extract'
    New-Item -ItemType Directory -Path $extract | Out-Null
    & $realTar '-xzf' (Join-Path $out "Rimmerge-$version-linux-x64-portable.tar.gz") '-C' $extract
    Assert-That ($LASTEXITCODE -eq 0) 'portable extraction failed'
    $portable = Join-Path $extract "Rimmerge-$version-linux-x64-portable"
    Assert-That ((Get-Content (Join-Path $portable 'Rimmerge')) -eq 'synthetic desktop') 'desktop payload changed'
    foreach ($name in @('Rimmerge', 'cli/rimmerge')) {
        Assert-That (([int][System.IO.File]::GetUnixFileMode((Join-Path $portable $name)) -band 0x49) -ne 0) "missing execute permission: $name"
    }
    foreach ($license in @('LICENSE-MIT', 'LICENSE-APACHE')) {
        Assert-That ((Get-FileHash (Join-Path $portable $license)).Hash -eq (Get-FileHash (Join-Path $repoRoot $license)).Hash) "license differs: $license"
    }
    Assert-That (-not ((Get-Content (Join-Path $portable 'README.txt') -Raw).Contains('\x27'))) 'README contains an escaped apostrophe'
    $cliExtract = Join-Path $scratch 'extract-cli'
    New-Item -ItemType Directory -Path $cliExtract | Out-Null
    & $realTar '-xzf' (Join-Path $out "rimmerge-cli-$version-x86_64-unknown-linux-gnu.tar.gz") '-C' $cliExtract
    Assert-That ($LASTEXITCODE -eq 0) 'CLI extraction failed'
    Assert-That (([int][System.IO.File]::GetUnixFileMode((Join-Path $cliExtract 'rimmerge')) -band 0x49) -ne 0) 'CLI tarball lost execute permission'
    foreach ($license in @('LICENSE-MIT', 'LICENSE-APACHE')) {
        Assert-That ((Get-FileHash (Join-Path $cliExtract $license)).Hash -eq (Get-FileHash (Join-Path $repoRoot $license)).Hash) "CLI license differs: $license"
    }
    Pass 'three Linux packages; tar contents, licenses and executable permissions'

    Invoke-Script $package @('-Version', $version, '-TargetDir', $target, '-OutDir', (Join-Path $target 'bundle/deb')) $false
    Assert-That (Test-Path $env:RIMMERGE_TEST_DEB -PathType Leaf) 'input alias deleted source deb'
    Assert-Unchanged $packageInputs
    Pass 'output alias cannot delete source packages'

    $debDir = Split-Path -Parent $env:RIMMERGE_TEST_DEB
    $debAlias = Join-Path $scratch 'deb-alias'
    New-Item -ItemType SymbolicLink -Path $debAlias -Target $debDir | Out-Null
    $targetAlias = Join-Path $scratch 'target-alias'
    New-Item -ItemType SymbolicLink -Path $targetAlias -Target $target | Out-Null
    foreach ($aliasOut in @($debAlias, (Join-Path $targetAlias 'bundle/deb'))) {
        Invoke-Script $package @('-Version', $version, '-TargetDir', $target, '-OutDir', $aliasOut) $false
        Assert-Unchanged $packageInputs
        Pass 'packaging rejects output symlink or linked parent without changing input hashes'
    }
    $brokenOut = Join-Path $scratch 'broken-output'
    New-Item -ItemType SymbolicLink -Path $brokenOut -Target (Join-Path $scratch 'absent') | Out-Null
    Invoke-Script $package @('-Version', $version, '-TargetDir', $target, '-OutDir', (Join-Path $brokenOut 'new')) $false
    Assert-Unchanged $packageInputs
    Pass 'packaging refuses broken output links before cleanup'


    $artifacts = Join-Path $scratch 'artifacts'
    $names = @(
        "zips-windows/Rimmerge-$version-windows-x64-portable.zip",
        "zips-windows/rimmerge-cli-$version-x86_64-pc-windows-msvc.zip"
    ) + @((Package-Names) | ForEach-Object { "zips-linux/$_" })
    foreach ($name in $names) {
        $file = Join-Path $artifacts $name
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $file) | Out-Null
        Set-Content $file "synthetic $name"
    }
    $collectorInputs = Save-Hashes @($names | ForEach-Object { Join-Path $artifacts $_ })
    $linuxInputs = Join-Path $artifacts 'zips-linux'
    $releaseAlias = Join-Path $scratch 'release-alias'
    New-Item -ItemType SymbolicLink -Path $releaseAlias -Target $linuxInputs | Out-Null
    $artifactsAlias = Join-Path $scratch 'artifacts-alias'
    New-Item -ItemType SymbolicLink -Path $artifactsAlias -Target $artifacts | Out-Null
    foreach ($aliasOut in @($linuxInputs, $releaseAlias, (Join-Path $artifactsAlias 'zips-linux'))) {
        Invoke-Script $collect @('-Version', $version, '-ArtifactDir', $artifacts, '-OutDir', $aliasOut) $false
        Assert-Unchanged $collectorInputs
        Pass 'collector rejects Linux input directory or aliases and preserves all five input hashes'
    }
    $crossAlias = Join-Path $scratch 'cross-alias'
    New-Item -ItemType Directory -Path $crossAlias | Out-Null
    New-Item -ItemType SymbolicLink -Path (Join-Path $crossAlias 'SHA256SUMS.txt') -Target (Join-Path $artifacts $names[0]) | Out-Null
    Invoke-Script $collect @('-Version', $version, '-ArtifactDir', $artifacts, '-OutDir', $crossAlias) $false
    Assert-Unchanged $collectorInputs
    Pass 'checksum output cannot alias any input artifact'
    Invoke-Script $collect @('-Version', $version, '-ArtifactDir', $artifacts, '-OutDir', (Join-Path $brokenOut 'new')) $false
    Assert-Unchanged $collectorInputs
    Pass 'collector refuses broken output links before cleanup'

    foreach ($name in $names) {
        $file = Join-Path $artifacts $name
        Move-Item $file "$file.removed"
        $release = Join-Path $scratch ('missing-' + [guid]::NewGuid().ToString('N'))
        Invoke-Script $collect @('-Version', $version, '-ArtifactDir', $artifacts, '-OutDir', $release) $false
        Assert-That (-not (Test-Path (Join-Path $release 'SHA256SUMS.txt'))) 'checksums generated with a missing artifact'
        Move-Item "$file.removed" $file
        Assert-Unchanged $collectorInputs
        Pass "release collection refuses missing $name"
    }
    $empty = Join-Path $artifacts $names[-1]
    $payload = Get-Content -LiteralPath $empty -Raw
    [System.IO.File]::WriteAllText($empty, '')
    $emptyOut = Join-Path $scratch 'empty-release'
    Invoke-Script $collect @('-Version', $version, '-ArtifactDir', $artifacts, '-OutDir', $emptyOut) $false
    Assert-That (-not (Test-Path (Join-Path $emptyOut 'SHA256SUMS.txt'))) 'empty artifact accepted'
    [System.IO.File]::WriteAllText($empty, $payload)
    Assert-Unchanged $collectorInputs
    Pass 'release collection refuses empty artifacts'

    # Inject an I/O failure after the first copy, in a separate process.
    $wrapper = Join-Path $scratch 'copy-failure.ps1'
    Set-Content -LiteralPath $wrapper -Encoding utf8NoBOM -Value @'
param($Collector, $Version, $ArtifactDir, $OutDir)
$ErrorActionPreference = 'Stop'
$global:rimmergeTestCopies = 0
function Copy-Item {
    param($LiteralPath, $Destination, [switch]$Force)
    $global:rimmergeTestCopies++
    if ($global:rimmergeTestCopies -eq 2) { throw 'synthetic copy failure' }
    Microsoft.PowerShell.Management\Copy-Item -LiteralPath $LiteralPath -Destination $Destination -Force:$Force
}
& $Collector -Version $Version -ArtifactDir $ArtifactDir -OutDir $OutDir
'@
    $failedRelease = Join-Path $scratch 'copy-failed-release'
    New-Item -ItemType Directory -Path $failedRelease | Out-Null
    Set-Content (Join-Path $failedRelease 'unrelated.txt') 'keep'
    Invoke-Script $wrapper @('-Collector', $collect, '-Version', $version, '-ArtifactDir', $artifacts, '-OutDir', $failedRelease) $false 'synthetic copy failure'
    Assert-That (@(Get-ChildItem $failedRelease -File).Count -eq 1) 'partial release outputs remain'
    Assert-That ((Get-Content (Join-Path $failedRelease 'unrelated.txt')) -eq 'keep') 'unrelated release file removed'
    Assert-Unchanged $collectorInputs
    Pass 'release copy failure cleans artifacts and checksums without deleting unrelated files'

    $publishWrapper = Join-Path $scratch 'publish-failure.ps1'
    Set-Content -LiteralPath $publishWrapper -Encoding utf8NoBOM -Value @'
param($Packager, $Collector, $Version, $TargetDir, $ArtifactDir, $OutDir)
$ErrorActionPreference = 'Stop'
$global:rimmergeTestPublished = 0
function Copy-Item {
    param($LiteralPath, $Destination, [switch]$Force)
    if ((Split-Path -Leaf $Destination).StartsWith('.rimmerge-')) {
        $global:rimmergeTestPublished++
        if ($global:rimmergeTestPublished -eq 2) {
            Set-Content -LiteralPath $Destination 'synthetic partial copy'
            throw 'synthetic publish failure'
        }
    }
    Microsoft.PowerShell.Management\Copy-Item -LiteralPath $LiteralPath -Destination $Destination -Force:$Force
}
if ($Packager) { & $Packager -Version $Version -TargetDir $TargetDir -OutDir $OutDir }
else { & $Collector -Version $Version -ArtifactDir $ArtifactDir -OutDir $OutDir }
'@
    foreach ($tool in @('package', 'collect')) {
        foreach ($existing in @($false, $true)) {
            $failedOut = Join-Path $scratch "publish-failed-$tool-$existing"
            New-Item -ItemType Directory -Path $failedOut | Out-Null
            $previousFiles = @((Join-Path $failedOut 'unrelated.txt'))
            Set-Content $previousFiles[0] 'unrelated previous file'
            if ($existing) {
                $outputNames = if ($tool -eq 'package') { Package-Names } else {
                    @($names | ForEach-Object { Split-Path -Leaf $_ }) + @('SHA256SUMS.txt')
                }
                foreach ($name in $outputNames) {
                    $file = Join-Path $failedOut $name
                    Set-Content $file "previous $name"
                    $previousFiles += $file
                }
            }
            $previousHashes = Save-Hashes $previousFiles
            $arguments = @('-Version', $version, '-OutDir', $failedOut)
            $arguments += if ($tool -eq 'package') { @('-Packager', $package, '-TargetDir', $target) } else {
                @('-Collector', $collect, '-ArtifactDir', $artifacts)
            }
            Invoke-Script $publishWrapper $arguments $false 'synthetic publish failure'
            Assert-Unchanged $packageInputs
            Assert-Unchanged $collectorInputs
            Assert-Unchanged $previousHashes
            Assert-That (@(Get-ChildItem $failedOut -Force).Count -eq $previousFiles.Count) 'partial publication or pending files remain'
            Pass "$tool publication failure rolls back outputs (previous files: $existing) and preserves input hashes"
        }
    }

    foreach ($tool in @('package', 'collect')) {
        $linkedOut = Join-Path $scratch "file-link-$tool"
        New-Item -ItemType Directory -Path $linkedOut | Out-Null
        $sentinel = Join-Path $scratch "unrelated-$tool.txt"
        Set-Content $sentinel 'unrelated target'
        $leaf = if ($tool -eq 'package') { "Rimmerge_${version}_amd64.deb" } else { 'SHA256SUMS.txt' }
        New-Item -ItemType SymbolicLink -Path (Join-Path $linkedOut $leaf) -Target $sentinel | Out-Null
        $sentinelHash = Save-Hashes @($sentinel)
        if ($tool -eq 'package') {
            Invoke-Script $package @('-Version', $version, '-TargetDir', $target, '-OutDir', $linkedOut) $false 'output is not a regular file'
        } else {
            Invoke-Script $collect @('-Version', $version, '-ArtifactDir', $artifacts, '-OutDir', $linkedOut) $false 'output is not a regular file'
        }
        Assert-Unchanged $sentinelHash
        Assert-Unchanged $packageInputs
        Assert-Unchanged $collectorInputs
        Pass "$tool refuses output file links without changing unrelated targets"

        if ($tool -eq 'package') {
            Invoke-Script $package @('-Version', $version, '-TargetDir', $target, '-OutDir', (Join-Path $target 'rimmerge/new')) $false 'not a directory'
        } else {
            Invoke-Script $collect @('-Version', $version, '-ArtifactDir', $artifacts, '-OutDir', (Join-Path (Join-Path $artifacts $names[0]) 'new')) $false 'not a directory'
        }
        Assert-Unchanged $packageInputs
        Assert-Unchanged $collectorInputs
        Pass "$tool rejects a non-directory ancestor before copying"

        $separate = Join-Path $scratch "separate-$tool"
        New-Item -ItemType Directory -Path $separate | Out-Null
        $separateAlias = Join-Path $scratch "separate-alias-$tool"
        New-Item -ItemType SymbolicLink -Path $separateAlias -Target $separate | Out-Null
        if ($tool -eq 'package') {
            Invoke-Script $package @('-Version', $version, '-TargetDir', $target, '-OutDir', $separateAlias) $true
            Assert-That (@(Get-ChildItem $separate -File).Count -eq 3) 'linked separate package output failed'
        } else {
            Invoke-Script $collect @('-Version', $version, '-ArtifactDir', $artifacts, '-OutDir', $separateAlias) $true
            Assert-That (@(Get-ChildItem $separate -File).Count -eq 6) 'linked separate collection output failed'
        }
        Assert-Unchanged $packageInputs
        Assert-Unchanged $collectorInputs
        Pass "$tool accepts a symlinked output directory physically separate from inputs"
    }

    $release = Join-Path $scratch 'release'
    Invoke-Script $collect @('-Version', $version, '-ArtifactDir', $artifacts, '-OutDir', $release) $true
    Assert-That (@(Get-ChildItem $release -File).Count -eq 6) 'expected five artifacts plus checksums'
    Push-Location $release
    try {
        & $sha256sum '-c' 'SHA256SUMS.txt'
        Assert-That ($LASTEXITCODE -eq 0) 'checksum verification failed'
    } finally { Pop-Location }
    Pass 'all five release artifacts copied and independently verified with sha256sum'
    Write-Output "Summary: $passed packaging checks passed"
}
finally {
    $env:PATH = $savedPath
    foreach ($name in $testVariables) { [Environment]::SetEnvironmentVariable($name, $savedVariables[$name]) }
    if (Test-Path $scratch) { Remove-Item -LiteralPath $scratch -Recurse -Force }
}
