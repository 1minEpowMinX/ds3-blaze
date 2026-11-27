<#
.SYNOPSIS
Fails when tracked or staged files include logs, captures, game files, keys or personal data.

.DESCRIPTION
Checks each file against $ForbiddenPaths and $MaxBytes, and searches text files
other than $EmailExemptFiles for e-mail addresses not listed in $AllowedEmails. When .git\info\sensitive.txt
exists, every non-empty line of it that does not start with '#' is searched
for as a literal string as well; the file lives inside .git and is never
committed, so it can hold a persona name or a home IP address.

.PARAMETER Staged
Checks the files staged for the next commit instead of all tracked files.

.EXAMPLE
powershell -ExecutionPolicy Bypass -File .\tools\check-clean.ps1 -Staged
#>
param([switch]$Staged)

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
Set-Location (Split-Path -Parent $PSScriptRoot)

$ForbiddenPaths = @(
    '(^|/)logs/', '\.log$',
    '\.pcapng?$', '\.dmp$',
    '\.(exe|dll|sb|toc)$',
    '\.(pem|key|pfx|p12|der|crt)$',
    '(^|/)(target|vendor|dist)/',
    '\.bak$'
)
$MaxBytes = 1MB
$AllowedEmails = @('dirtysock-contact@ea.com', 'noreply@anthropic.com')
# Files whose e-mail addresses are not searched for. THIRD_PARTY_NOTICES
# reproduces upstream license texts, which carry their authors' addresses.
$EmailExemptFiles = @('THIRD_PARTY_NOTICES')
$EmailPattern = '[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}'
$SensitiveFile = '.git/info/sensitive.txt'

if ($Staged) {
    $files = @(git diff --cached --name-only --diff-filter=ACMR)
} else {
    $files = @(git ls-files)
}
if ($LASTEXITCODE -ne 0) { throw 'git failed to list files' }
$files = @($files | Where-Object { $_ })
if ($files.Count -eq 0) { exit 0 }

$problems = New-Object System.Collections.Generic.List[string]

foreach ($file in $files) {
    foreach ($pattern in $ForbiddenPaths) {
        if ($file -match $pattern) { $problems.Add("$file matches the forbidden pattern '$pattern'") }
    }
    # The index holds what is committed, so sizes are taken from it.
    $size = [long](git cat-file -s ":$file")
    if ($size -gt $MaxBytes) { $problems.Add("$file is $size bytes, over the $MaxBytes byte limit") }
}

# git grep --cached searches the index; -I skips binary files.
$emailFiles = @($files | Where-Object { $EmailExemptFiles -notcontains $_ })
$hits = @()
if ($emailFiles.Count -gt 0) { $hits = @(git grep --cached -I -n -o -E $EmailPattern -- $emailFiles) }
foreach ($hit in $hits) {
    $email = ($hit -split ':', 3)[2]
    if ($AllowedEmails -notcontains $email.ToLowerInvariant()) {
        $problems.Add("e-mail address at $(($hit -split ':', 3)[0..1] -join ':'): add it to `$AllowedEmails in tools\check-clean.ps1 if it is meant to be public")
    }
}

if (Test-Path $SensitiveFile) {
    $needles = @(Get-Content -Encoding UTF8 $SensitiveFile | Where-Object { $_.Trim() -and -not $_.StartsWith('#') })
    if ($needles.Count -gt 0) {
        $grepArgs = @('grep', '--cached', '-I', '-n', '-F')
        foreach ($needle in $needles) { $grepArgs += @('-e', $needle.Trim()) }
        # Only the location is printed, so the sensitive value itself stays off the screen.
        foreach ($hit in @(git @grepArgs -- $files)) {
            $problems.Add("value from $SensitiveFile found at $(($hit -split ':', 3)[0..1] -join ':')")
        }
    }
}

if ($problems.Count -gt 0) {
    Write-Host 'repository check failed:'
    $problems | ForEach-Object { Write-Host "  $_" }
    exit 1
}
Write-Host "repository check passed ($($files.Count) files)"
