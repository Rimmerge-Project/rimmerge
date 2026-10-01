#Requires -Version 7
<#
.SYNOPSIS
    Compiles the synthetic install generator's committed assembly
    fixtures.

.DESCRIPTION
    Builds every `crates/rim-resolve/tests/fixtures/synthetic/assemblies/src/*.cs`
    source into `crates/rim-resolve/tests/fixtures/synthetic/assemblies/bin/*.dll`
    with `csc.exe` (the .NET Framework reference assembler — no network,
    no `dotnet build`/project file needed for a handful of single-file
    libraries). Run by hand only when a `.cs` source changes; the
    synthetic install generator itself only ever copies the committed
    `.dll` blobs, never recompiles them (see `assemblies/README.md`).

    Rebuilding reproduces every blob's *classification* identically
    (assembly name, references, runtime-patch targets) but not its bytes: csc
    stamps a COFF timestamp and a fresh MVID (module version id) into
    every build, so two rebuilds of the same source differ by a handful
    of bytes even with no source change. `assemblies/README.md` has the
    full disclosure — the committed blobs are the reference; nothing
    hashes them.
#>

$ErrorActionPreference = 'Stop'

$csc = 'C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe'
if (-not (Test-Path $csc)) {
    throw "csc.exe not found at $csc -- this script needs the .NET Framework reference assembler."
}

$root = Split-Path -Parent $PSScriptRoot
$src = Join-Path $root 'crates\rim-resolve\tests\fixtures\synthetic\assemblies\src'
$bin = Join-Path $root 'crates\rim-resolve\tests\fixtures\synthetic\assemblies\bin'
New-Item -ItemType Directory -Force -Path $bin | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $bin 'versioned') | Out-Null

function Build-Library {
    param(
        [Parameter(Mandatory)][string]$Name,
        [string[]]$References = @(),
        # The compiled `.cs` file's own base name, when it differs from
        # `-Name` (the *output* assembly's name, i.e. its `Assembly`
        # table row) -- `ExportsVersioned.cs` compiles to `Exports.dll`
        # so its `Assembly` table name matches the plain copy's.
        [string]$SourceName,
        # Where to write the output, when it isn't `$bin\$Name.dll` --
        # `versioned\Exports.dll`, so it never overwrites the plain copy.
        [string]$OutPath
    )
    $sourcePath = Join-Path $src "$(if ($SourceName) { $SourceName } else { $Name }).cs"
    $resolvedOutPath = if ($OutPath) { Join-Path $bin $OutPath } else { Join-Path $bin "$Name.dll" }
    $cscArgs = @('/nologo', '/target:library', "/out:$resolvedOutPath")
    foreach ($reference in $References) {
        $cscArgs += "/reference:$(Join-Path $bin "$reference.dll")"
    }
    $cscArgs += $sourcePath
    & $csc @cscArgs
    if ($LASTEXITCODE -ne 0) {
        throw "csc.exe failed for $Name (exit $LASTEXITCODE)"
    }
    Write-Host "built $resolvedOutPath"
}

# Base fixtures (see assemblies/README.md).
Build-Library -Name 'MiniPatchLib'
Build-Library -Name 'Exports'
Build-Library -Name 'ExtendsHard' -References @('Exports')
Build-Library -Name 'CallsSoft' -References @('Exports')

# Single-owner Hard-AssemblyRef chain: unlike `Exports`
# (deliberately 2 owners, always ambiguous -> `AnyOf`), `ExportsSolo` is
# shipped by exactly one mod, so `ExtendsSolo`'s reference to it is a
# real, unambiguous `AssemblyRef` edge.
Build-Library -Name 'ExportsSolo'
Build-Library -Name 'ExtendsSolo' -References @('ExportsSolo')

# The higher-version half of the `AssemblyVersionPrecedence` pair: same
# `Assembly` table name ("exports") as the plain copy,
# different version (1.1.0.0 vs. 1.0.0.0), written to `versioned\` so it
# never overwrites `bin\Exports.dll`.
Build-Library -Name 'Exports' -SourceName 'ExportsVersioned' -OutPath 'versioned\Exports.dll'

# Six runtime-patch-shaped target pairs (twelve assemblies) — see
# assemblies/README.md for which targets are Transpiler-vs-Transpiler
# (TranspilerCollision) and which are Prefix-vs-Postfix
# (RuntimePatchCollision only).
1..12 | ForEach-Object {
    $n = 'SynthPatch{0:D2}' -f $_
    Build-Library -Name $n -References @('MiniPatchLib')
}

Write-Host "done: $((Get-ChildItem $bin -Recurse -Filter '*.dll').Count) assemblies in $bin"
