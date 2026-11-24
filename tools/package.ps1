<#
.SYNOPSIS
Builds ds3-blaze in release mode and packs the files a player needs into a zip.

.DESCRIPTION
The archive holds the files listed in $entries, laid out so that the commands
in README.md run from the unpacked folder. Build
by-products in target\release (.pdb, .d, deps, incremental) stay out. A
<archive>.sha256 file with the archive's SHA-256 is written next to it.

.PARAMETER Tag
Release tag the version must match, as "v<version>"; the script fails on a
mismatch. Omitted, the version is not checked.

.PARAMETER OutDir
Directory the archive is written to. Defaults to dist\ in the repository root.

.PARAMETER SkipBuild
Packs the existing target\release\ds3-blaze.exe without running cargo.

.OUTPUTS
The full path of the archive.

.EXAMPLE
powershell -ExecutionPolicy Bypass -File .\tools\package.ps1
#>
param(
    [string]$Tag,
    [string]$OutDir,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
$Manifest = Join-Path $Root 'ds3-blaze\Cargo.toml'
if (-not $OutDir) { $OutDir = Join-Path $Root 'dist' }

# cargo owns the name, version and target directory; nothing here repeats them.
$metadata = cargo metadata --no-deps --format-version 1 --manifest-path $Manifest | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed' }
$package = $metadata.packages[0]
$version = $package.version
if ($Tag -and $Tag -ne "v$version") {
    throw "tag $Tag does not match crate version $version (expected v$version)"
}

if (-not $SkipBuild) {
    cargo build --release --locked --manifest-path $Manifest
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
}

$exe = Join-Path $metadata.target_directory "release\$($package.name).exe"
if (-not (Test-Path $exe)) { throw "$exe not found; build without -SkipBuild" }

$entries = [ordered]@{
    "$($package.name).exe" = $exe
    'scripts/hosts.ps1'    = Join-Path $Root 'ds3-blaze\scripts\hosts.ps1'
    'README.md'            = Join-Path $Root 'README.md'
    'LICENSE'              = Join-Path $Root 'LICENSE'
}

$name = "$($package.name)-$version-windows-x64"
New-Item -ItemType Directory -Force $OutDir | Out-Null
$archive = Join-Path (Resolve-Path $OutDir).Path "$name.zip"
if (Test-Path $archive) { Remove-Item -Force $archive }

# Entries are added one by one with '/' separators: Compress-Archive in Windows
# PowerShell 5.1 stores '\', which unzip on other systems keeps in file names.
Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
$zip = [IO.Compression.ZipFile]::Open($archive, [IO.Compression.ZipArchiveMode]::Create)
try {
    foreach ($entry in $entries.GetEnumerator()) {
        [IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
            $zip, $entry.Value, $entry.Key, [IO.Compression.CompressionLevel]::Optimal) | Out-Null
    }
} finally {
    $zip.Dispose()
}

$hash = (Get-FileHash -Algorithm SHA256 $archive).Hash.ToLowerInvariant()
# sha256sum format, so `sha256sum -c` verifies it.
[IO.File]::WriteAllText("$archive.sha256", "$hash  $name.zip`n")

Write-Host "packed $archive"
$archive
