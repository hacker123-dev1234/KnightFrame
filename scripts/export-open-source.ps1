param([string]$Destination, [switch]$Refresh)

$ErrorActionPreference = "Stop"
$projectRoot = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($Destination)) { $Destination = Join-Path (Split-Path -Parent $projectRoot) "KnightFrame-OpenSource" }
$destinationPath = [System.IO.Path]::GetFullPath($Destination)
$parentPath = [System.IO.Path]::GetFullPath((Split-Path -Parent $projectRoot))
if (-not $destinationPath.StartsWith($parentPath, [System.StringComparison]::OrdinalIgnoreCase)) { throw "Destination must stay inside $parentPath" }
if (Test-Path -LiteralPath $destinationPath) {
    if (-not $Refresh) { throw "Destination already exists: $destinationPath (use -Refresh to update it)" }
} else {
    New-Item -ItemType Directory -Path $destinationPath | Out-Null
}
$files = @('package.json','pnpm-lock.yaml','pnpm-workspace.yaml','tsconfig.json','vite.config.ts','index.html','studio.html','README.md','README.en.md','LICENSE','NOTICE','CONTRIBUTING.md','SECURITY.md','.gitignore')
foreach ($file in $files) {
    $source = Join-Path $projectRoot $file
    if (Test-Path -LiteralPath $source -PathType Leaf) { Copy-Item -LiteralPath $source -Destination (Join-Path $destinationPath $file) -Force }
}

$directories = @('src','src-tauri','public','assets','docs','scripts')
foreach ($directory in $directories) {
    $source = Join-Path $projectRoot $directory
    $target = Join-Path $destinationPath $directory
    & robocopy $source $target /E /NFL /NDL /NJH /NJS /NP /XD target node_modules dist gen __pycache__ .bench artifacts validation /XF *.tmp *.log *.pdb *.ilk
    if ($LASTEXITCODE -gt 7) { throw "Failed to export $directory (robocopy $LASTEXITCODE)" }
}

$release = Join-Path $projectRoot 'release'
if (Test-Path -LiteralPath $release -PathType Container) {
    & robocopy $release (Join-Path $destinationPath 'release') /E /NFL /NDL /NJH /NJS /NP
    if ($LASTEXITCODE -gt 7) { throw "Failed to export release artifacts" }
}

@{
    source = 'knightframe-rs'
    version = (Get-Content -Raw -LiteralPath (Join-Path $projectRoot 'package.json') | ConvertFrom-Json).version
    generatedAt = [DateTimeOffset]::Now.ToString('o')
    excludes = @('credentials','node_modules','target','dist','temporary files','diagnostics','reference projects')
} | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $destinationPath 'OPEN_SOURCE_EXPORT.json') -Encoding UTF8

Write-Output $destinationPath
