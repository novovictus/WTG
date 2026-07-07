# wtg_providers_test.ps1
# Windows 10/11 provider test harness (PowerShell 5.1 compatible)
# Assumes current working directory contains wtg.exe.
#
# Output file (results\):
#   wtg_providers_<hostname>_<amd-model>_<amd-driver>_<intel-model>_<intel-driver>_<yyyyMMdd-HHmmss>.txt

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-GpuTokenFromName {
    param(
        [Parameter(Mandatory=$true)][string]$GpuName,
        [Parameter(Mandatory=$true)][string[]]$StripWords
    )

    # Human-facing token:
    # - drop vendor-specific noise words (case-insensitive)
    # - lowercase
    # - spaces -> dashes
    $token = $GpuName.ToLower()
    foreach ($word in $StripWords) {
        $token = $token -replace [regex]::Escape($word.ToLower()), ""
    }
    $token = $token -replace "[^a-z0-9]+", "-"
    $token = $token -replace "-+", "-"
    $token = $token.Trim("-")
    if ([string]::IsNullOrEmpty($token)) { $token = "unknown" }
    return $token
}

function Get-AmdGpuTokenFromName {
    param(
        [Parameter(Mandatory=$true)][string]$GpuName
    )

    # Preserve model-focused AMD slugs such as r9-m360, but avoid weak generic
    # filenames like "graphics" for integrated AMD Radeon(TM) Graphics.
    $token = Get-GpuTokenFromName -GpuName $GpuName -StripWords @("amd","radeon","gpu","(tm)")

    if ($token -eq "graphics" -or $token -eq "unknown") {
        $token = Get-GpuTokenFromName -GpuName $GpuName -StripWords @("gpu","(tm)")
    }

    return $token
}

$root   = (Get-Location).Path
$outDir = Join-Path $root "results"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null

$hostname  = $env:COMPUTERNAME
$timestamp = (Get-Date -Format u)
$tsTag     = (Get-Date).ToUniversalTime().ToString("yyyyMMdd-HHmmss")

# System identity
$cs   = Get-CimInstance -ClassName Win32_ComputerSystem
$bios = Get-CimInstance -ClassName Win32_BIOS
$bb   = Get-CimInstance -ClassName Win32_BaseBoard

# GPU identity (CIM) - query once
$vcAll = @(Get-CimInstance -ClassName Win32_VideoController)

$amdVc = $vcAll |
    Where-Object { $_.Name -match "AMD|Radeon" } |
    Select-Object -First 1

$intelVc = $vcAll |
    Where-Object { $_.Name -match "Intel" } |
    Select-Object -First 1

$amdName = if ($amdVc) { $amdVc.Name } else { "N/A" }
$amdDrv  = if ($amdVc) { $amdVc.DriverVersion } else { "N/A" }

$intelName = if ($intelVc) { $intelVc.Name } else { "N/A" }
$intelDrv  = if ($intelVc) { $intelVc.DriverVersion } else { "N/A" }

# Filename tokens (dirty/best-effort - beta harness)
$amdToken   = if ($amdVc)   { Get-AmdGpuTokenFromName -GpuName $amdVc.Name } else { "no-amd" }
$amdDrvTok  = if ($amdVc)   { $amdDrv -replace "[^\w\.]", "-" } else { "no-amd-drv" }
$intelToken = if ($intelVc) { Get-GpuTokenFromName -GpuName $intelVc.Name -StripWords @("intel","graphics","gpu","(r)") } else { "no-intel" }
$intelDrvTok = if ($intelVc) { $intelDrv -replace "[^\w\.]", "-" } else { "no-intel-drv" }

# Output file
$f = Join-Path $outDir ("wtg_providers_{0}_{1}_{2}_{3}_{4}_{5}.txt" -f `
    $hostname,
    $amdToken,
    $amdDrvTok,
    $intelToken,
    $intelDrvTok,
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
Add-Content -Path $f -Value ("AMD GPU (CIM): {0}" -f $amdName) -Encoding utf8
Add-Content -Path $f -Value ("AMD Windows DriverVersion (CIM): {0}" -f $amdDrv) -Encoding utf8
Add-Content -Path $f -Value ("Intel GPU (CIM): {0}" -f $intelName) -Encoding utf8
Add-Content -Path $f -Value ("Intel Windows DriverVersion (CIM): {0}" -f $intelDrv) -Encoding utf8
Add-Content -Path $f -Value "----" -Encoding utf8

# All video controllers section
Add-Content -Path $f -Value "---- VIDEO CONTROLLERS (CIM) ----" -Encoding utf8

foreach ($vc in $vcAll) {
    Add-Content -Path $f -Value ("Name: {0}" -f $vc.Name) -Encoding utf8
    Add-Content -Path $f -Value ("DriverVersion: {0}" -f $vc.DriverVersion) -Encoding utf8
    Add-Content -Path $f -Value ("PNPDeviceID: {0}" -f $vc.PNPDeviceID) -Encoding utf8
    Add-Content -Path $f -Value "----" -Encoding utf8
}

# WTG provider sections + validation
$wtg = Join-Path $root "wtg.exe"

$pass = $true
$failReasons = New-Object System.Collections.Generic.List[string]
$notes = New-Object System.Collections.Generic.List[string]

if (-not (Test-Path $wtg)) {
    $pass = $false
    $failReasons.Add("wtg.exe not found in current working directory: $root")
    Add-Content -Path $f -Value ("ERROR: {0}" -f $failReasons[$failReasons.Count-1]) -Encoding utf8
} else {
    # AMD provider section
    Add-Content -Path $f -Value "---- WTG --provider amd --once ----" -Encoding utf8

    $amdOut = & $wtg --provider amd --once 2>&1
    $amdExit = $LASTEXITCODE

    if ($amdOut) { $amdOut | Add-Content -Path $f -Encoding utf8 }

    $joined = ($amdOut -join "`n")
    $commonRequired = @(
        "WTG snapshot mode (provider: AMD ADL)",
        "Provider source: wtg.provider.amd.adl",
        "Telemetry class: provider_telemetry"
    )

    foreach ($token in $commonRequired) {
        if ($joined -notmatch [regex]::Escape($token)) {
            $pass = $false
            $failReasons.Add("AMD provider output missing token: $token")
        }
    }

    if ($amdVc) {
        if ($joined -match [regex]::Escape("Provider status: error")) {
            $pass = $false
            $failReasons.Add("AMD provider reported error on present AMD hardware")
        }

        if ($amdExit -ne 0) {
            $pass = $false
            $failReasons.Add("wtg.exe --provider amd --once returned non-zero exit code with AMD hardware present: $amdExit")
        }

        if ($joined -notmatch [regex]::Escape("ADL adapter group")) {
            $pass = $false
            $failReasons.Add("AMD provider output missing token: ADL adapter group")
        }
    } else {
        if ($amdExit -ne 2) {
            $pass = $false
            $failReasons.Add("wtg.exe --provider amd --once returned unexpected exit code without AMD hardware: $amdExit")
        }

        foreach ($token in @("Provider status: unavailable","Reason:")) {
            if ($joined -notmatch [regex]::Escape($token)) {
                $pass = $false
                $failReasons.Add("AMD absent-hardware output missing token: $token")
            }
        }

        if ($pass) {
            $notes.Add("AMD absent hardware accepted")
        }
    }

    Add-Content -Path $f -Value "----" -Encoding utf8

    # Intel provider section
    Add-Content -Path $f -Value "---- WTG --provider intel --once ----" -Encoding utf8

    $intelOut = & $wtg --provider intel --once 2>&1
    $intelExit = $LASTEXITCODE

    if ($intelOut) { $intelOut | Add-Content -Path $f -Encoding utf8 }

    $joined = ($intelOut -join "`n")
    $commonRequired = @(
        "WTG snapshot mode (provider: Intel Level Zero)",
        "Provider source: wtg.provider.intel.level_zero",
        "Telemetry class: provider_telemetry"
    )

    foreach ($token in $commonRequired) {
        if ($joined -notmatch [regex]::Escape($token)) {
            $pass = $false
            $failReasons.Add("Intel provider output missing token: $token")
        }
    }

    if ($intelVc) {
        if ($intelExit -ne 0) {
            $pass = $false
            $failReasons.Add("wtg.exe --provider intel --once returned non-zero exit code with Intel hardware present: $intelExit")
        }

        foreach ($token in @("Intel device 0","UUID:")) {
            if ($joined -notmatch [regex]::Escape($token)) {
                $pass = $false
                $failReasons.Add("Intel provider output missing token: $token")
            }
        }

        if ($joined -match [regex]::Escape("Sysman unavailable")) {
            $notes.Add("Intel Sysman unavailable accepted")
        }
    } else {
        if ($intelExit -ne 2) {
            $pass = $false
            $failReasons.Add("wtg.exe --provider intel --once returned unexpected exit code without Intel hardware: $intelExit")
        }

        foreach ($token in @("Provider status: unavailable","Reason:")) {
            if ($joined -notmatch [regex]::Escape($token)) {
                $pass = $false
                $failReasons.Add("Intel absent-hardware output missing token: $token")
            }
        }

        if ($pass) {
            $notes.Add("Intel absent hardware accepted")
        }
    }

}

Add-Content -Path $f -Value "----" -Encoding utf8
Add-Content -Path $f -Value ("RESULT: {0}" -f ($(if ($pass) { "PASS" } else { "FAIL" }))) -Encoding utf8

if ($notes.Count -gt 0) {
    Add-Content -Path $f -Value "NOTES:" -Encoding utf8
    foreach ($n in $notes) {
        Add-Content -Path $f -Value ("- {0}" -f $n) -Encoding utf8
    }
}

if (-not $pass) {
    Add-Content -Path $f -Value "FAIL_REASONS:" -Encoding utf8

    foreach ($r in $failReasons) {
        Add-Content -Path $f -Value ("- {0}" -f $r) -Encoding utf8
    }
}

Write-Host ("Wrote {0}" -f $f)

if ($pass) { exit 0 } else { exit 1 }
