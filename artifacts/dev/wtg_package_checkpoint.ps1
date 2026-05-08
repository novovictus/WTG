# WTG branch checkpoint packaging helper.
# Creates a reproducible local artifact bundle for branch validation.
# PowerShell 5.1 compatible. ASCII output only.

param(
    [string]$OutputRoot = "artifacts\packages",
    [string]$Label = "probe-fields",
    [switch]$Release
)

$ErrorActionPreference = "Stop"

function Safe-Name {
    param([string]$Value)

    if ([string]::IsNullOrWhiteSpace($Value)) {
        return "unlabeled"
    }

    return ($Value -replace '[^A-Za-z0-9._-]', '_')
}

function Write-TextFile {
    param(
        [string]$Path,
        [string]$Text
    )

    $Text | Out-File -FilePath $Path -Encoding UTF8
}

function Run-ProcessCapture {
    param(
        [string]$Name,
        [string]$FilePath,
        [string[]]$Arguments,
        [string]$OutDir
    )

    $logPath = Join-Path $OutDir "$Name.txt"
    $stdoutPath = Join-Path $OutDir "$Name.stdout.tmp"
    $stderrPath = Join-Path $OutDir "$Name.stderr.tmp"

    $display = $FilePath
    if ($Arguments -and $Arguments.Count -gt 0) {
        $display = "$FilePath $($Arguments -join ' ')"
    }

    "PS> $display" | Out-File -FilePath $logPath -Encoding UTF8

    Remove-Item $stdoutPath, $stderrPath -ErrorAction SilentlyContinue

    $proc = Start-Process -FilePath $FilePath `
        -ArgumentList $Arguments `
        -NoNewWindow `
        -Wait `
        -PassThru `
        -RedirectStandardOutput $stdoutPath `
        -RedirectStandardError $stderrPath

    if (Test-Path $stdoutPath) {
        Get-Content $stdoutPath | Tee-Object -FilePath $logPath -Append | Out-Host
    }

    if (Test-Path $stderrPath) {
        Get-Content $stderrPath | Tee-Object -FilePath $logPath -Append | Out-Host
    }

    "exit_code: $($proc.ExitCode)" | Tee-Object -FilePath $logPath -Append | Out-Host

    Remove-Item $stdoutPath, $stderrPath -ErrorAction SilentlyContinue

    return [int]$proc.ExitCode
}

# Ensure repo root.
if (-not (Test-Path ".\Cargo.toml")) {
    throw "Run this script from the WTG repository root."
}

$timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
$computer = Safe-Name $env:COMPUTERNAME
$labelSafe = Safe-Name $Label

$branch = (git branch --show-current).Trim()
$head = (git rev-parse --short HEAD).Trim()
$fullHead = (git rev-parse HEAD).Trim()

# Version from workspace Cargo.toml.
$versionLine = Select-String -Path ".\Cargo.toml" -Pattern 'version = "' | Select-Object -First 1
$version = "unknown"
if ($versionLine) {
    $version = ($versionLine.Line -replace '.*version = "', '') -replace '".*', ''
}

$profile = "debug"
$buildArgs = @("build")
$exePath = ".\target\debug\wtg.exe"

if ($Release) {
    $profile = "release"
    $buildArgs = @("build", "--release")
    $exePath = ".\target\release\wtg.exe"
}

$bundleName = "wtg_${labelSafe}_v${version}_${branch}_${head}_${timestamp}"
$bundleName = Safe-Name $bundleName
$outDir = Join-Path $OutputRoot $bundleName

New-Item -ItemType Directory -Force -Path $outDir | Out-Null

# Clean prior root sink files.
Remove-Item .\wtg_sink_*.csv, .\wtg_sink_*.jsonl -ErrorAction SilentlyContinue

# Capture environment and repo state.
Run-ProcessCapture "00_git_status" "git" @("status", "-sb") $outDir | Out-Null
Run-ProcessCapture "01_git_log" "git" @("log", "--oneline", "--decorate", "-12") $outDir | Out-Null
Run-ProcessCapture "02_rustc_version" "rustc" @("--version") $outDir | Out-Null
Run-ProcessCapture "03_cargo_version" "cargo" @("--version") $outDir | Out-Null
Run-ProcessCapture "04_cargo_metadata_versions" "cargo" @("metadata", "--no-deps", "--format-version", "1") $outDir | Out-Null

# Build.
$buildExit = Run-ProcessCapture "05_build" "cargo" $buildArgs $outDir
if ($buildExit -ne 0) {
    throw "Build failed. See $outDir\05_build.txt"
}

if (-not (Test-Path $exePath)) {
    throw "Expected executable not found: $exePath"
}

$exeName = "wtg_${labelSafe}_v${version}_${branch}_${head}.exe"
$exeName = Safe-Name $exeName
$destExe = Join-Path $outDir $exeName
Copy-Item $exePath $destExe -Force

# Capture runtime proof from copied exe, not cargo run.
Run-ProcessCapture "06_wtg_probe" $destExe @("--probe") $outDir | Out-Null
Run-ProcessCapture "07_wtg_probe_fields" $destExe @("--probe-fields", "--field-id", "74", "--field-id", "78", "--field-id", "83", "--field-id", "94", "--field-id", "95") $outDir | Out-Null
Run-ProcessCapture "08_wtg_once" $destExe @("--once") $outDir | Out-Null

# Hashes.
$hashPath = Join-Path $outDir "SHA256SUMS.txt"
Get-ChildItem $outDir -File |
    Where-Object { $_.Name -ne "SHA256SUMS.txt" -and $_.Name -notlike "*.tmp" } |
    ForEach-Object {
        $hash = Get-FileHash -Algorithm SHA256 -Path $_.FullName
        "$($hash.Hash)  $($_.Name)"
    } | Out-File -FilePath $hashPath -Encoding ASCII

# Manifest.
$manifest = @"
WTG checkpoint package
timestamp: $timestamp
computer: $env:COMPUTERNAME
label: $Label
branch: $branch
head_short: $head
head_full: $fullHead
version: $version
profile: $profile
exe: $exeName

Purpose:
Branch checkpoint artifact for probe/probe-fields validation.
Use this bundle for dev laptop and bench comparison before main merge/release packaging.

Included:
- executable
- git status/log
- rustc/cargo metadata
- build log
- probe output
- probe-fields output
- once output
- SHA256SUMS.txt

Notes:
- This is not necessarily a main-branch release artifact.
- For release packaging, rebuild after merge/tag on main.
"@

Write-TextFile (Join-Path $outDir "manifest.txt") $manifest

# Recompute hashes after manifest creation so it is included too.
Get-ChildItem $outDir -File |
    Where-Object { $_.Name -ne "SHA256SUMS.txt" -and $_.Name -notlike "*.tmp" } |
    ForEach-Object {
        $hash = Get-FileHash -Algorithm SHA256 -Path $_.FullName
        "$($hash.Hash)  $($_.Name)"
    } | Out-File -FilePath $hashPath -Encoding ASCII

# Zip bundle.
$zipPath = "$outDir.zip"
if (Test-Path $zipPath) {
    Remove-Item $zipPath -Force
}
Compress-Archive -Path (Join-Path $outDir "*") -DestinationPath $zipPath -Force

# Clean root sink files again.
Remove-Item .\wtg_sink_*.csv, .\wtg_sink_*.jsonl -ErrorAction SilentlyContinue

Write-Host ""
Write-Host "WTG checkpoint package complete."
Write-Host "Directory: $outDir"
Write-Host "Zip:       $zipPath"
Write-Host ""
Write-Host "Recommended next:"
Write-Host "  Copy the zip to the bench system and run the included executable with:"
Write-Host "  .\$exeName --probe"
Write-Host "  .\$exeName --probe-fields --field-id 74 --field-id 78 --field-id 83 --field-id 94 --field-id 95"
