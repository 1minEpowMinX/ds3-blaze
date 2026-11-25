<#
.SYNOPSIS
Fails when a branch name or a commit subject breaks the repository's naming rules.

.DESCRIPTION
A commit subject must read `<type>(<scope>)!: <description>` (Conventional
Commits): `<type>` from $Types, an optional lower-case `<scope>`, an optional
`!`, and a description that starts with no capital letter and ends with no full
stop. Merge commits are not checked: their subjects describe the branch.

A branch other than main must be named `<type>/<name>`, with `<type>` from
$Types and `<name>` in lower case.

.PARAMETER Range
Commits to check, as a git revision range such as origin/main..HEAD.

.PARAMETER MessageFile
A commit message being written, as the commit-msg hook receives it. Lines
starting with '#' are ignored. Together with -Branch main, the commit is
refused unless it concludes a merge, since work lands on main only by merging
a branch.

.PARAMETER Branch
Branch name to check.

.EXAMPLE
powershell -ExecutionPolicy Bypass -File .\tools\check-commits.ps1 -Range origin/main..HEAD -Branch feat/game-list
#>
param(
    [string]$Range,
    [string]$MessageFile,
    [string]$Branch
)

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
Set-Location (Split-Path -Parent $PSScriptRoot)

# The commit types of this repository; branch prefixes use the same set.
$Types = @('feat', 'fix', 'security', 'refactor', 'perf', 'style', 'test', 'docs', 'build', 'ops', 'chore')
$MainBranch = 'main'

$SubjectPattern = '^(?<type>[a-z0-9]+)(\((?<scope>[^)]*)\))?!?: (?<description>.*)$'
$ScopePattern = '^[a-z0-9][a-z0-9-]*$'
$BranchPattern = '^(?<type>[a-z0-9]+)/[a-z0-9][a-z0-9._-]*$'

$problems = New-Object System.Collections.Generic.List[string]

function Test-Subject([string]$Subject, [string]$Where) {
    if ($Subject -cnotmatch $SubjectPattern) {
        $problems.Add("$Where`: '$Subject' is not '<type>(<scope>): <description>'")
        return
    }
    # Every later -match replaces $Matches, so the parts are copied first.
    $type, $scope, $description = $Matches.type, $Matches.scope, [string]$Matches.description
    if ($Types -cnotcontains $type) {
        $problems.Add("$Where`: type '$type' is not one of $($Types -join ', ')")
    }
    if ($scope -and $scope -cnotmatch $ScopePattern) {
        $problems.Add("$Where`: scope '$scope' must be lower case letters, digits and hyphens")
    }
    if (-not $description.Trim()) {
        $problems.Add("$Where`: the description is empty")
    } elseif ($description -cmatch '^[A-Z]') {
        $problems.Add("$Where`: the description starts with a capital letter")
    } elseif ($description.EndsWith('.')) {
        $problems.Add("$Where`: the description ends with a full stop")
    }
}

if ($Branch -and $Branch -ne $MainBranch) {
    if ($Branch -cnotmatch $BranchPattern) {
        $problems.Add("branch '$Branch' is not '<type>/<name>' in lower case")
    } elseif ($Types -cnotcontains $Matches.type) {
        $problems.Add("branch '$Branch': type '$($Matches.type)' is not one of $($Types -join ', ')")
    }
}

if ($MessageFile) {
    $merging = Test-Path (Join-Path (git rev-parse --git-dir) 'MERGE_HEAD')
    $subject = Get-Content -Encoding UTF8 $MessageFile |
        Where-Object { $_.Trim() -and -not $_.StartsWith('#') } |
        Select-Object -First 1
    if ($Branch -eq $MainBranch -and -not $merging) {
        $problems.Add("commit on $MainBranch`: create a '<type>/<name>' branch and merge it instead")
    }
    if (-not $merging) {
        Test-Subject ([string]$subject) 'commit message'
    }
}

if ($Range) {
    # One line per commit: hash, parent hashes, subject.
    $log = @(git log --format='%h%x09%p%x09%s' $Range)
    if ($LASTEXITCODE -ne 0) { throw "git log failed for range $Range" }
    foreach ($line in $log) {
        $hash, $parents, $subject = $line -split "`t", 3
        if (@($parents -split ' ').Count -gt 1) { continue }
        Test-Subject $subject $hash
    }
}

if ($problems.Count -gt 0) {
    Write-Host 'naming check failed:'
    $problems | ForEach-Object { Write-Host "  $_" }
    exit 1
}
Write-Host 'naming check passed'
