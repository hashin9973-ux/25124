param(
    [string]$Configuration = "Release"
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$DistRoot = Join-Path $Root "dist"
$PackageName = "physics-sandbox-windows-x64"
$Dist = Join-Path $DistRoot $PackageName
$Archive = Join-Path $DistRoot "$PackageName.zip"

Push-Location $Root
try {
    if ($Configuration -ne "Release") {
        throw "Portable packages must use Release configuration."
    }

    Write-Host "==> Building locked Windows release with a statically linked MSVC runtime"
    $OriginalRustFlags = $env:RUSTFLAGS
    if ($env:RUSTFLAGS -notmatch 'target-feature=\+crt-static') {
        if ([string]::IsNullOrWhiteSpace($env:RUSTFLAGS)) {
            $env:RUSTFLAGS = "-C target-feature=+crt-static"
        }
        else {
            $env:RUSTFLAGS = "$($env:RUSTFLAGS) -C target-feature=+crt-static"
        }
    }
    try {
        & cargo build --release --locked --target x86_64-pc-windows-msvc
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build --release --locked failed with exit code $LASTEXITCODE"
        }
    }
    finally {
        if ($null -eq $OriginalRustFlags) {
            Remove-Item Env:RUSTFLAGS -ErrorAction SilentlyContinue
        }
        else {
            $env:RUSTFLAGS = $OriginalRustFlags
        }
    }

    $Executable = Join-Path $Root "target/x86_64-pc-windows-msvc/release/physics_sandbox.exe"
    if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
        throw "Release executable was not produced: $Executable"
    }

    if (Test-Path -LiteralPath $Dist) {
        Remove-Item -LiteralPath $Dist -Recurse -Force
    }
    if (Test-Path -LiteralPath $Archive) {
        Remove-Item -LiteralPath $Archive -Force
    }

    New-Item -ItemType Directory -Path $Dist -Force | Out-Null
    Copy-Item -LiteralPath $Executable -Destination $Dist -Force

    foreach ($Directory in @("data", "assets", "worlds")) {
        $Source = Join-Path $Root $Directory
        if (-not (Test-Path -LiteralPath $Source -PathType Container)) {
            throw "Required portable content directory is missing: $Source"
        }
        Copy-Item -LiteralPath $Source -Destination (Join-Path $Dist $Directory) -Recurse -Force
    }

    # Mods are an optional extension point. Always create the directory so
    # the portable layout remains stable even when no mods are installed.
    $ModsSource = Join-Path $Root "mods"
    $ModsDestination = Join-Path $Dist "mods"
    New-Item -ItemType Directory -Path $ModsDestination -Force | Out-Null
    if (Test-Path -LiteralPath $ModsSource -PathType Container) {
        Get-ChildItem -LiteralPath $ModsSource -Force |
            Copy-Item -Destination $ModsDestination -Recurse -Force
    }

    $VersionText = Get-Content -LiteralPath (Join-Path $Root "Cargo.toml") -Raw
    $VersionMatch = [regex]::Match($VersionText, '(?m)^version\s*=\s*"([^"]+)"')
    if (-not $VersionMatch.Success) {
        throw "Could not read package version from Cargo.toml"
    }
    $Version = $VersionMatch.Groups[1].Value

    $Commit = $env:GITHUB_SHA
    if ([string]::IsNullOrWhiteSpace($Commit)) {
        $Commit = (& git -C $Root rev-parse HEAD 2>$null)
        if ($LASTEXITCODE -ne 0) {
            $Commit = "unknown"
        }
    }

    @"
PHYSICS SANDBOX — PORTABLE WINDOWS x64

Version: $Version
Commit:  $Commit

QUICK START
1. Extract the entire ZIP to a folder you can write to.
2. Run "Physics Sandbox.cmd".
3. Keep the data, assets, mods, and worlds folders beside the executable.

This is a self-contained application bundle; no installer is used.
The package includes the Windows MSVC C runtime statically. Windows,
graphics drivers, and the GPU's supported graphics backend are still
provided by the host computer.

SAVES
World saves are written under worlds/<world>/state.json. Keep the
worlds folder writable and back it up when moving the portable folder.

COMMAND LINE
You can pass the same options as the executable, for example:
  Physics Sandbox.cmd --world worlds/sandbox/world.json

Do not move the executable out of this folder. The application resolves
its portable data and asset roots beside the executable.
"@ | Set-Content -LiteralPath (Join-Path $Dist "README-PORTABLE.txt") -Encoding utf8

    @"
@echo off
setlocal
cd /d "%~dp0"
if not exist "%~dp0physics_sandbox.exe" (
    echo Physics Sandbox executable was not found.
    echo Extract the complete portable package before launching.
    pause
    exit /b 1
)
"%~dp0physics_sandbox.exe" %*
exit /b %ERRORLEVEL%
"@ | Set-Content -LiteralPath (Join-Path $Dist "Physics Sandbox.cmd") -Encoding ascii

    $Files = Get-ChildItem -LiteralPath $Dist -Recurse -File
    if (-not ($Files | Where-Object { $_.Name -eq "physics_sandbox.exe" })) {
        throw "Portable package is missing physics_sandbox.exe"
    }
    foreach ($Directory in @("data", "assets", "mods", "worlds")) {
        if (-not (Test-Path -LiteralPath (Join-Path $Dist $Directory) -PathType Container)) {
            throw "Portable package is missing '$Directory'"
        }
    }

    Write-Host "==> Creating portable ZIP"
    Compress-Archive -Path (Join-Path $Dist "*") -DestinationPath $Archive -CompressionLevel Optimal -Force

    if (-not (Test-Path -LiteralPath $Archive -PathType Leaf)) {
        throw "Portable archive was not created: $Archive"
    }

    $ArchiveInfo = Get-Item -LiteralPath $Archive
    $ArchiveHash = (Get-FileHash -LiteralPath $Archive -Algorithm SHA256).Hash.ToLowerInvariant()
    "$ArchiveHash  $PackageName.zip" |
        Set-Content -LiteralPath "$Archive.sha256" -Encoding ascii

    Write-Host ("Portable folder: {0}" -f $Dist)
    Write-Host ("Portable ZIP:    {0} ({1:N1} MiB)" -f $Archive, ($ArchiveInfo.Length / 1MB))
    Write-Host ("SHA-256:         {0}" -f $ArchiveHash)
}
finally {
    Pop-Location
}
