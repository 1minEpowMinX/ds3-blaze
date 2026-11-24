<#
.SYNOPSIS
Adds or removes the hosts entries that send Dead Space 3 to ds3-blaze.

.DESCRIPTION
Every line the script writes ends with the marker "# ds3-blaze"; -Disable
removes exactly those lines and leaves the rest of the file untouched.
Needs an elevated PowerShell. Flushes the DNS cache afterwards.

.PARAMETER Enable
Adds the entries, replacing any earlier ds3-blaze lines.

.PARAMETER Disable
Removes the entries.

.PARAMETER Address
IP the redirector host resolves to. Defaults to 127.0.0.1.

.EXAMPLE
powershell -ExecutionPolicy Bypass -File .\hosts.ps1 -Enable
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

if ($Enable -eq $Disable) {
    Write-Error 'Pass exactly one of -Enable or -Disable.'
}

$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Error 'Run this script from an elevated PowerShell (Run as administrator).'
}

$lines = [IO.File]::ReadAllLines($Path) | Where-Object { -not $_.EndsWith($Marker) }
if ($Enable) {
    $lines = @($lines) + ($Hosts | ForEach-Object { "$Address $_ $Marker" })
}
# The stock hosts file is UTF-8 with BOM; the same encoding is written back.
[IO.File]::WriteAllLines($Path, [string[]]$lines, (New-Object Text.UTF8Encoding($true)))
ipconfig /flushdns | Out-Null

Write-Host "hosts entries marked '$Marker':"
Get-Content $Path | Where-Object { $_.EndsWith($Marker) } | ForEach-Object { Write-Host "  $_" }
if ($Disable) { Write-Host '  (none, removed)' }
