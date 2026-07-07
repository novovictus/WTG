# wtg_rc_multi_host_smoke.ps1
# Multi-host RC smoke harness for WTG provider validation.
#
# Purpose:
# - stage a packaged WTG build on the orchestrator and remote targets
# - run the existing script-owned smoke tests locally and remotely
# - preserve evidence filenames exactly as emitted by the smoke scripts
# - collect run metadata, stdout logs, and flat source-of-truth results
#
# This is development/test infrastructure. It intentionally stays under artifacts/dev.

param(
    [string] $Repo = "C:\Users\plays\source\github_wtg\WTG",
    [string] $ReleaseDir = "C:\Users\plays\source\github_wtg\WTG\artifacts\packages\wtg_release_v0.3.0-rc1_v0.3.0_release_v0.3.0-rc1_73a22d6_20260706_231504",
    [string] $Stage = "C:\Users\plays\Desktop\share\0.3.0-rc1",
    [string] $RunRootBase = "C:\Users\plays\Desktop\share\remote_runs",
    [string] $Key = "$env:USERPROFILE\.ssh\wtg_bench_admin_ed25519",
    [switch] $NoPause
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$Results = Join-Path $Stage "results"
$RunId = "wtg_v0.3.0-rc1_" + (Get-Date).ToUniversalTime().ToString("yyyyMMdd-HHmmss")
$RunRoot = Join-Path $RunRootBase $RunId
$RunResults = Join-Path $RunRoot "results"
$RunStdout = Join-Path $RunRoot "stdout"
$RunLogs = Join-Path $RunRoot "logs"
$OrchestratorLog = Join-Path $RunLogs "orchestrator.log"

$Remotes = @(
    @{
        Name = "bench"
        User = "admin"
        Host = "bench"
        Desktop = "C:\Users\admin\Desktop"
    },
    @{
        Name = "surface"
        User = "plays"
        Host = "surface"
        Desktop = "C:\Users\plays\Desktop"
    },
    @{
        Name = "nuc"
        User = "intelnuc"
        Host = "nuc"
        Desktop = "C:\Users\intelnuc\Desktop"
    }
)

$TargetStatus = New-Object System.Collections.Generic.List[object]

function Write-Log {
    param([Parameter(Mandatory=$true)][string] $Message)

    $line = "{0} {1}" -f ((Get-Date).ToUniversalTime().ToString("o")), $Message
    Write-Host $line
    Add-Content -Path $OrchestratorLog -Value $line -Encoding UTF8
}

function Pause-Step {
    param([Parameter(Mandatory=$true)][string] $Message)

    Write-Host ""
    Write-Host "==== $Message ====" -ForegroundColor Cyan

    if ($NoPause) {
        return
    }

    Write-Host "Press Enter to continue, or Ctrl+C to stop." -ForegroundColor Yellow
    Read-Host | Out-Null
}

function Invoke-RemotePS {
    param(
        [Parameter(Mandatory=$true)] [string] $User,
        [Parameter(Mandatory=$true)] [string] $HostName,
        [Parameter(Mandatory=$true)] [string] $Command
    )

    $wrapped = @"
`$ProgressPreference = 'SilentlyContinue'
`$InformationPreference = 'SilentlyContinue'
`$WarningPreference = 'Continue'
`$VerbosePreference = 'SilentlyContinue'
`$DebugPreference = 'SilentlyContinue'
`$ErrorActionPreference = 'Continue'
try {
$Command
  if (`$LASTEXITCODE -ne `$null) {
    exit `$LASTEXITCODE
  } else {
    exit 0
  }
} catch {
  [Console]::Out.WriteLine("REMOTE_EXCEPTION: " + `$_.Exception.Message)
  exit 1
}
"@

    $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($wrapped))
    $target = "${User}@${HostName}"

    ssh -T -F NUL -o IdentitiesOnly=yes -i $Key $target "cmd.exe /d /q /c powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand $encoded"
}

function Invoke-LoggedLocalScript {
    param(
        [Parameter(Mandatory=$true)] [string] $Name,
        [Parameter(Mandatory=$true)] [string] $ScriptPath
    )

    $stdoutPath = Join-Path $RunStdout $Name

    & powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File $ScriptPath 2>&1 |
        Tee-Object -FilePath $stdoutPath

    return $LASTEXITCODE
}

New-Item -ItemType Directory -Force -Path $RunRoot | Out-Null
New-Item -ItemType Directory -Force -Path $RunResults | Out-Null
New-Item -ItemType Directory -Force -Path $RunStdout | Out-Null
New-Item -ItemType Directory -Force -Path $RunLogs | Out-Null
New-Item -ItemType File -Force -Path $OrchestratorLog | Out-Null

Pause-Step "Preflight: show planned run"

Write-Log "run_id=$RunId"
Write-Log "repo=$Repo"
Write-Log "release_dir=$ReleaseDir"
Write-Log "stage=$Stage"
Write-Log "run_root=$RunRoot"
Write-Log "ssh_key=$Key"

foreach ($remote in $Remotes) {
    Write-Log ("remote={0} target={1}@{2} desktop={3}" -f $remote.Name, $remote.User, $remote.Host, $remote.Desktop)
}

Pause-Step "Purge local stage results and run root results"

New-Item -ItemType Directory -Force -Path $Stage | Out-Null
Remove-Item -Recurse -Force $Results -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Results | Out-Null

$TransferZip = "C:\Users\plays\Desktop\share\wtg_0.3.0-rc1_smoke_stage.zip"
Remove-Item -Force $TransferZip -ErrorAction SilentlyContinue

Pause-Step "Stage RC package locally"

$CliExe = Get-ChildItem $ReleaseDir -Filter "*.exe" |
    Where-Object { $_.Name -ne "wtg-ui.exe" } |
    Select-Object -First 1

if (-not $CliExe) {
    throw "No CLI exe found in release directory: $ReleaseDir"
}

Copy-Item $CliExe.FullName (Join-Path $Stage "wtg.exe") -Force

$UiExe = Join-Path $ReleaseDir "wtg-ui.exe"
if (Test-Path $UiExe) {
    Copy-Item $UiExe (Join-Path $Stage "wtg-ui.exe") -Force
}

Copy-Item (Join-Path $Repo "artifacts\dev\wtg_test.ps1") (Join-Path $Stage "wtg_test.ps1") -Force
Copy-Item (Join-Path $Repo "artifacts\dev\wtg_providers_test.ps1") (Join-Path $Stage "wtg_providers_test.ps1") -Force

Get-ChildItem $Stage |
    Where-Object { -not $_.PSIsContainer } |
    Select-Object LastWriteTime, Name, Length

Pause-Step "Run local orchestrator host smoke"

Push-Location $Stage

Remove-Item -Recurse -Force ".\results" -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force ".\results" | Out-Null

$LocalTestExit = Invoke-LoggedLocalScript -Name "local.wtg_test.stdout.txt" -ScriptPath ".\wtg_test.ps1"
$LocalProvidersExit = Invoke-LoggedLocalScript -Name "local.wtg_providers_test.stdout.txt" -ScriptPath ".\wtg_providers_test.ps1"

Pop-Location

Get-ChildItem $Results -File -ErrorAction SilentlyContinue |
    ForEach-Object {
        Copy-Item $_.FullName -Destination (Join-Path $RunResults $_.Name) -Force
    }

$TargetStatus.Add([pscustomobject]@{
    name = "local"
    role = "local"
    host = $env:COMPUTERNAME
    user = [System.Security.Principal.WindowsIdentity]::GetCurrent().Name
    wtg_test_exit = $LocalTestExit
    wtg_providers_test_exit = $LocalProvidersExit
}) | Out-Null

Write-Log "local wtg_test_exit=$LocalTestExit wtg_providers_test_exit=$LocalProvidersExit"

Pause-Step "Build transfer zip without results"

Remove-Item -Force $TransferZip -ErrorAction SilentlyContinue

Get-ChildItem $Stage |
    Where-Object { $_.Name -ne "results" } |
    Compress-Archive -DestinationPath $TransferZip -Force

Get-Item $TransferZip | Select-Object FullName, Length, LastWriteTime

foreach ($remote in $Remotes) {
    $remoteName = $remote.Name
    $user = $remote.User
    $hostName = $remote.Host
    $desktop = $remote.Desktop

    $remoteShare = Join-Path $desktop "share"
    $remoteStage = Join-Path $remoteShare "0.3.0-rc1"
    $remoteZip = Join-Path $remoteShare "wtg_0.3.0-rc1_smoke_stage.zip"
    $remoteResultsZip = Join-Path $remoteShare "wtg_0.3.0-rc1_results_$remoteName.zip"

    $remoteZipScp = $remoteZip.Replace("\", "/")
    $remoteResultsZipScp = $remoteResultsZip.Replace("\", "/")

    Pause-Step "Connectivity check for $remoteName"

    $connectOut = Invoke-RemotePS -User $user -HostName $hostName -Command @"
[Console]::Out.WriteLine("hostname=" + `$env:COMPUTERNAME)
[Console]::Out.WriteLine("whoami=" + [System.Security.Principal.WindowsIdentity]::GetCurrent().Name)
[Console]::Out.WriteLine("userprofile=" + `$env:USERPROFILE)
"@ 2>&1

    $connectOut | Tee-Object -FilePath (Join-Path $RunStdout "$remoteName.connectivity.stdout.txt")
    if ($LASTEXITCODE -ne 0) {
        throw "Connectivity check failed for $remoteName"
    }

    Pause-Step "Purge and prepare remote destination on $remoteName"

    $prepOut = Invoke-RemotePS -User $user -HostName $hostName -Command @"
New-Item -ItemType Directory -Force -Path '$remoteShare' | Out-Null
Remove-Item -Recurse -Force -Path '$remoteStage' -ErrorAction SilentlyContinue
Remove-Item -Force -Path '$remoteZip' -ErrorAction SilentlyContinue
Remove-Item -Force -Path '$remoteResultsZip' -ErrorAction SilentlyContinue
[Console]::Out.WriteLine("prepared=$remoteShare")
"@ 2>&1

    $prepOut | Tee-Object -FilePath (Join-Path $RunStdout "$remoteName.prepare.stdout.txt")
    if ($LASTEXITCODE -ne 0) {
        throw "Remote folder preparation failed for $remoteName"
    }

    Pause-Step "Push transfer zip to $remoteName"

    $remoteZipTarget = "${user}@${hostName}:$remoteZipScp"
    scp -F NUL -o IdentitiesOnly=yes -i $Key $TransferZip $remoteZipTarget
    if ($LASTEXITCODE -ne 0) {
        throw "SCP push failed for $remoteName"
    }

    Pause-Step "Expand staged files on $remoteName"

    $expandOut = Invoke-RemotePS -User $user -HostName $hostName -Command @"
Expand-Archive -Force -LiteralPath '$remoteZip' -DestinationPath '$remoteStage'
New-Item -ItemType Directory -Force -Path '$remoteStage\results' | Out-Null
Get-ChildItem '$remoteStage' |
  ForEach-Object {
    [Console]::Out.WriteLine("stage_item=" + `$_.Name + " length=" + `$_.Length)
  }
"@ 2>&1

    $expandOut | Tee-Object -FilePath (Join-Path $RunStdout "$remoteName.expand.stdout.txt")
    if ($LASTEXITCODE -ne 0) {
        throw "Remote expand failed for $remoteName"
    }

    Pause-Step "Run smoke scripts on $remoteName"

    $runOut = Invoke-RemotePS -User $user -HostName $hostName -Command @"
Set-Location '$remoteStage'
Remove-Item -Recurse -Force '.\results' -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force '.\results' | Out-Null

[Console]::Out.WriteLine("running=wtg_test.ps1")
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File '.\wtg_test.ps1'
`$TestExit = `$LASTEXITCODE
[Console]::Out.WriteLine("exit_wtg_test=" + `$TestExit)

[Console]::Out.WriteLine("running=wtg_providers_test.ps1")
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File '.\wtg_providers_test.ps1'
`$ProvidersExit = `$LASTEXITCODE
[Console]::Out.WriteLine("exit_wtg_providers_test=" + `$ProvidersExit)

if (`$TestExit -ne 0 -or `$ProvidersExit -ne 0) {
  [Console]::Out.WriteLine("remote_smoke_warning=one_or_more_scripts_nonzero")
}

if (Test-Path '$remoteResultsZip') {
  Remove-Item -Force '$remoteResultsZip'
}

Compress-Archive -Force -Path '.\results\*' -DestinationPath '$remoteResultsZip'

Get-ChildItem '.\results' -File -ErrorAction SilentlyContinue |
  ForEach-Object {
    [Console]::Out.WriteLine("result_item=" + `$_.Name + " length=" + `$_.Length)
  }
"@ 2>&1

    $runOut | Tee-Object -FilePath (Join-Path $RunStdout "$remoteName.run.stdout.txt")
    if ($LASTEXITCODE -ne 0) {
        throw "Remote smoke execution failed for $remoteName"
    }

    $TestExitParsed = (($runOut | Select-String -Pattern "^exit_wtg_test=" | Select-Object -Last 1).ToString() -replace "^.*exit_wtg_test=", "")
    $ProvidersExitParsed = (($runOut | Select-String -Pattern "^exit_wtg_providers_test=" | Select-Object -Last 1).ToString() -replace "^.*exit_wtg_providers_test=", "")

    Pause-Step "Pull source-of-truth results from $remoteName"

    $localHostZip = Join-Path $Results "$remoteName-results.zip"
    Remove-Item -Force $localHostZip -ErrorAction SilentlyContinue

    $remoteResultsTarget = "${user}@${hostName}:$remoteResultsZipScp"
    scp -F NUL -o IdentitiesOnly=yes -i $Key $remoteResultsTarget $localHostZip
    if ($LASTEXITCODE -ne 0) {
        throw "SCP pull failed for $remoteName"
    }

    $TempExtract = Join-Path $Results "_extract_$remoteName"
    Remove-Item -Recurse -Force $TempExtract -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force $TempExtract | Out-Null

    Expand-Archive -Force -LiteralPath $localHostZip -DestinationPath $TempExtract

    Get-ChildItem $TempExtract -File -Recurse |
        ForEach-Object {
            Copy-Item $_.FullName -Destination (Join-Path $Results $_.Name) -Force
            Copy-Item $_.FullName -Destination (Join-Path $RunResults $_.Name) -Force
        }

    Remove-Item -Recurse -Force $TempExtract
    Remove-Item -Force $localHostZip

    $TargetStatus.Add([pscustomobject]@{
        name = $remoteName
        role = "remote"
        host = $hostName
        user = $user
        desktop = $desktop
        wtg_test_exit = $TestExitParsed
        wtg_providers_test_exit = $ProvidersExitParsed
    }) | Out-Null

    Write-Log "$remoteName wtg_test_exit=$TestExitParsed wtg_providers_test_exit=$ProvidersExitParsed"
}

Pause-Step "Remove local transfer zip"

Remove-Item -Force $TransferZip -ErrorAction SilentlyContinue

$Manifest = [ordered]@{
    run_id = $RunId
    completed_utc = (Get-Date).ToUniversalTime().ToString("o")
    package = [ordered]@{
        release_dir = $ReleaseDir
        staged_cli = Join-Path $Stage "wtg.exe"
        staged_ui = Join-Path $Stage "wtg-ui.exe"
    }
    orchestrator = [ordered]@{
        host = $env:COMPUTERNAME
        user = [System.Security.Principal.WindowsIdentity]::GetCurrent().Name
        repo = $Repo
        stage = $Stage
        run_root = $RunRoot
    }
    policy = [ordered]@{
        evidence_naming = "script-owned"
        result_collection = "flat"
        transport_zip = "temporary"
        orchestrator_renames_evidence = $false
    }
    targets = $TargetStatus
    results = @(Get-ChildItem $RunResults -File | Sort-Object Name | ForEach-Object {
        [ordered]@{
            name = $_.Name
            length = $_.Length
            last_write_time = $_.LastWriteTimeUtc.ToString("o")
        }
    })
}

$ManifestPath = Join-Path $RunRoot "manifest.json"
$Manifest | ConvertTo-Json -Depth 8 | Set-Content -Path $ManifestPath -Encoding UTF8

Pause-Step "Show final source-of-truth results"

Write-Host "Run root: $RunRoot"
Write-Host "Manifest: $ManifestPath"
Write-Host "Results:  $RunResults"

Get-ChildItem $RunResults -File |
    Sort-Object Name |
    Select-Object Name, Length, LastWriteTime

Write-Host ""
Write-Host "Stage results mirror:"
Get-ChildItem $Results -File |
    Sort-Object Name |
    Select-Object Name, Length, LastWriteTime
