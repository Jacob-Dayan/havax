$ErrorActionPreference = 'Stop'

$repoRoot = (Resolve-Path "$PSScriptRoot\..\..").Path
Set-Location $repoRoot

cargo build --release

$targetDir = Join-Path $env:LOCALAPPDATA 'Havax\app'
if (-not (Test-Path $targetDir)) {
    New-Item -ItemType Directory -Force -Path $targetDir | Out-Null
}

$sourceExe = Join-Path $repoRoot 'target\release\havax.exe'
if (-not (Test-Path $sourceExe)) {
    try {
        $cargoTarget = (cargo metadata --format-version 1 --no-deps | ConvertFrom-Json).target_directory
        if ($cargoTarget) {
            $candidate = Join-Path $cargoTarget 'release\havax.exe'
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

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$pathParts = if ($userPath) { $userPath -split ';' } else { @() }
if ($targetDir -notin $pathParts) {
    $newPath = if ($userPath) { "$userPath;$targetDir" } else { $targetDir }
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
}

if ($targetDir -notin ($env:Path -split ';')) {
    $env:Path = "$env:Path;$targetDir"
}

& $destExe -V
& $destAliasExe -V

