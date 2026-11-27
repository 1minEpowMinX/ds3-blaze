<#
.SYNOPSIS
Writes THIRD_PARTY_NOTICES from the crates locked in ds3-blaze\Cargo.lock, or checks that it is current.

.DESCRIPTION
Renders ds3-blaze\about.hbs with cargo-about, which takes its targets and
accepted licenses from ds3-blaze\about.toml. The output is written with LF line
endings, the form git checks the file out in.

Requires cargo-about with its command line:
cargo install cargo-about --locked --features cli

.PARAMETER Check
Compares a fresh rendering with THIRD_PARTY_NOTICES and fails when they differ,
leaving the file untouched.

.EXAMPLE
powershell -ExecutionPolicy Bypass -File .\tools\notices.ps1 -Check
#>
param([switch]$Check)

$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
$Workspace = Join-Path $Root 'ds3-blaze'
$Notices = Join-Path $Root 'THIRD_PARTY_NOTICES'
$Utf8 = New-Object System.Text.UTF8Encoding $false

$Manifest = Join-Path $Workspace 'Cargo.toml'
cargo fetch --locked --manifest-path $Manifest
if ($LASTEXITCODE -ne 0) { throw 'cargo fetch failed' }

$rendered = [IO.Path]::GetTempFileName()
try {
    # --frozen reads license files only from the fetched crate sources, so the
    # output does not depend on the network: online, cargo-about also queries
    # ClearlyDefined and the crates' git repositories.
    cargo about generate --workspace --frozen --fail `
        --manifest-path $Manifest `
        --config (Join-Path $Workspace 'about.toml') `
        --output-file $rendered `
        (Join-Path $Workspace 'about.hbs')
    if ($LASTEXITCODE -ne 0) { throw 'cargo about failed' }
    # Some crates ship their license files with CRLF endings.
    $text = [IO.File]::ReadAllText($rendered, $Utf8).Replace("`r`n", "`n").TrimEnd("`n") + "`n"
} finally {
    Remove-Item -Force $rendered
}

if ($Check) {
    $current = if (Test-Path $Notices) { [IO.File]::ReadAllText($Notices, $Utf8).Replace("`r`n", "`n") } else { '' }
    if ($current -ne $text) {
        throw 'THIRD_PARTY_NOTICES is out of date: run tools\notices.ps1 and commit the result'
    }
    Write-Host 'THIRD_PARTY_NOTICES is current'
} else {
    [IO.File]::WriteAllText($Notices, $text, $Utf8)
    Write-Host "wrote $Notices"
}
