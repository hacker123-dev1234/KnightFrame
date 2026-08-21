param(
    [switch]$Smoke
)

$ErrorActionPreference = "Stop"
$projectRoot = Split-Path -Parent $PSScriptRoot
$sourceExe = Join-Path $projectRoot "src-tauri\target\release\knightframe.exe"
$publishedExe = Join-Path $projectRoot "KnightFrame-Test.exe"
$stagingExe = Join-Path $projectRoot "KnightFrame-Test.exe.next"
$backupExe = Join-Path $projectRoot "KnightFrame-Test.exe.previous"

function Get-Sha256([string]$Path) {
    $stream = [System.IO.File]::OpenRead($Path)
    try {
        $sha256 = [System.Security.Cryptography.SHA256]::Create()
        try {
            return [System.BitConverter]::ToString($sha256.ComputeHash($stream)).Replace("-", "")
        } finally {
            $sha256.Dispose()
        }
    } finally {
        $stream.Dispose()
    }
}

Push-Location $projectRoot
try {
    if (Test-Path -LiteralPath $backupExe -PathType Leaf) {
        if (Test-Path -LiteralPath $publishedExe -PathType Leaf) {
            try {
                Remove-Item -LiteralPath $backupExe -Force -ErrorAction Stop
            } catch {
                # Antivirus scanners can briefly retain a just-renamed EXE.
                # Use a unique rollback path instead of failing a valid build.
                $backupExe = "$backupExe.$([Guid]::NewGuid().ToString('N'))"
            }
        } else {
            [System.IO.File]::Move($backupExe, $publishedExe)
        }
    }

    & pnpm exec tauri build --no-bundle --ci --features custom-protocol
    if ($LASTEXITCODE -ne 0) {
        throw "Tauri test executable build failed with exit code $LASTEXITCODE"
    }

    if (-not (Test-Path -LiteralPath $sourceExe -PathType Leaf)) {
        throw "Tauri did not produce the expected executable: $sourceExe"
    }

    Copy-Item -LiteralPath $sourceExe -Destination $stagingExe -Force
    $sourceHash = Get-Sha256 $sourceExe
    $stagingHash = Get-Sha256 $stagingExe
    if ($sourceHash -ne $stagingHash) {
        throw "Staged executable hash does not match the Tauri output"
    }

    $hadPrevious = Test-Path -LiteralPath $publishedExe -PathType Leaf
    if ($hadPrevious) {
        [System.IO.File]::Move($publishedExe, $backupExe)
    }
    try {
        [System.IO.File]::Move($stagingExe, $publishedExe)
        if ((Get-Sha256 $publishedExe) -ne $sourceHash) {
            throw "Published executable hash does not match the Tauri output"
        }
        if ($hadPrevious) {
            try {
                Remove-Item -LiteralPath $backupExe -Force -ErrorAction Stop
            } catch {
                # Publishing already succeeded and hashes match. A stale
                # rollback copy is harmless and can be removed on a later run.
            }
        }
    } catch {
        if (Test-Path -LiteralPath $publishedExe -PathType Leaf) {
            Remove-Item -LiteralPath $publishedExe -Force
        }
        if ($hadPrevious -and (Test-Path -LiteralPath $backupExe -PathType Leaf)) {
            [System.IO.File]::Move($backupExe, $publishedExe)
        }
        throw
    }
    $published = Get-Item -LiteralPath $publishedExe
    [pscustomobject]@{
        Executable = $published.FullName
        Bytes = $published.Length
        Sha256 = $sourceHash
        AssetMode = "embedded-custom-protocol"
        InstallerGenerated = $false
    } | Format-List

    if ($Smoke) {
        & powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "smoke-test-exe.ps1")
        if ($LASTEXITCODE -ne 0) {
            throw "Standalone executable smoke test failed with exit code $LASTEXITCODE"
        }
    }
} finally {
    if (Test-Path -LiteralPath $stagingExe -PathType Leaf) {
        Remove-Item -LiteralPath $stagingExe -Force
    }
    Pop-Location
}
