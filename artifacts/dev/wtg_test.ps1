# wtg_test.ps1
# Windows 10/11 test harness (PowerShell 5.1 compatible)
# Assumes current working directory contains wtg.exe.
#
# Output file (results\):
#   wtg_<hostname>_<card-model>_<driver-version>_<yyyyMMdd-HHmmss>.txt

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-GpuTokenFromName {
    param(
        [Parameter(Mandatory=$true)][string]$GpuName
    )

    # Human-facing token:
    # - drop "nvidia", "geforce", "gpu"
    # - lowercase
    # - spaces -> dashes
    $token = $GpuName.ToLower()
    $token = $token -replace "nvidia",""
    $token = $token -replace "geforce",""
    $token = $token -replace "gpu",""
    $token = $token -replace "\s+","-"
    $token = $token.Trim("-")
    return $token
}

$root   = (Get-Location).Path
$outDir = Join-Path $root "results"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null

$hostname  = $env:COMPUTERNAME
$timestamp = (Get-Date -Format u)
$tsTag     = (Get-Date).ToUniversalTime().ToString("yyyyMMdd-HHmmss")

# Resolve nvidia-smi
$nvsmi = $null
$cmd = Get-Command "nvidia-smi" -ErrorAction SilentlyContinue
if ($cmd) {
    $nvsmi = $cmd.Source
} elseif (Test-Path "C:\Program Files\NVIDIA Corporation\NVSMI\nvidia-smi.exe") {
    $nvsmi = "C:\Program Files\NVIDIA Corporation\NVSMI\nvidia-smi.exe"
}

# System identity
$cs   = Get-CimInstance -ClassName Win32_ComputerSystem
$bios = Get-CimInstance -ClassName Win32_BIOS
$bb   = Get-CimInstance -ClassName Win32_BaseBoard

# GPU identity (CIM) - query once
$vc = Get-CimInstance -ClassName Win32_VideoController |
      Where-Object { $_.Name -match "NVIDIA" } |
      Select-Object -First 1

$gpuName = if ($vc) { $vc.Name } else { "N/A" }
$winDrv  = if ($vc) { $vc.DriverVersion } else { "N/A" }
$gpuToken = if ($vc) { Get-GpuTokenFromName -GpuName $vc.Name } else { "no-nvidia" }

# Run nvidia-smi (to get NVIDIA driver version)
$driverVersion = "unknown"
$smiOut = @()

if ($nvsmi) {
    $smiOut = & $nvsmi 2>&1
    $line = $smiOut | Select-String -Pattern "Driver Version" | Select-Object -First 1
    if ($line) {
        $tmp = ($line.ToString() -split "Driver Version:\s*")
        if ($tmp.Length -ge 2) {
            $driverVersion = (($tmp[1] -split "\s+")[0]).Trim()
        }
    }
} else {
    $smiOut = @("nvidia-smi not found (PATH or default NVSMI path)")
}

# Output file
$f = Join-Path $outDir ("wtg_{0}_{1}_{2}_{3}.txt" -f `
    $hostname,
    $gpuToken,
    $driverVersion,
    $tsTag)

# Disable colored output (belt + suspenders)
$env:NO_COLOR = "1"
$env:CLICOLOR = "0"
$env:RUST_LOG_STYLE = "never"

# Write header (Set-Content creates/overwrites; Add-Content appends)
Set-Content -Path $f -Value ("Host: {0}" -f $hostname) -Encoding utf8
Add-Content -Path $f -Value ("Timestamp: {0}" -f $timestamp) -Encoding utf8
Add-Content -Path $f -Value ("System: {0} {1}" -f $cs.Manufacturer, $cs.Model) -Encoding utf8
Add-Content -Path $f -Value ("BIOS Serial: {0}" -f $bios.SerialNumber) -Encoding utf8
Add-Content -Path $f -Value ("Baseboard: {0} {1}  Serial: {2}" -f $bb.Manufacturer, $bb.Product, $bb.SerialNumber) -Encoding utf8
Add-Content -Path $f -Value ("GPU (CIM): {0}" -f $gpuName) -Encoding utf8
Add-Content -Path $f -Value ("Windows DriverVersion (CIM): {0}" -f $winDrv) -Encoding utf8
Add-Content -Path $f -Value ("nvidia-smi path: {0}" -f $nvsmi) -Encoding utf8
Add-Content -Path $f -Value "----" -Encoding utf8

# NVIDIA-SMI section
Add-Content -Path $f -Value "---- NVIDIA-SMI ----" -Encoding utf8
if ($smiOut) {
    $smiOut | Add-Content -Path $f -Encoding utf8
}
Add-Content -Path $f -Value "----" -Encoding utf8

# WTG section + validation
$wtg = Join-Path $root "wtg.exe"
Add-Content -Path $f -Value "---- WTG --once ----" -Encoding utf8

$pass = $true
$failReasons = New-Object System.Collections.Generic.List[string]

if (-not (Test-Path $wtg)) {
    $pass = $false
    $failReasons.Add("wtg.exe not found in current working directory: $root")
    Add-Content -Path $f -Value ("ERROR: {0}" -f $failReasons[$failReasons.Count-1]) -Encoding utf8
} else {
    $wtgOut = & $wtg --once 2>&1
    $wtgExit = $LASTEXITCODE

    if ($wtgOut) { $wtgOut | Add-Content -Path $f -Encoding utf8 }

    if ($wtgExit -ne 0) {
        $pass = $false
        $failReasons.Add("wtg.exe returned non-zero exit code: $wtgExit")
    }

    $joined = ($wtgOut -join "`n")
    $required = @("NVML device 0","UUID","Util","VRAM","Power")

    foreach ($token in $required) {
        if ($joined -notmatch [regex]::Escape($token)) {
            $pass = $false
            $failReasons.Add("WTG output missing token: $token")
        }
    }
}

Add-Content -Path $f -Value "----" -Encoding utf8
Add-Content -Path $f -Value ("RESULT: {0}" -f ($(if ($pass) { "PASS" } else { "FAIL" }))) -Encoding utf8

if (-not $pass) {
    Add-Content -Path $f -Value "FAIL_REASONS:" -Encoding utf8
    foreach ($r in $failReasons) {
        Add-Content -Path $f -Value ("- {0}" -f $r) -Encoding utf8
    }
}

Write-Host ("Wrote {0}" -f $f)
if ($pass) { exit 0 } else { exit 1 }
