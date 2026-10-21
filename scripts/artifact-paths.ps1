# Shared filesystem checks for packaging and release collection.
function Resolve-PhysicalArtifactPath {
    param([Parameter(Mandatory)][string]$Path, [int]$LinkDepth = 0)
    if ($LinkDepth -gt 40) { throw "too many filesystem links: $Path" }
    if (-not [System.IO.Path]::IsPathFullyQualified($Path)) {
        $Path = if ([System.IO.Path]::IsPathRooted($Path)) {
            [System.IO.Path]::GetFullPath($Path)
        } else {
            [System.IO.Path]::Combine((Get-Location).ProviderPath, $Path)
        }
    }
    $root = [System.IO.Path]::GetPathRoot($Path)
    $separator = if ($IsWindows) { '[/\\]' } else { '/' }
    $resolved = $root
    $parentIsFile = $false
    foreach ($part in ($Path.Substring($root.Length) -split $separator)) {
        if ($part -eq '') { continue }
        if ($parentIsFile) { throw "path ancestor is not a directory: $resolved" }
        if ($part -eq '.') { continue }
        if ($part -eq '..') {
            if ($resolved -ne $root) { $resolved = [System.IO.Path]::GetDirectoryName($resolved) }
            continue
        }
        $resolved = [System.IO.Path]::Combine($resolved, $part)
        try { $attributes = [System.IO.File]::GetAttributes($resolved) }
        catch [System.IO.FileNotFoundException] { continue }
        catch [System.IO.DirectoryNotFoundException] { continue }
        if (($attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
            $item = if (($attributes -band [System.IO.FileAttributes]::Directory) -ne 0) {
                [System.IO.DirectoryInfo]::new($resolved)
            } else { [System.IO.FileInfo]::new($resolved) }
            $target = $item.LinkTarget
            if (-not $target) { throw "unsupported filesystem link: $resolved" }
            if (-not [System.IO.Path]::IsPathFullyQualified($target)) {
                $target = [System.IO.Path]::Combine([System.IO.Path]::GetDirectoryName($resolved), $target)
            }
            $resolved = Resolve-PhysicalArtifactPath -Path $target -LinkDepth ($LinkDepth + 1)
            # A broken link is an error, rather than a new output directory.
            $attributes = [System.IO.File]::GetAttributes($resolved)
        }
        $parentIsFile = ($attributes -band [System.IO.FileAttributes]::Directory) -eq 0
    }
    return $resolved
}

function Assert-ArtifactOutputs {
    param([string[]]$Inputs, [string[]]$Outputs)
    $comparison = if ($IsWindows) { [StringComparison]::OrdinalIgnoreCase } else { [StringComparison]::Ordinal }
    for ($index = 0; $index -lt $Outputs.Count; $index++) {
        $output = $Outputs[$index]
        $physicalOutput = Resolve-PhysicalArtifactPath $output
        foreach ($other in @($Inputs) + @($Outputs | Select-Object -First $index)) {
            if ([string]::Equals($physicalOutput, $other, $comparison)) { throw "output aliases an input or output: $output" }
        }
        try {
            if (([System.IO.File]::GetAttributes($output) -band ([System.IO.FileAttributes]::Directory -bor [System.IO.FileAttributes]::ReparsePoint)) -ne 0) {
                throw "output is not a regular file: $output"
            }
        }
        catch [System.IO.FileNotFoundException] { }
        catch [System.IO.DirectoryNotFoundException] { }
    }
}

# Publish complete staged files. Roll back only files this invocation touched.
function Publish-ArtifactFiles {
    param([string[]]$Sources, [string[]]$Outputs, [string]$Stage)
    $changed = [System.Collections.Generic.List[object]]::new()
    try {
        for ($index = 0; $index -lt $Outputs.Count; $index++) {
            $output = $Outputs[$index]
            $backup = $null
            if ([System.IO.File]::Exists($output)) {
                $backup = Join-Path $Stage "previous-$index"
                [System.IO.File]::Copy($output, $backup)
            }
            $pending = Join-Path ([System.IO.Path]::GetDirectoryName($output)) ('.rimmerge-' + [guid]::NewGuid().ToString('N'))
            try {
                Copy-Item -LiteralPath $Sources[$index] -Destination $pending
                [System.IO.File]::Move($pending, $output, $true)
                $changed.Add(@{ Path = $output; Backup = $backup })
            } finally {
                if ([System.IO.File]::Exists($pending)) { [System.IO.File]::Delete($pending) }
            }
        }
    } catch {
        for ($index = $changed.Count - 1; $index -ge 0; $index--) {
            $entry = $changed[$index]
            if ($entry.Backup) { [System.IO.File]::Copy($entry.Backup, $entry.Path, $true) }
            elseif ([System.IO.File]::Exists($entry.Path)) { [System.IO.File]::Delete($entry.Path) }
        }
        throw
    }
}
