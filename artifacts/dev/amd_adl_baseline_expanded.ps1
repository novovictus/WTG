cd C:\Users\plays\source\github_wtg\WTG

cargo build -p wtg-app --release
if ($LASTEXITCODE -ne 0) {
    throw "Release build failed."
}

$repo = (Resolve-Path ".").Path
$ts = Get-Date -Format "yyyyMMdd_HHmmss"
$run = ".\artifacts\validation\amd_adl_baseline_expanded_$env:COMPUTERNAME`_$ts"
New-Item -ItemType Directory -Force -Path $run | Out-Null
$run = (Resolve-Path $run).Path

$watchLog = Join-Path $run "amd_adl_watch.txt"
$watchErr = Join-Path $run "amd_adl_watch.err.txt"
$markerLog = Join-Path $run "phase_markers.txt"
$metaLog = Join-Path $run "run_metadata.txt"

$holdSeconds = 60
$countdownSeconds = 5

function Write-Utf8Line {
    param(
        [string]$Path,
        [string]$Text = ""
    )

    $Text | Out-File -FilePath $Path -Append -Encoding utf8
}

function Init-Utf8File {
    param(
        [string]$Path,
        [string[]]$Lines
    )

    $Lines | Out-File -FilePath $Path -Encoding utf8
}

function Add-Marker {
    param([string]$Text)

    Write-Utf8Line -Path $markerLog
    Write-Utf8Line -Path $markerLog -Text "=== $Text $(Get-Date -Format o) ==="
}

function Countdown-To-Go {
    param(
        [string]$Name,
        [string]$Action
    )

    Write-Host ""
    Write-Host "Queued phase: $Name"
    Write-Host "Action at GO: $Action"

    for ($i = $countdownSeconds; $i -gt 0; $i--) {
        Write-Host "$i..."
        Start-Sleep -Seconds 1
    }

    Add-Marker "PHASE: $Name | ACTION: $Action"
    Write-Host "GO - $Action"
    [console]::beep(1200,300)
}

function Hold-Phase {
    param([string]$Name)

    Write-Host "Holding $Name for $holdSeconds seconds..."
    for ($i = $holdSeconds; $i -gt 0; $i--) {
        Write-Host "  $i sec remaining"
        Start-Sleep -Seconds 1
    }
}

Init-Utf8File -Path $metaLog -Lines @(
    "WTG AMD ADL expanded baseline run metadata",
    "Host: $env:COMPUTERNAME",
    "Start: $(Get-Date -Format o)",
    "Run folder: $run",
    "Repo: $repo",
    "Countdown seconds: $countdownSeconds",
    "Hold seconds per phase: $holdSeconds",
    "Phase order:",
    "  1. USB-C REST - no load",
    "  2. USB-C LOAD - start load",
    "  3. USB-C LOAD CONFIRM - keep USB-C/load steady",
    "  4. BATTERY LOAD - remove USB-C",
    "  5. BARREL AC LOAD - connect OEM AC",
    "  6. BARREL AC COOLDOWN - stop load, stay on OEM AC",
    "Command: .\target\release\wtg.exe --provider amd --watch --interval 1000",
    ""
)

Init-Utf8File -Path $watchLog -Lines @(
    "WTG AMD ADL expanded baseline watch",
    "Host: $env:COMPUTERNAME",
    "Start: $(Get-Date -Format o)",
    "Phase order: USB-C rest -> USB-C load -> USB-C load confirm -> battery load -> barrel AC load -> barrel AC cooldown",
    ""
)

Init-Utf8File -Path $watchErr -Lines @(
    "WTG AMD ADL expanded baseline watch stderr",
    "Host: $env:COMPUTERNAME",
    "Start: $(Get-Date -Format o)",
    ""
)

Init-Utf8File -Path $markerLog -Lines @(
    "WTG AMD ADL expanded baseline markers",
    "Host: $env:COMPUTERNAME",
    "Start: $(Get-Date -Format o)",
    "Run folder: $run",
    ""
)

Write-Host ""
Write-Host "Run folder:"
Write-Host $run
Write-Host ""

$watchCmd = @"
@echo off
cd /d "$repo"
echo WTG AMD ADL expanded baseline watch window
echo Logging to:
echo $watchLog
echo.
.\target\release\wtg.exe --provider amd --watch --interval 1000 >> "$watchLog" 2>> "$watchErr"
"@

$watchCmdPath = Join-Path $run "run_adl_watch.cmd"
$watchCmd | Out-File -FilePath $watchCmdPath -Encoding ascii

Write-Host "Opening AMD ADL watch window..."

$watchWindow = Start-Process `
    -FilePath "cmd.exe" `
    -ArgumentList @("/c", "`"$watchCmdPath`"") `
    -PassThru

Start-Sleep -Seconds 3
Add-Marker "WATCH WINDOW STARTED pid=$($watchWindow.Id)"

try {
    Write-Host ""
    Write-Host "Initial setup:"
    Write-Host "  USB-C connected"
    Write-Host "  No stress load running"
    Read-Host "Press Enter to begin queued 6-phase sequence"

    Countdown-To-Go "USB-C REST - NO LOAD" "Keep USB-C connected; keep load OFF"
    Hold-Phase "USB-C REST - NO LOAD"

    Countdown-To-Go "USB-C LOAD - START LOAD" "Start stress load; keep USB-C connected"
    Hold-Phase "USB-C LOAD - START LOAD"

    Countdown-To-Go "USB-C LOAD CONFIRM - STEADY STATE" "Keep stress load running; keep USB-C connected"
    Hold-Phase "USB-C LOAD CONFIRM - STEADY STATE"

    Countdown-To-Go "BATTERY LOAD - REMOVE USB-C" "Remove USB-C; keep stress load running"
    Hold-Phase "BATTERY LOAD - REMOVE USB-C"

    Countdown-To-Go "BARREL AC LOAD - CONNECT OEM AC" "Connect OEM barrel AC; keep stress load running"
    Hold-Phase "BARREL AC LOAD - CONNECT OEM AC"

    Countdown-To-Go "BARREL AC COOLDOWN - STOP LOAD" "Stop stress load; stay on OEM barrel AC"
    Hold-Phase "BARREL AC COOLDOWN - STOP LOAD"

    Add-Marker "END REQUESTED"
}
finally {
    Write-Host ""
    Write-Host "Closing AMD ADL watch window..."

    Add-Marker "WATCH WINDOW CLOSE REQUESTED"

    if ($watchWindow -and -not $watchWindow.HasExited) {
        taskkill /PID $watchWindow.Id /T /F | Out-Null
        Start-Sleep -Seconds 1
    }

    Add-Marker "WATCH WINDOW CLOSED"

    Write-Utf8Line -Path $metaLog -Text "End: $(Get-Date -Format o)"
    Write-Utf8Line -Path $metaLog -Text "Watch window exit observed: $($watchWindow.HasExited)"

    Write-Host ""
    Write-Host "Capture complete:"
    Write-Host $run
    Write-Host ""
    Write-Host "Files:"
    Get-ChildItem $run | Select-Object Name, Length, LastWriteTime
}
