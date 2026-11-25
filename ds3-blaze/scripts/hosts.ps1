<#
.SYNOPSIS
Adds or removes the hosts entries that send Dead Space 3 to ds3-blaze.

.DESCRIPTION
The entries go at the end of the file in a section that opens with $BeginLine
and closes with $EndLine, as Docker Desktop and similar programs mark theirs.
-Disable removes that section, and any line that ends with $LegacyMarker as
earlier versions wrote them, and leaves the rest of the file untouched. The file
keeps its encoding (a byte order mark if it had one, the ANSI code page
otherwise) and its line endings.

Before changing the file the script writes hosts.ds3-blaze.bak next to it: the
file as it was before the latest run, minus every ds3-blaze entry, so restoring
it never brings the redirect back. Needs an elevated PowerShell. Flushes the DNS
cache afterwards.

.PARAMETER Enable
Adds the section, replacing any earlier ds3-blaze entries.

.PARAMETER Disable
Removes the ds3-blaze entries.

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
$BeginLine = '# Added by ds3-blaze'
# Unlike Docker Desktop's "# End of section", this end line names its owner, so
# removing the section never reaches into another program's.
$EndLine = '# End of ds3-blaze section'
$LegacyMarker = '# ds3-blaze'
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
$kept = New-Object Collections.Generic.List[string]
$inSection = $false
foreach ($line in $lines) {
    $trimmed = $line.Trim()
    if ($inSection) {
        if ($trimmed -eq $EndLine) { $inSection = $false }
    } elseif ($trimmed -eq $BeginLine) {
        $inSection = $true
    } elseif (-not $line.EndsWith($LegacyMarker)) {
        $kept.Add($line)
    }
}
# Without its end line the section's extent is unknown, and guessing could
# delete the user's own entries.
if ($inSection) {
    Write-Error "$Path has '$BeginLine' without '$EndLine'; remove the ds3-blaze lines by hand, then run the script again."
}

# Returns $Lines joined with the file's line break, with a final one when
# $Trailing is set and there is any line.
function Join-HostsLines([string[]]$Lines, [bool]$Trailing) {
    $joined = $Lines -join $newline
    if ($Lines.Count -gt 0 -and $Trailing) { $joined += $newline }
    $joined
}

# The backup is taken from the file without ds3-blaze entries: a copy taken
# while an entry is present would restore the redirect, which -Disable cannot
# remove from the backup.
$clean = Join-HostsLines $kept.ToArray() $endsWithNewline
if ($Enable) {
    $kept.Add($BeginLine)
    $Hosts | ForEach-Object { $kept.Add("$Address $_") }
    $kept.Add($EndLine)
}
$lines = $kept.ToArray()
$updated = Join-HostsLines $lines ($endsWithNewline -or $Enable)

[IO.File]::WriteAllText($Backup, $clean, $encoding)
[IO.File]::WriteAllText($Path, $updated, $encoding)
Write-Host "previous hosts file without ds3-blaze entries saved as $Backup"

ipconfig /flushdns | Out-Null
if ($LASTEXITCODE -ne 0) {
    Write-Warning "ipconfig /flushdns failed with exit code $LASTEXITCODE; the game may keep the old address until the DNS cache expires or Windows restarts."
}

if ($Enable) {
    Write-Host 'hosts section added:'
    $lines | Select-Object -Last ($Hosts.Count + 2) | ForEach-Object { Write-Host "  $_" }
} else {
    Write-Host 'hosts entries of ds3-blaze removed'
}
