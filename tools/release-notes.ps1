<#
.SYNOPSIS
Writes Markdown release notes for a tag from its Conventional Commits.

.DESCRIPTION
Takes the commits after the previous tag up to -Tag (or from the first commit
when there is no earlier tag) and groups their subjects by type in the order of
$Sections. Deploy: trailers are collected under "Action required" and commits
marked with "!" under "Breaking changes", both at the top. Subjects that do not
follow the convention go under "Other".

.PARAMETER Tag
Tag the notes describe. Defaults to HEAD, for a preview before tagging.

.PARAMETER OutFile
File the notes are written to as UTF-8. Omitted, the notes go to the output.

.EXAMPLE
powershell -ExecutionPolicy Bypass -File .\tools\release-notes.ps1 -Tag v0.1.0
#>
param(
    [string]$Tag = 'HEAD',
    [string]$OutFile
)

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
Set-Location (Split-Path -Parent $PSScriptRoot)

$Sections = [ordered]@{
    feat     = 'Features'
    fix      = 'Fixes'
    security = 'Security'
    perf     = 'Performance'
    refactor = 'Refactoring'
    docs     = 'Documentation'
    test     = 'Tests'
    build    = 'Build'
    ops      = 'CI and operations'
    style    = 'Style'
    chore    = 'Chores'
}

# git describe exits non-zero when no earlier tag exists; the range then starts at
# the root. Windows PowerShell turns its stderr into a terminating error under
# 'Stop', so the preference is relaxed for this one call.
$ErrorActionPreference = 'Continue'
$previous = git describe --tags --abbrev=0 "$Tag^" 2>$null
$ErrorActionPreference = 'Stop'
$range = if ($LASTEXITCODE -eq 0 -and $previous) { "$previous..$Tag" } else { $Tag }

# Unit separator between fields, record separator between commits: subjects and
# trailers never contain either.
$log = (git log --no-merges --format='%s%x1f%(trailers:key=Deploy,valueonly,separator=%x1d)%x1e' $range) -join "`n"
if ($LASTEXITCODE -ne 0) { throw "git log $range failed" }

$groups = @{}
$breaking = New-Object System.Collections.Generic.List[string]
$deploy = New-Object System.Collections.Generic.List[string]
foreach ($record in $log -split [char]0x1e) {
    $record = $record.Trim()
    if (-not $record) { continue }
    $subject, $trailers = $record -split [char]0x1f, 2
    foreach ($line in ($trailers -split [char]0x1d)) {
        if ($line.Trim()) { $deploy.Add($line.Trim()) }
    }
    if ($subject -match '^(?<type>[a-z0-9]+)(\((?<scope>[^)]+)\))?(?<bang>!)?: (?<text>.+)$') {
        $type = $Matches.type
        $entry = if ($Matches.scope) { "**$($Matches.scope):** $($Matches.text)" } else { $Matches.text }
        if ($Matches.bang) { $breaking.Add($entry) }
        if (-not $Sections.Contains($type)) { $type = 'other' }
    } else {
        $type = 'other'
        $entry = $subject
    }
    if (-not $groups.ContainsKey($type)) { $groups[$type] = New-Object System.Collections.Generic.List[string] }
    $groups[$type].Add($entry)
}

$out = New-Object System.Collections.Generic.List[string]
function Add-Section([string]$Title, $Entries) {
    if (-not $Entries -or $Entries.Count -eq 0) { return }
    $out.Add("## $Title")
    $out.Add('')
    foreach ($entry in $Entries) { $out.Add("- $entry") }
    $out.Add('')
}
Add-Section 'Action required' $deploy
Add-Section 'Breaking changes' $breaking
foreach ($type in $Sections.Keys) { Add-Section $Sections[$type] $groups[$type] }
Add-Section 'Other' $groups['other']
if ($out.Count -eq 0) { $out.Add("No changes in $range.") }

$text = ($out -join "`n").TrimEnd() + "`n"
if ($OutFile) {
    [IO.File]::WriteAllText($OutFile, $text, (New-Object Text.UTF8Encoding($false)))
} else {
    $text
}
