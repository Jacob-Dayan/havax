$ErrorActionPreference = 'Stop'

$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    $scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
    $hostExe = (Get-Process -Id $PID).Path
    Start-Process $hostExe -ArgumentList "-NoProfile -ExecutionPolicy Bypass -File `"$PSCommandPath`"" -WorkingDirectory $scriptDir -Verb RunAs
    exit
}

$repoRoot = (Resolve-Path "$PSScriptRoot\..\..").Path
Set-Location $repoRoot

cargo build --profile opt

$targetDir = Join-Path $env:ProgramFiles 'Havax\app'
if (-not (Test-Path $targetDir)) {
    New-Item -ItemType Directory -Force -Path $targetDir | Out-Null
}

$sourceExe = Join-Path $repoRoot 'target\opt\havax.exe'
if (-not (Test-Path $sourceExe)) {
    try {
        $cargoTarget = (cargo metadata --format-version 1 --no-deps | ConvertFrom-Json).target_directory
        if ($cargoTarget) {
            $candidate = Join-Path $cargoTarget 'opt\havax.exe'
            if (Test-Path $candidate) {
                $sourceExe = $candidate
            }
        }
    } catch {
        # ignore metadata failure, fallback to original sourceExe path
    }
}

$destExe = Join-Path $targetDir 'havax.exe'
$destAliasExe = Join-Path $targetDir 'hv.exe'
Copy-Item -Force -Path $sourceExe -Destination $destExe
Copy-Item -Force -Path $sourceExe -Destination $destAliasExe

$systemPath = [Environment]::GetEnvironmentVariable('Path', 'Machine')
$pathParts = if ($systemPath) { $systemPath -split ';' } else { @() }
if ($targetDir -notin $pathParts) {
    $newPath = if ($systemPath) { "$systemPath;$targetDir" } else { $targetDir }
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'Machine')
}

if ($targetDir -notin ($env:Path -split ';')) {
    $env:Path = "$env:Path;$targetDir"
}

& $destExe -V
& $destAliasExe -V

