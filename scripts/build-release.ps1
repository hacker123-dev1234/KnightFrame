param([switch]$SkipTests, [switch]$SkipInstallers)

$ErrorActionPreference = "Stop"
$projectRoot = Split-Path -Parent $PSScriptRoot
$releaseRoot = Join-Path $projectRoot "release"

Push-Location $projectRoot
try {
    if (-not $SkipTests) {
        & pnpm check
        if ($LASTEXITCODE -ne 0) { throw "Frontend checks failed" }
        & cargo test --manifest-path src-tauri/Cargo.toml --features custom-protocol
        if ($LASTEXITCODE -ne 0) { throw "Rust tests failed" }
    }

    & pnpm exec tauri build --ci --features custom-protocol --no-bundle
    if ($LASTEXITCODE -ne 0) { throw "Tauri release build failed" }

    New-Item -ItemType Directory -Path $releaseRoot -Force | Out-Null
    $portable = Join-Path $projectRoot "src-tauri\target\release\knightframe.exe"
    if (-not (Test-Path -LiteralPath $portable -PathType Leaf)) { throw "Portable executable was not produced" }
    Copy-Item -LiteralPath $portable -Destination (Join-Path $releaseRoot "KnightFrame-Portable.exe") -Force

    if (-not $SkipInstallers) {
        & pnpm exec tauri build --ci --features custom-protocol --bundles 'msi,nsis'
        if ($LASTEXITCODE -ne 0) {
            Write-Warning "Installer tooling was unavailable; the portable executable is still ready. Re-run later to create MSI/NSIS installers."
        }
    }

    $bundleRoot = Join-Path $projectRoot "src-tauri\target\release\bundle"
    if (Test-Path -LiteralPath $bundleRoot) {
        $bundles = Get-ChildItem -LiteralPath $bundleRoot -Recurse -File |
            Where-Object { $_.Extension -in '.msi', '.exe' }
        foreach ($bundle in $bundles) { Copy-Item -LiteralPath $bundle.FullName -Destination (Join-Path $releaseRoot $bundle.Name) -Force }
    }

    Get-ChildItem -LiteralPath $releaseRoot -File | Select-Object Name, Length, @{Name='Sha256';Expression={(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash}} | Format-Table -AutoSize
} finally { Pop-Location }
