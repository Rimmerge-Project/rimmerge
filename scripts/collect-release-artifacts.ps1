# Collect the exact Windows/Linux release set before writing checksums.
# Used by release.yml; does not create or publish a release. Output files must
# be regular files physically separate from inputs, including through links.
param(
    [Parameter(Mandatory = $true)][ValidatePattern('^\d+\.\d+\.\d+$')][string]$Version,
    [Parameter(Mandatory = $true)][string]$ArtifactDir,
    [Parameter(Mandatory = $true)][string]$OutDir
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'artifact-paths.ps1')
$artifacts = @(
    "zips-windows/Rimmerge-$Version-windows-x64-portable.zip",
    "zips-windows/rimmerge-cli-$Version-x86_64-pc-windows-msvc.zip",
    "zips-linux/Rimmerge-$Version-linux-x64-portable.tar.gz",
    "zips-linux/rimmerge-cli-$Version-x86_64-unknown-linux-gnu.tar.gz",
    "zips-linux/Rimmerge_${Version}_amd64.deb"
)
foreach ($artifact in $artifacts) {
    $source = Join-Path $ArtifactDir $artifact
    if (-not (Test-Path -LiteralPath $source -PathType Leaf) -or (Get-Item -LiteralPath $source).Length -eq 0) {
        throw "missing or empty release artifact: $source"
    }
}
$sources = @($artifacts | ForEach-Object { Resolve-PhysicalArtifactPath (Join-Path $ArtifactDir $_) })
$outRoot = Resolve-PhysicalArtifactPath $OutDir
$outputs = @($artifacts | ForEach-Object { Join-Path $outRoot (Split-Path -Leaf $_) })
$checksums = Join-Path $outRoot 'SHA256SUMS.txt'
Assert-ArtifactOutputs -Inputs $sources -Outputs (@($outputs) + @($checksums))
$stage = Join-Path ([System.IO.Path]::GetTempPath()) ('rimmerge-collect-' + [guid]::NewGuid().ToString('N'))
$stagedOutputs = @($outputs | ForEach-Object { Join-Path $stage (Split-Path -Leaf $_) })
$stagedChecksums = Join-Path $stage 'SHA256SUMS.txt'
$stageCreated = $false
try {
    New-Item -ItemType Directory -Path $stage | Out-Null
    $stageCreated = $true
    for ($index = 0; $index -lt $sources.Count; $index++) {
        Copy-Item -LiteralPath $sources[$index] -Destination $stagedOutputs[$index]
    }
    $lines = foreach ($output in $stagedOutputs) {
        if (-not (Test-Path -LiteralPath $output -PathType Leaf) -or (Get-Item -LiteralPath $output).Length -eq 0) {
            throw "missing or empty staged artifact: $output"
        }
        $hash = (Get-FileHash -LiteralPath $output -Algorithm SHA256).Hash.ToLowerInvariant()
        "$hash  $(Split-Path -Leaf $output)"
    }
    Set-Content -LiteralPath $stagedChecksums -Value $lines -Encoding utf8NoBOM
    New-Item -ItemType Directory -Force -Path $outRoot | Out-Null
    Publish-ArtifactFiles -Sources (@($stagedOutputs) + @($stagedChecksums)) -Outputs (@($outputs) + @($checksums)) -Stage $stage
    Get-Content -LiteralPath $checksums
}
finally {
    if ($stageCreated) { Remove-Item -LiteralPath $stage -Recurse -Force }
}
