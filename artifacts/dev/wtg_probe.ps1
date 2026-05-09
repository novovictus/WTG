# wtg_probe.ps1
# Windows 10/11 probe harness (PowerShell 5.1 compatible)
# Assumes current working directory contains wtg.exe.
#
# Output directory:
#   results\probe_<hostname>_<card-model>_<driver-version>_<yyyyMMdd-HHmmss>\
#
# Output files:
#   wtg_probe_<hostname>_<card-model>_<driver-version>_<yyyyMMdd-HHmmss>.txt
#   wtg_probe_sink_jsonl_<hostname>_<card-model>_<driver-version>_<yyyyMMdd-HHmmss>.txt
#   wtg_probe_sink_<hostname>_<card-model>_<driver-version>_<yyyyMMdd-HHmmss>.jsonl
#   wtg_probe_sink_csv_<hostname>_<card-model>_<driver-version>_<yyyyMMdd-HHmmss>.txt
#   wtg_probe_sink_<hostname>_<card-model>_<driver-version>_<yyyyMMdd-HHmmss>.csv
#   wtg_probe_fields_1-255_<hostname>_<card-model>_<driver-version>_<yyyyMMdd-HHmmss>.txt
#
# Raw NVML field-ID scan:
#   Field scan calls wtg.exe once with repeated --field-id arguments rather than
#   using Invoke-WtgCapture, because unsupported field IDs can emit stderr or
#   non-success field results by design.
#   The full scan is captured as raw evidence in one file.
#
# Known issue:
#   WTG stderr note is captured as a PowerShell NativeCommandError block
#   Transcript contains:
#   wtg.exe : WTG note: sink enabled...
#   CategoryInfo...
#   FullyQualifiedErrorId...
# Reasoning:
#   If WTG writes a useful warning to stderr while still exiting 0, that warning will appear in the wrapper .txt.
#

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-GpuTokenFromName {
    param(
        [Parameter(Mandatory=$true)][string]$GpuName
    )

    $token = $GpuName.ToLower()
    $token = $token -replace "nvidia",""
    $token = $token -replace "geforce",""
    $token = $token -replace "gpu",""
    $token = $token -replace "\s+","-"
    $token = $token.Trim("-")
    return $token
}

function Invoke-WtgCapture {
    param(
        [Parameter(Mandatory=$true)][string]$Exe,
        [Parameter(Mandatory=$true)][string[]]$ArgumentList,
        [Parameter(Mandatory=$true)][string]$Path
    )

    $oldEap = $ErrorActionPreference
    $ErrorActionPreference = "Continue"

    try {
        & $Exe @ArgumentList 2>&1 |
            Out-File -FilePath $Path -Encoding utf8

        if ($LASTEXITCODE -ne 0) {
            throw "WTG exited with code $LASTEXITCODE for: $Exe $($ArgumentList -join ' ')"
        }
    } finally {
        $ErrorActionPreference = $oldEap
    }
}

$root   = (Get-Location).Path
$outDir = Join-Path $root "results"

New-Item -ItemType Directory -Force -Path $outDir | Out-Null

$hostname = $env:COMPUTERNAME
$tsTag    = (Get-Date).ToUniversalTime().ToString("yyyyMMdd-HHmmss")

$cmd = Get-Command "nvidia-smi" -ErrorAction SilentlyContinue

if ($cmd) {
    $nvsmi = $cmd.Source
} elseif (Test-Path "C:\Program Files\NVIDIA Corporation\NVSMI\nvidia-smi.exe") {
    $nvsmi = "C:\Program Files\NVIDIA Corporation\NVSMI\nvidia-smi.exe"
} else {
    $nvsmi = $null
}

$vc = Get-CimInstance -ClassName Win32_VideoController |
    Where-Object { $_.Name -match "NVIDIA" } |
    Select-Object -First 1

$gpuToken = if ($vc) {
    Get-GpuTokenFromName -GpuName $vc.Name
} else {
    "no-nvidia"
}

$driverVersion = "unknown"

if ($nvsmi) {
    $smiOut = & $nvsmi 2>&1
    $line = $smiOut |
        Select-String -Pattern "Driver Version" |
        Select-Object -First 1

    if ($line) {
        $tmp = ($line.ToString() -split "Driver Version:\s*")
        if ($tmp.Length -ge 2) {
            $driverVersion = (($tmp[1] -split "\s+")[0]).Trim()
        }
    }
}

$stem = "{0}_{1}_{2}_{3}" -f $hostname, $gpuToken, $driverVersion, $tsTag
$out  = Join-Path $outDir ("probe_{0}" -f $stem)

New-Item -ItemType Directory -Force -Path $out | Out-Null

$exe = Join-Path $root "wtg.exe"

$env:NO_COLOR = "1"
$env:CLICOLOR = "0"
$env:RUST_LOG_STYLE = "never"

# 1. Baseline probe
Invoke-WtgCapture `
    -Exe $exe `
    -ArgumentList @("--probe") `
    -Path (Join-Path $out ("wtg_probe_{0}.txt" -f $stem))

# 2. Probe with JSONL sink
Invoke-WtgCapture `
    -Exe $exe `
    -ArgumentList @("--probe", "--sink", "jsonl") `
    -Path (Join-Path $out ("wtg_probe_sink_jsonl_{0}.txt" -f $stem))

Get-ChildItem -Path $root -Filter "wtg_sink_*.jsonl" |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1 |
    Move-Item -Destination (Join-Path $out ("wtg_probe_sink_{0}.jsonl" -f $stem)) -Force

# 3. Probe with CSV sink
Invoke-WtgCapture `
    -Exe $exe `
    -ArgumentList @("--probe", "--sink", "csv") `
    -Path (Join-Path $out ("wtg_probe_sink_csv_{0}.txt" -f $stem))

Get-ChildItem -Path $root -Filter "wtg_sink_*.csv" |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1 |
    Move-Item -Destination (Join-Path $out ("wtg_probe_sink_{0}.csv" -f $stem)) -Force

# 4. Raw NVML field-ID scan - single WTG invocation, one NVML init
$fieldOut = Join-Path $out ("wtg_probe_fields_1-255_{0}.txt" -f $stem)

$fieldArgs = @("--probe-fields")
1..255 | ForEach-Object { $fieldArgs += "--field-id"; $fieldArgs += "$_" }

$oldEap = $ErrorActionPreference
$ErrorActionPreference = "Continue"

try {
    & $exe @fieldArgs 2>&1 | Out-File -FilePath $fieldOut -Encoding utf8
} finally {
    $ErrorActionPreference = $oldEap
}

Get-ChildItem $out