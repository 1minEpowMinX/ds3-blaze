<#
.SYNOPSIS
Adds or removes the hosts entries that send Dead Space 3 to ds3-blaze.

.DESCRIPTION
Every line the script writes ends with the marker "# ds3-blaze"; -Disable
removes exactly those lines and leaves the rest of the file untouched. The file
keeps its encoding (a byte order mark if it had one, the ANSI code page
otherwise) and its line endings.

Before changing the file the script copies it to hosts.ds3-blaze.bak next to it,
so the backup holds the file as it was before the latest run. Needs an elevated
PowerShell. Flushes the DNS cache afterwards.

.PARAMETER Enable
Adds the entries, replacing any earlier ds3-blaze lines.

.PARAMETER Disable
Removes the entries.

.PARAMETER Address
IPv4 address the redirector host resolves to, in dotted form. Defaults to
127.0.0.1.

.EXAMPLE
powershell -ExecutionPolicy Bypass -File .\hosts.ps1 -Enable -Address 26.1.2.3
#>
param(
    [switch]$Enable,
    [switch]$Disable,
    [string]$Address = '127.0.0.1'
)

$ErrorActionPreference = 'Stop'
$Marker = '# ds3-blaze'
$Hosts = @('gosredirector.ea.com')
$Path = Join-Path $env:SystemRoot 'System32\drivers\etc\hosts'
$Backup = "$Path.ds3-blaze.bak"

if ($Enable -eq $Disable) {
    Write-Error 'Pass exactly one of -Enable or -Disable.'
}

# TryParse also takes shorthand such as "26.1" or a bare number; comparing the
# parsed address back with the input admits only the four-part dotted form.
$parsed = $null
if (-not [Net.IPAddress]::TryParse($Address, [ref]$parsed) -or
    $parsed.AddressFamily -ne [Net.Sockets.AddressFamily]::InterNetwork -or
    $parsed.ToString() -ne $Address) {
    Write-Error "'$Address' is not an IPv4 address such as 26.1.2.3."
}

$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Error 'Run this script from an elevated PowerShell (Run as administrator).'
}

# A byte order mark selects its encoding; without one the file is read in the
# ANSI code page, which maps every byte to a character and back unchanged.
$reader = New-Object IO.StreamReader($Path, [Text.Encoding]::Default, $true)
try {
    $text = $reader.ReadToEnd()
    $encoding = $reader.CurrentEncoding
} finally {
    $reader.Dispose()
}
$newline = if ($text.Contains("`r`n")) { "`r`n" } else { "`n" }

$lines = if ($text) { @($text -split '\r?\n') } else { @() }
# A final line break leaves an empty last element, which the join restores.
$endsWithNewline = $lines.Count -gt 1 -and $lines[-1] -eq ''
if ($endsWithNewline) { $lines = @($lines | Select-Object -SkipLast 1) }
$lines = @($lines | Where-Object { -not $_.EndsWith($Marker) })
if ($Enable) {
    $lines += $Hosts | ForEach-Object { "$Address $_ $Marker" }
}
$updated = $lines -join $newline
if ($lines.Count -gt 0 -and ($endsWithNewline -or $Enable)) { $updated += $newline }

Copy-Item -LiteralPath $Path -Destination $Backup -Force
[IO.File]::WriteAllText($Path, $updated, $encoding)
Write-Host "previous hosts file saved as $Backup"

ipconfig /flushdns | Out-Null
if ($LASTEXITCODE -ne 0) {
    Write-Warning "ipconfig /flushdns failed with exit code $LASTEXITCODE; the game may keep the old address until the DNS cache expires or Windows restarts."
}

Write-Host "hosts entries marked '$Marker':"
Get-Content $Path | Where-Object { $_.EndsWith($Marker) } | ForEach-Object { Write-Host "  $_" }
if ($Disable) { Write-Host '  (none, removed)' }
