# NVIDIA bug 6162407 Windows diagnostic collector
# Run from the folder containing:
#   - wtg_test.ps1
#   - wtg.exe
#
# Behavior:
#   1. Clears stale NVML debug env vars before running wtg_test.ps1.
#   2. Runs .\wtg_test.ps1 exactly once.
#   3. Waits for the generated .\results\wtg_<host>_<gpu>_<driver>_<timestamp>.txt file.
#   4. Parses the NVIDIA driver version from that filename.
#   5. Creates nvidia_bug_6162407_driver_<driver>.
#   6. Copies WTG artifacts, Windows diagnostics, NVIDIA-SMI outputs, event context.
#   7. Enables NVML debug logging after the output folder exists.
#   8. Collects nvml_debug.log as generated.
#   9. Runs an extra .\wtg.exe --probe --sink jsonl at the end.
#   10. Copies the generated wtg_sink_*.jsonl artifact.
#   11. Writes one concise collection_manifest.txt.
#   12. Zips the final bundle.

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$ScriptStartTime = Get-Date
$Cwd = Get-Location
$ResultsDir = Join-Path $Cwd "results"

if (-not (Test-Path ".\wtg_test.ps1")) {
    throw "Missing .\wtg_test.ps1 in current directory: $Cwd"
}

if (-not (Test-Path ".\wtg.exe")) {
    throw "Missing .\wtg.exe in current directory: $Cwd"
}

# Clear stale NVML debug variables before wtg_test.ps1.
# The output folder does not exist yet, so a stale __NVML_DBG_FILE path can break nvidia-smi.
Remove-Item Env:\__NVML_DBG_FILE -ErrorAction SilentlyContinue
Remove-Item Env:\__NVML_DBG_APPEND -ErrorAction SilentlyContinue
Remove-Item Env:\__NVML_DBG_LVL -ErrorAction SilentlyContinue

# Record existing WTG result files so we can identify the new one without ambiguity.
$ExistingResultFiles = @{}
if (Test-Path $ResultsDir) {
    Get-ChildItem $ResultsDir -File -Filter "wtg_*.txt" | ForEach-Object {
        $ExistingResultFiles[$_.FullName] = $true
    }
}

Write-Host "Running wtg_test.ps1 once..."

# wtg_test.ps1 may call nvidia-smi, which can write warnings to stderr.
# Do not let that abort the whole collector.
$oldErrorActionPreference = $ErrorActionPreference
$ErrorActionPreference = "Continue"

try {
    & .\wtg_test.ps1
    $wtgTestExitCode = $LASTEXITCODE
}
finally {
    $ErrorActionPreference = $oldErrorActionPreference
}

if ($null -ne $wtgTestExitCode -and $wtgTestExitCode -ne 0) {
    Write-Host "wtg_test.ps1 returned exit code $wtgTestExitCode. Continuing to search for generated result file..."
}

# Wait for the newly generated WTG result file.
$wtgResultPath = $null
$deadline = (Get-Date).AddSeconds(45)

do {
    if (Test-Path $ResultsDir) {
        $candidate = Get-ChildItem $ResultsDir -File -Filter "wtg_*.txt" |
            Where-Object {
                -not $ExistingResultFiles.ContainsKey($_.FullName) -and
                $_.LastWriteTime -ge $ScriptStartTime.AddMinutes(-2)
            } |
            Sort-Object LastWriteTime -Descending |
            Select-Object -First 1

        if ($candidate) {
            $wtgResultPath = $candidate.FullName
            break
        }
    }

    Start-Sleep -Milliseconds 500
} while ((Get-Date) -lt $deadline)

# Fallback: choose newest WTG result written near this script run.
if (-not $wtgResultPath) {
    if (Test-Path $ResultsDir) {
        $fallbackCandidate = Get-ChildItem $ResultsDir -File -Filter "wtg_*.txt" |
            Where-Object { $_.LastWriteTime -ge $ScriptStartTime.AddMinutes(-2) } |
            Sort-Object LastWriteTime -Descending |
            Select-Object -First 1

        if ($fallbackCandidate) {
            $wtgResultPath = $fallbackCandidate.FullName
        }
    }
}

if (-not $wtgResultPath -or -not (Test-Path $wtgResultPath)) {
    throw "Could not locate new WTG result file in $ResultsDir after running wtg_test.ps1."
}

$wtgResultFile = Split-Path -Leaf $wtgResultPath

# Parse driver version from WTG filename.
# Expected:
#   wtg_<hostname>_<gpu-token>_<driver-version>_<yyyyMMdd-HHmmss>.txt
$driverVersion = "unknown"
if ($wtgResultFile -match "_(?<driver>[0-9]+\.[0-9]+|unknown)_[0-9]{8}-[0-9]{6}\.txt$") {
    $driverVersion = $Matches.driver
}

$driverToken = $driverVersion -replace "\.", "_"
$Tag = "driver_$driverToken"

$Out = Join-Path $Cwd "nvidia_bug_6162407_$Tag"
New-Item -ItemType Directory -Force -Path $Out | Out-Null

# Copy the exact WTG test result into the diagnostic bundle.
Copy-Item -Path $wtgResultPath -Destination $Out -Force

# Copy recent WTG artifacts from .\results for this run.
# This catches the main results file plus any companion artifacts wtg_test.ps1 may create.
if (Test-Path $ResultsDir) {
    Get-ChildItem $ResultsDir -File |
        Where-Object {
            $_.FullName -eq $wtgResultPath -or
            $_.LastWriteTime -ge $ScriptStartTime.AddMinutes(-2) -or
            $_.Name -like "wtg_*_$driverVersion`_*"
        } |
        Copy-Item -Destination $Out -Force
}

# Enable NVML debug logging for subsequent NVIDIA-SMI / NVML calls.
# This intentionally happens after $Out exists.
$NvmlDebugLog = Join-Path $Out "nvml_debug.log"
Remove-Item $NvmlDebugLog -ErrorAction SilentlyContinue

$env:__NVML_DBG_FILE = $NvmlDebugLog
$env:__NVML_DBG_APPEND = "1"
$env:__NVML_DBG_LVL = "DEBUG"

# System identity and OS details
Write-Host "Collecting Windows system diagnostics..."
Get-ComputerInfo | Out-File "$Out\computer_info.txt" -Encoding utf8
systeminfo | Out-File "$Out\systeminfo.txt" -Encoding utf8
dxdiag /t "$Out\dxdiag.txt"

# msinfo32 can be slow. Start it explicitly and wait up to 5 minutes.
$msinfoPath = "$Out\msinfo.nfo"
$msinfoProcess = Start-Process -FilePath "msinfo32.exe" -ArgumentList "/nfo `"$msinfoPath`"" -PassThru

if (-not $msinfoProcess.WaitForExit(300000)) {
    Stop-Process -Id $msinfoProcess.Id -Force
    "msinfo32 timed out after 300 seconds." | Out-File "$Out\msinfo_timeout.txt" -Encoding utf8
}

# Hardware / platform details
Get-CimInstance Win32_BIOS | Format-List * | Out-File "$Out\bios.txt" -Encoding utf8
Get-CimInstance Win32_ComputerSystem | Format-List * | Out-File "$Out\computer_system.txt" -Encoding utf8
Get-CimInstance Win32_BaseBoard | Format-List * | Out-File "$Out\baseboard.txt" -Encoding utf8
Get-CimInstance Win32_VideoController | Format-List * | Out-File "$Out\video_controller.txt" -Encoding utf8

# Driver and device inventory
pnputil /enum-devices /class Display /connected > "$Out\pnputil_display_devices.txt"
pnputil /enum-drivers > "$Out\pnputil_drivers.txt"

# NVIDIA process/service context.
# Service query intentionally avoids broad "nv" matching because it catches unrelated Windows services.
Get-Service | Where-Object {
    $_.DisplayName -match "NVIDIA" -or
    $_.Name -match "^Nv|^NV|NVIDIA"
} |
    Sort-Object Name |
    Select-Object Name, DisplayName, Status, StartType, ServiceType, CanStop, CanPauseAndContinue |
    Format-List * |
    Out-File "$Out\nvidia_services.txt" -Encoding utf8

# CIM process query gives better path/command-line context than Get-Process.
Get-CimInstance Win32_Process | Where-Object {
    $_.Name -match "nvidia|nvcontainer|nvdisplay|nvsphelper|nvcpl|nvtelemetry"
} |
    Sort-Object Name |
    Select-Object ProcessId, Name, ExecutablePath, CommandLine |
    Format-List * |
    Out-File "$Out\nvidia_processes.txt" -Encoding utf8

# NVIDIA-SMI diagnostics
Write-Host "Collecting NVIDIA-SMI diagnostics..."
nvidia-smi -q > "$Out\nvidia_smi_q.txt"
nvidia-smi -q -x > "$Out\nvidia_smi_q.xml"
nvidia-smi --query-gpu=name,driver_version,vbios_version,pci.bus_id,pstate,power.draw,power.limit,utilization.gpu,utilization.memory,memory.used,memory.total,temperature.gpu --format=csv > "$Out\nvidia_smi_idle.csv"

# Windows display/NVIDIA-related event context
Get-WinEvent -LogName System -MaxEvents 2000 | Where-Object {
    $_.ProviderName -match "NVIDIA|Display|Kernel-PnP|nvlddmkm"
} |
    Format-List * |
    Out-File "$Out\system_display_nvidia_events.txt" -Encoding utf8

# Extra WTG JSONL probe for good measure.
# WTG writes the useful probe record to wtg_sink_*.jsonl in the current directory.
# stdout/stderr helper files are intentionally omitted.
Write-Host "Running extra WTG JSONL probe..."
$ProbeStartTime = Get-Date

# Use cmd.exe wrapper so PowerShell does not turn harmless native stderr into NativeCommandError.
cmd /c ".\wtg.exe --probe --sink jsonl 1>NUL 2>NUL"
$wtgProbeExitCode = $LASTEXITCODE

# Copy generated JSONL sink file(s) from this probe run.
$probeSinkFiles = Get-ChildItem "." -File -Filter "wtg_sink_*.jsonl" |
    Where-Object { $_.LastWriteTime -ge $ProbeStartTime.AddMinutes(-1) } |
    Sort-Object LastWriteTime -Descending

if ($probeSinkFiles) {
    $probeSinkFiles | Copy-Item -Destination $Out -Force
}
else {
    "No wtg_sink_*.jsonl file found after extra WTG probe. WTG probe exit code: $wtgProbeExitCode" |
        Out-File "$Out\wtg_probe_jsonl_missing_sink.txt" -Encoding utf8
}

# Clear NVML debug env vars so they do not leak into later shell sessions.
Remove-Item Env:\__NVML_DBG_FILE -ErrorAction SilentlyContinue
Remove-Item Env:\__NVML_DBG_APPEND -ErrorAction SilentlyContinue
Remove-Item Env:\__NVML_DBG_LVL -ErrorAction SilentlyContinue

# Write a short note for NVIDIA about the NVML debug log format.
if (Test-Path $NvmlDebugLog) {
    $NvmlDebugInfo = Get-Item $NvmlDebugLog
    @"
NVML debug log generated via:
  __NVML_DBG_FILE=$NvmlDebugLog
  __NVML_DBG_APPEND=1
  __NVML_DBG_LVL=DEBUG

The file is included as generated.

Observed note:
On Windows, this file does not appear to be human-readable plain text in a standard text viewer. It may be binary or otherwise encoded. It is included for NVIDIA analysis.

File:
  Name: $($NvmlDebugInfo.Name)
  Length: $($NvmlDebugInfo.Length)
  LastWriteTime: $($NvmlDebugInfo.LastWriteTime)
"@ | Out-File "$Out\nvml_debug_log_note.txt" -Encoding utf8
}
else {
    @"
NVML debug log was requested via:
  __NVML_DBG_FILE=$NvmlDebugLog
  __NVML_DBG_APPEND=1
  __NVML_DBG_LVL=DEBUG

No nvml_debug.log file was generated.
"@ | Out-File "$Out\nvml_debug_log_note.txt" -Encoding utf8
}

# Single concise manifest, written after collection so it includes final state.
$CollectionEndTime = Get-Date
$ZipPath = "$Out.zip"

$filesForManifest = Get-ChildItem $Out -File |
    Sort-Object Name |
    Select-Object Name, Length, LastWriteTime

@"
NVIDIA bug: 6162407
Collection start time: $ScriptStartTime
Collection end time: $CollectionEndTime

Working directory: $Cwd
Diagnostic folder: $Out
Archive path: $ZipPath

WTG result selected: $wtgResultPath
WTG result file: $wtgResultFile
Detected NVIDIA driver version: $driverVersion
Diagnostic tag: $Tag
wtg_test.ps1 exit code: $wtgTestExitCode
extra wtg probe exit code: $wtgProbeExitCode

NVML debug log:
Collected as nvml_debug.log using __NVML_DBG_FILE / __NVML_DBG_APPEND / __NVML_DBG_LVL.
On Windows, this file does not appear to be human-readable plain text in a standard text viewer. It is included as generated for NVIDIA analysis.
See nvml_debug_log_note.txt.

Files:
$($filesForManifest | Format-Table -AutoSize | Out-String)
"@ | Out-File "$Out\collection_manifest.txt" -Encoding utf8

# Final package
if (Test-Path $ZipPath) {
    Remove-Item $ZipPath -Force
}

Compress-Archive -Path "$Out\*" -DestinationPath $ZipPath -Force

Write-Host ""
Write-Host "Complete."
Write-Host "WTG result: $wtgResultPath"
Write-Host "Detected NVIDIA driver version: $driverVersion"
Write-Host "Diagnostic folder: $Out"
Write-Host "Diagnostic archive: $ZipPath"
