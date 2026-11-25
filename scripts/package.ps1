$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$Dist = Join-Path $Root "dist/physics_sandbox"

Push-Location $Root
try {
    cargo build --release

    if (Test-Path $Dist) {
        Remove-Item $Dist -Recurse -Force
    }

    New-Item -ItemType Directory -Path $Dist -Force | Out-Null
    Copy-Item (Join-Path $Root "target/release/physics_sandbox.exe") $Dist
    foreach ($Directory in @("data", "assets", "mods", "worlds")) {
        Copy-Item (Join-Path $Root $Directory) (Join-Path $Dist $Directory) -Recurse -Force
    }

    Write-Host "Portable build created at $Dist"
}
finally {
    Pop-Location
}
