param(
    [string]$Executable
)

$ErrorActionPreference = "Stop"
$projectRoot = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($Executable)) {
    $Executable = Join-Path $projectRoot "KnightFrame-Test.exe"
}
$Executable = [System.IO.Path]::GetFullPath($Executable)
if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
    throw "Test executable does not exist: $Executable"
}

$listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::IPv6Any, 1420)
$listener.Server.DualMode = $true
$process = $null
try {
    # Occupying the dev port turns any accidental devUrl dependency into a visible failure.
    $listener.Start()
    $process = Start-Process -FilePath $Executable -WorkingDirectory $projectRoot -WindowStyle Hidden -PassThru
    $windowReady = $false
    $attemptedDevUrl = $false

    for ($attempt = 0; $attempt -lt 80; $attempt++) {
        Start-Sleep -Milliseconds 250
        $process.Refresh()
        if ($process.HasExited) {
            break
        }
        if ($listener.Pending()) {
            $attemptedDevUrl = $true
            break
        }
        if ($process.MainWindowHandle -ne 0) {
            $windowReady = $true
            break
        }
    }

    if ($attemptedDevUrl) {
        throw "Executable attempted to connect to localhost:1420 instead of using embedded assets"
    }
    if ($process.HasExited) {
        throw "Executable exited during startup with code $($process.ExitCode)"
    }
    if (-not $windowReady) {
        throw "Executable did not create its main window within 20 seconds"
    }

    Start-Sleep -Milliseconds 750
    $process.Refresh()
    if ($listener.Pending()) {
        throw "Executable attempted a delayed connection to localhost:1420"
    }
    if ($process.HasExited) {
        throw "Executable exited immediately after creating its main window"
    }

    [pscustomobject]@{
        Executable = $Executable
        MainWindowReady = $true
        ProcessStayedAlive = $true
        AttemptedLocalhost1420 = $false
    } | Format-List
} finally {
    $listener.Stop()
    if ($null -ne $process -and -not $process.HasExited) {
        [void]$process.CloseMainWindow()
        if (-not $process.WaitForExit(3000)) {
            Stop-Process -Id $process.Id -Force
        }
    }
}
