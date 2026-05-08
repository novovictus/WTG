param(
    [string]$OutputRoot = "artifacts/validation",
    [string]$Label = "",
    [switch]$IncludeWatch,
    [int]$WatchSeconds = 5
)

$ErrorActionPreference = "Stop"
$HarnessName = "wtg_validate_probe_fields"

function ConvertTo-SafePathSegment {
    param(
        [string]$Value,
        [string]$Fallback
    )

    if ([string]::IsNullOrWhiteSpace($Value)) {
        return $Fallback
    }

    $safe = $Value.Trim() -replace '[^A-Za-z0-9._-]+', '-'
    $safe = $safe.Trim(".-_".ToCharArray())

    if ([string]::IsNullOrWhiteSpace($safe)) {
        return $Fallback
    }

    return $safe
}

function Get-RepoRoot {
    $current = (Get-Location).Path

    try {
        $rootLines = & git rev-parse --show-toplevel 2>$null
        if ($LASTEXITCODE -eq 0 -and $rootLines) {
            $root = [string]($rootLines | Select-Object -First 1)
            if (-not [string]::IsNullOrWhiteSpace($root)) {
                return (Resolve-Path -LiteralPath $root.Trim()).Path
            }
        }
    } catch {
    }

    return $current
}

function Get-CommandValue {
    param(
        [string]$FileName,
        [string[]]$Arguments,
        [string]$Fallback
    )

    try {
        $lines = & $FileName @Arguments 2>$null
        if ($LASTEXITCODE -eq 0 -and $lines) {
            return ([string]($lines | Select-Object -First 1)).Trim()
        }
    } catch {
    }

    return $Fallback
}

function Format-CommandPart {
    param([string]$Value)

    if ($null -eq $Value -or $Value.Length -eq 0) {
        return '""'
    }

    if ($Value -match '^[A-Za-z0-9._/\\:=+-]+$') {
        return $Value
    }

    return '"' + ($Value -replace '"', '\"') + '"'
}

function Format-CommandLine {
    param(
        [string]$FileName,
        [string[]]$Arguments
    )

    $parts = @((Format-CommandPart -Value $FileName))
    foreach ($argument in $Arguments) {
        $parts += (Format-CommandPart -Value $argument)
    }

    return ($parts -join " ")
}

function Format-CmdPath {
    param([string]$Path)

    return '"' + ($Path -replace '"', '""') + '"'
}

function Write-AsciiFile {
    param(
        [string]$Path,
        [string[]]$Lines
    )

    if ($null -eq $Lines -or $Lines.Count -eq 0) {
        [System.IO.File]::WriteAllText($Path, "", [System.Text.Encoding]::ASCII)
        return
    }

    $Lines | Out-File -FilePath $Path -Encoding ascii -Width 4096
}

function Get-RootSinkFiles {
    param([string]$RepoRoot)

    $files = @()
    foreach ($pattern in @("wtg_sink_*.csv", "wtg_sink_*.jsonl")) {
        $files += @(Get-ChildItem -LiteralPath $RepoRoot -Filter $pattern -File -ErrorAction SilentlyContinue)
    }

    return $files
}

function Remove-RootSinkFiles {
    param([string]$RepoRoot)

    foreach ($file in @(Get-RootSinkFiles -RepoRoot $RepoRoot)) {
        try {
            Remove-Item -LiteralPath $file.FullName -Force -ErrorAction Stop
        } catch {
        }
    }
}

function Copy-RootSinkFiles {
    param(
        [string]$RepoRoot,
        [string]$OutputDir,
        [string]$Prefix
    )

    $copied = @()
    foreach ($file in @(Get-RootSinkFiles -RepoRoot $RepoRoot)) {
        $destination = Join-Path $OutputDir ("{0}_{1}" -f $Prefix, $file.Name)
        Copy-Item -LiteralPath $file.FullName -Destination $destination -Force
        $copied += $destination
    }

    return $copied
}

function New-CommandSpec {
    param(
        [string]$Step,
        [string]$Name,
        [string]$FileName,
        [string[]]$Arguments,
        [bool]$Sink,
        [string]$SinkPrefix,
        [int]$TimeoutSeconds,
        [bool]$ExpectedStop
    )

    return [pscustomobject]@{
        Step = $Step
        Name = $Name
        FileName = $FileName
        Arguments = $Arguments
        Sink = $Sink
        SinkPrefix = $SinkPrefix
        TimeoutSeconds = $TimeoutSeconds
        ExpectedStop = $ExpectedStop
    }
}

function Invoke-CapturedCommand {
    param(
        [pscustomobject]$Command,
        [string]$RepoRoot,
        [string]$OutputDir
    )

    $stdoutPath = Join-Path $OutputDir ($Command.Step + ".stdout.txt")
    $stderrPath = Join-Path $OutputDir ($Command.Step + ".stderr.txt")
    $commandPath = Join-Path $OutputDir ($Command.Step + ".command.txt")
    $exitPath = Join-Path $OutputDir ($Command.Step + ".exit-code.txt")
    $displayCommand = Format-CommandLine -FileName $Command.FileName -Arguments $Command.Arguments
    $exitCode = 127
    $timedOut = $false
    $started = $false
    $harnessError = ""
    $copiedSinks = @()
    $cmdLine = "{0} 1> {1} 2> {2}" -f $displayCommand, (Format-CmdPath -Path $stdoutPath), (Format-CmdPath -Path $stderrPath)

    Write-AsciiFile -Path $commandPath -Lines @($displayCommand)

    if ($Command.Sink) {
        Remove-RootSinkFiles -RepoRoot $RepoRoot
    }

    try {
        $processStartInfo = New-Object System.Diagnostics.ProcessStartInfo
        $processStartInfo.FileName = "cmd.exe"
        $processStartInfo.Arguments = "/s /c " + $cmdLine
        $processStartInfo.WorkingDirectory = $RepoRoot
        $processStartInfo.UseShellExecute = $false
        $processStartInfo.CreateNoWindow = $true

        $process = New-Object System.Diagnostics.Process
        $process.StartInfo = $processStartInfo
        [void]$process.Start()
        $started = $true

        if ($Command.TimeoutSeconds -gt 0) {
            $timeoutMilliseconds = [int]([Math]::Max(1, $Command.TimeoutSeconds) * 1000)
            if (-not $process.WaitForExit($timeoutMilliseconds)) {
                $timedOut = $true

                try {
                    & taskkill.exe /PID $process.Id /T /F > $null 2>&1
                } catch {
                    try {
                        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
                    } catch {
                    }
                }

                try {
                    $process.WaitForExit()
                } catch {
                }
            }
        } else {
            $process.WaitForExit()
        }

        try {
            $exitCode = $process.ExitCode
        } catch {
            $exitCode = 127
        }

        if ($timedOut) {
            $exitCode = -1
        }
    } catch {
        $harnessError = $_.Exception.Message
        Write-AsciiFile -Path $stderrPath -Lines @("Harness error:", $harnessError)
    }

    if (-not (Test-Path -LiteralPath $stdoutPath)) {
        Write-AsciiFile -Path $stdoutPath -Lines @()
    }

    if (-not (Test-Path -LiteralPath $stderrPath)) {
        Write-AsciiFile -Path $stderrPath -Lines @()
    }

    if ($Command.Sink) {
        Start-Sleep -Milliseconds 300
        $copiedSinks = @(Copy-RootSinkFiles -RepoRoot $RepoRoot -OutputDir $OutputDir -Prefix $Command.SinkPrefix)
        Remove-RootSinkFiles -RepoRoot $RepoRoot
    }

    $exitLines = @(
        "step: $($Command.Step)",
        "name: $($Command.Name)",
        "command: $displayCommand",
        "started: $(if ($started) { 'yes' } else { 'no' })",
        "exit_code: $exitCode",
        "timed_out: $(if ($timedOut) { 'yes' } else { 'no' })",
        "expected_stop: $(if ($Command.ExpectedStop) { 'yes' } else { 'no' })",
        "sink_files_copied: $($copiedSinks.Count)"
    )

    if ($harnessError.Length -gt 0) {
        $exitLines += "harness_error: $harnessError"
    }

    foreach ($sink in $copiedSinks) {
        $exitLines += "sink_file: $sink"
    }

    Write-AsciiFile -Path $exitPath -Lines $exitLines

    return [pscustomobject]@{
        Step = $Command.Step
        Name = $Command.Name
        Command = $displayCommand
        ExitCode = $exitCode
        TimedOut = $timedOut
        ExpectedStop = $Command.ExpectedStop
        StdoutPath = $stdoutPath
        StderrPath = $stderrPath
        ExitPath = $exitPath
        SinkFiles = @($copiedSinks)
    }
}

function Get-Result {
    param(
        [object[]]$Results,
        [string]$Name
    )

    foreach ($result in $Results) {
        if ($result.Name -eq $Name) {
            return $result
        }
    }

    return $null
}

function Format-YesNo {
    param([bool]$Value)

    if ($Value) {
        return "yes"
    }

    return "no"
}

function Test-LogCaptured {
    param([object]$Result)

    if ($null -eq $Result) {
        return $false
    }

    return ((Test-Path -LiteralPath $Result.StdoutPath) -and
        (Test-Path -LiteralPath $Result.StderrPath) -and
        (Test-Path -LiteralPath $Result.ExitPath))
}

function Test-SinkCaptured {
    param([object]$Result)

    if ($null -eq $Result) {
        return $false
    }

    return (@($Result.SinkFiles).Count -gt 0)
}

if ($WatchSeconds -lt 1) {
    $WatchSeconds = 1
}

$RepoRoot = Get-RepoRoot
$Timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
$ComputerName = if ([string]::IsNullOrWhiteSpace($env:COMPUTERNAME)) { "unknown-computer" } else { $env:COMPUTERNAME }
$SafeComputerName = ConvertTo-SafePathSegment -Value $ComputerName -Fallback "unknown-computer"
$SafeLabel = ConvertTo-SafePathSegment -Value $Label -Fallback "unlabeled"

if ([System.IO.Path]::IsPathRooted($OutputRoot)) {
    $OutputRootPath = $OutputRoot
} else {
    $OutputRootPath = Join-Path $RepoRoot $OutputRoot
}

$OutputRootPath = (New-Item -ItemType Directory -Path $OutputRootPath -Force).FullName
$OutputDir = Join-Path $OutputRootPath ("{0}_{1}_{2}" -f $Timestamp, $SafeComputerName, $SafeLabel)
$OutputDir = (New-Item -ItemType Directory -Path $OutputDir -Force).FullName

$oldNoColor = $env:NO_COLOR
$oldCliColor = $env:CLICOLOR
$oldCargoTermColor = $env:CARGO_TERM_COLOR
$env:NO_COLOR = "1"
$env:CLICOLOR = "0"
$env:CARGO_TERM_COLOR = "never"

$commands = @()
$commands += New-CommandSpec -Step "01_git_status" -Name "git_status" -FileName "git" -Arguments @("status", "-sb") -Sink $false -SinkPrefix "" -TimeoutSeconds 0 -ExpectedStop $false
$commands += New-CommandSpec -Step "02_git_log" -Name "git_log" -FileName "git" -Arguments @("log", "--oneline", "--decorate", "-12") -Sink $false -SinkPrefix "" -TimeoutSeconds 0 -ExpectedStop $false
$commands += New-CommandSpec -Step "03_cargo_build" -Name "build" -FileName "cargo" -Arguments @("build") -Sink $false -SinkPrefix "" -TimeoutSeconds 0 -ExpectedStop $false
$commands += New-CommandSpec -Step "04_probe" -Name "probe" -FileName "cargo" -Arguments @("run", "--", "--probe") -Sink $false -SinkPrefix "" -TimeoutSeconds 0 -ExpectedStop $false
$commands += New-CommandSpec -Step "05_probe_sink_csv" -Name "probe_csv" -FileName "cargo" -Arguments @("run", "--", "--probe", "--sink", "csv") -Sink $true -SinkPrefix "05_probe_sink_csv" -TimeoutSeconds 0 -ExpectedStop $false
$commands += New-CommandSpec -Step "06_probe_fields" -Name "probe_fields" -FileName "cargo" -Arguments @("run", "--", "--probe-fields", "--field-id", "74", "--field-id", "78", "--field-id", "83", "--field-id", "94", "--field-id", "95") -Sink $false -SinkPrefix "" -TimeoutSeconds 0 -ExpectedStop $false
$commands += New-CommandSpec -Step "07_once" -Name "once" -FileName "cargo" -Arguments @("run", "--", "--once") -Sink $false -SinkPrefix "" -TimeoutSeconds 0 -ExpectedStop $false
$commands += New-CommandSpec -Step "08_once_stats" -Name "once_stats" -FileName "cargo" -Arguments @("run", "--", "--once", "--stats") -Sink $false -SinkPrefix "" -TimeoutSeconds 0 -ExpectedStop $false

if ($IncludeWatch) {
    $commands += New-CommandSpec -Step "09_watch_jsonl" -Name "watch" -FileName "cargo" -Arguments @("run", "--", "--watch", "--interval", "1000", "--sink", "jsonl") -Sink $true -SinkPrefix "09_watch_jsonl" -TimeoutSeconds $WatchSeconds -ExpectedStop $true
}

$GitBranch = Get-CommandValue -FileName "git" -Arguments @("branch", "--show-current") -Fallback "unknown"
$GitHead = Get-CommandValue -FileName "git" -Arguments @("rev-parse", "HEAD") -Fallback "unknown"
$ManifestPath = Join-Path $OutputDir "manifest.txt"
$SummaryPath = Join-Path $OutputDir "summary.txt"

$manifestLines = @(
    "harness name: $HarnessName",
    "timestamp: $Timestamp",
    "computer name: $ComputerName",
    "label: $Label",
    "safe label: $SafeLabel",
    "output directory: $OutputDir",
    "git branch: $GitBranch",
    "git HEAD: $GitHead",
    "",
    "command list:"
)

foreach ($command in $commands) {
    $manifestLines += ("{0}: {1}" -f $command.Step, (Format-CommandLine -FileName $command.FileName -Arguments $command.Arguments))
}

Write-AsciiFile -Path $ManifestPath -Lines $manifestLines

$results = @()

try {
    foreach ($command in $commands) {
        $results += Invoke-CapturedCommand -Command $command -RepoRoot $RepoRoot -OutputDir $OutputDir
    }
} finally {
    Remove-RootSinkFiles -RepoRoot $RepoRoot
    $env:NO_COLOR = $oldNoColor
    $env:CLICOLOR = $oldCliColor
    $env:CARGO_TERM_COLOR = $oldCargoTermColor
}

$buildResult = Get-Result -Results $results -Name "build"
$probeResult = Get-Result -Results $results -Name "probe"
$probeCsvResult = Get-Result -Results $results -Name "probe_csv"
$probeFieldsResult = Get-Result -Results $results -Name "probe_fields"
$onceResult = Get-Result -Results $results -Name "once"
$onceStatsResult = Get-Result -Results $results -Name "once_stats"
$watchResult = Get-Result -Results $results -Name "watch"

$summaryLines = @(
    "build: $(if ($null -ne $buildResult -and $buildResult.ExitCode -eq 0) { 'pass' } else { 'fail' })",
    "probe captured: $(Format-YesNo -Value (Test-LogCaptured -Result $probeResult))",
    "probe CSV captured: $(Format-YesNo -Value (Test-SinkCaptured -Result $probeCsvResult))",
    "probe-fields captured: $(Format-YesNo -Value (Test-LogCaptured -Result $probeFieldsResult))",
    "once captured: $(Format-YesNo -Value (Test-LogCaptured -Result $onceResult))",
    "once stats captured: $(Format-YesNo -Value (Test-LogCaptured -Result $onceStatsResult))"
)

if ($IncludeWatch) {
    $watchCaptured = (Test-LogCaptured -Result $watchResult) -or (Test-SinkCaptured -Result $watchResult)
    $summaryLines += "watch captured: $(Format-YesNo -Value $watchCaptured)"
}

$summaryLines += "output directory: $OutputDir"
$summaryLines += ""
$summaryLines += "nonzero exit codes:"

$nonzeroResults = @($results | Where-Object { $_.ExitCode -ne 0 })
if ($nonzeroResults.Count -eq 0) {
    $summaryLines += "none"
} else {
    foreach ($result in $nonzeroResults) {
        $detail = "- {0}: {1}" -f $result.Step, $result.ExitCode
        if ($result.ExpectedStop -and $result.TimedOut) {
            $detail += " (expected stop after $WatchSeconds seconds)"
        }
        $summaryLines += $detail
    }
}

Write-AsciiFile -Path $SummaryPath -Lines $summaryLines

Write-Host "WTG probe-fields validation complete"
Write-Host "Output directory: $OutputDir"
Write-Host "Summary: $SummaryPath"
