$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$Dist = Join-Path $Root "dist/physics_sandbox"

Push-Location $Root
try {
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build --release --locked failed with exit code $LASTEXITCODE"
    }

    $Executable = Join-Path $Root "target/release/physics_sandbox.exe"
    if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
        throw "Release executable was not produced: $Executable"
    }

    if (Test-Path $Dist) {
        Remove-Item $Dist -Recurse -Force
    }

    New-Item -ItemType Directory -Path $Dist -Force | Out-Null
    Copy-Item -LiteralPath $Executable -Destination $Dist
    foreach ($Directory in @("data", "assets", "mods", "worlds")) {
        $Source = Join-Path $Root $Directory
        if (-not (Test-Path -LiteralPath $Source -PathType Container)) {
            throw "Required portable content directory is missing: $Source"
        }
        Copy-Item $Source (Join-Path $Dist $Directory) -Recurse -Force
    }

    Write-Host "Portable build created at $Dist"
}
finally {
    Pop-Location
}
