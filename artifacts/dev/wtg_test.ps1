# wtg_test.ps1
# Universal Windows 10/11 test harness (PowerShell 5.1 compatible)
# Assumes current working directory contains wtg.exe.

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Write-LogLine {
    param(
        [Parameter(Mandatory=$true)][string]$Path,
        [Parameter(Mandatory=$true)][string]$Line
    )
    $Line | Out-File -FilePath $Path -Append -Encoding utf8
}

$root   = (Get-Location).Path
$outDir = Join-Path $root "results"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null

$h = $env:COMPUTERNAME
$timestamp = (Get-Date -Format u)

# Resolve nvidia-smi
$nvsmi = $null
$cmd = Get-Command "nvidia-smi" -ErrorAction SilentlyContinue
if ($cmd) {
    $nvsmi = $cmd.Source
} elseif (Test-Path "C:\Program Files\NVIDIA Corporation\NVSMI\nvidia-smi.exe") {
    $nvsmi = "C:\Program Files\NVIDIA Corporation\NVSMI\nvidia-smi.exe"
}

# System identity (assume elevated access available)
$cs   = Get-CimInstance -ClassName Win32_ComputerSystem
$bios = Get-CimInstance -ClassName Win32_BIOS
$bb   = Get-CimInstance -ClassName Win32_BaseBoard

# GPU identity via CIM
$vc = Get-CimInstance -ClassName Win32_VideoController |
      Where-Object { $_.Name -match "NVIDIA" } |
      Select-Object -First 1

$gpuName = if ($vc) { $vc.Name } else { "N/A" }
$winDrv  = if ($vc) { $vc.DriverVersion } else { "N/A" }

# Run nvidia-smi (capture output + driver version)
$nvDrv = "unknown"
$smiOut = @()

if ($nvsmi) {
    $smiOut = & $nvsmi 2>&1
    $line = $smiOut | Select-String -Pattern "Driver Version" | Select-Object -First 1
    if ($line) {
        $tmp = ($line.ToString() -split "Driver Version:\s*")
        if ($tmp.Length -ge 2) {
            $nvDrv = (($tmp[1] -split "\s+")[0]).Trim()
        }
    }
} else {
    $smiOut = @("nvidia-smi not found (PATH or default NVSMI path)")
}

# Output file
$f = Join-Path $outDir ("wtg_env-{0}-drv{1}.txt" -f $h, $nvDrv)

# Keep logs uncolored if wtg uses colored output
$env:NO_COLOR = "1"
$env:CLICOLOR = "0"
$env:RUST_LOG_STYLE = "never"

# Header
"Host: $h" | Out-File -FilePath $f -Encoding utf8
Write-LogLine $f ("Timestamp: {0}" -f $timestamp)
Write-LogLine $f ("System: {0} {1}" -f $cs.Manufacturer, $cs.Model)
Write-LogLine $f ("BIOS Serial: {0}" -f $bios.SerialNumber)
Write-LogLine $f ("Baseboard: {0} {1}  Serial: {2}" -f $bb.Manufacturer, $bb.Product, $bb.SerialNumber)
Write-LogLine $f ("GPU (CIM): {0}" -f $gpuName)
Write-LogLine $f ("Windows DriverVersion (CIM): {0}" -f $winDrv)
Write-LogLine $f ("nvidia-smi path: {0}" -f $nvsmi)
Write-LogLine $f "----"

# NVIDIA-SMI section
Write-LogLine $f "---- NVIDIA-SMI ----"
$smiOut | Out-File -FilePath $f -Append -Encoding utf8
Write-LogLine $f "----"

# WTG section + validation
$wtg = Join-Path $root "wtg.exe"
Write-LogLine $f "---- WTG --once ----"

$pass = $true
$failReasons = New-Object System.Collections.Generic.List[string]

if (-not (Test-Path $wtg)) {
    $pass = $false
    $failReasons.Add("wtg.exe not found in current working directory: $root")
    Write-LogLine $f ("ERROR: {0}" -f $failReasons[$failReasons.Count-1])
} else {
    $wtgOut = & $wtg --once 2>&1
    $wtgExit = $LASTEXITCODE

    $wtgOut | Out-File -FilePath $f -Append -Encoding utf8

    if ($wtgExit -ne 0) {
        $pass = $false
        $failReasons.Add("wtg.exe returned non-zero exit code: $wtgExit")
    }

    # Validation: low-fuss tokens that should exist if stats emitted
    $joined = ($wtgOut -join "`n")
    $required = @(
        "GPU 0",
        "UUID",
        "Util",
        "VRAM",
        "Power"
    )

    foreach ($token in $required) {
        if ($joined -notmatch [regex]::Escape($token)) {
            $pass = $false
            $failReasons.Add("WTG output missing token: $token")
        }
    }
}

Write-LogLine $f "----"
Write-LogLine $f ("RESULT: {0}" -f ($(if ($pass) { "PASS" } else { "FAIL" })))

if (-not $pass) {
    Write-LogLine $f "FAIL_REASONS:"
    foreach ($r in $failReasons) { Write-LogLine $f ("- {0}" -f $r) }
}

Write-Host ("Wrote {0}" -f $f)
if ($pass) { exit 0 } else { exit 1 }
