# wtg_encrypt.ps1
# Encrypt ONLY the .txt files in the CURRENT directory into a single 7z archive.
# Output is written to .\wtg-encrypted-results\ with a UTC timestamp.
# Source files are NOT modified.

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Read-SecretString([string]$Prompt) {
    $sec = Read-Host -AsSecureString -Prompt $Prompt
    if (-not $sec) { throw "No password entered." }
    $bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($sec)
    try { return [Runtime.InteropServices.Marshal]::PtrToStringBSTR($bstr) }
    finally { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr) }
}

$here = (Get-Location).Path

# Find 7-Zip
$sevenZip = "$env:ProgramFiles\7-Zip\7z.exe"
if (-not (Test-Path $sevenZip)) { $sevenZip = "$env:ProgramFiles(x86)\7-Zip\7z.exe" }
if (-not (Test-Path $sevenZip)) { throw "7-Zip not found in Program Files." }

# Output dir
$outDir = Join-Path $here "wtg-encrypted-results"
if (-not (Test-Path $outDir)) { New-Item -ItemType Directory -Path $outDir | Out-Null }

# Timestamp tag (UTC)
$shipTs  = (Get-Date).ToUniversalTime().ToString("yyyyMMdd_HHmmss")
$outPath = Join-Path $outDir ("wtg_txt_{0}.7z" -f $shipTs)

# Collect ONLY .txt files in the current directory (non-recursive)
$txtFiles = Get-ChildItem -Path $here -File -Filter "*.txt"
if (-not $txtFiles -or $txtFiles.Count -eq 0) {
    throw "No .txt files found in current directory: $here"
}

# Password prompt (confirm)
$pw1 = Read-SecretString "7-Zip password"
$pw2 = Read-SecretString "Re-enter password"
if ($pw1 -ne $pw2) { throw "Passwords do not match." }

# Create archive from CURRENT DIR so paths stored are relative (file names only)
Push-Location $here
try {
    & $sevenZip a -t7z -mhe=on -m0=lzma2 -mx=9 "-p$pw1" $outPath @($txtFiles.Name) | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "7-Zip failed with exit code $LASTEXITCODE." }
}
finally {
    Pop-Location
    $pw1 = $null
    $pw2 = $null
}

Write-Host ("Wrote {0}" -f $outPath)
Write-Host ("Included {0} file(s): {1}" -f $txtFiles.Count, (($txtFiles.Name) -join ", "))
