# WTG drop-folder probe-fields validation harness.
# Place this script next to wtg.exe and run it from the drop folder.
# PowerShell 5.1 compatible. ASCII output only.

param(
    [switch]$IncludeWatch,
    [int]$WatchSeconds = 5
)

$ErrorActionPreference = "Stop"
$HarnessName = "wtg_validate_probe_fields"

function ConvertTo-SafePathSegment {
    param([string]$Value)

    if ([string]::IsNullOrWhiteSpace($Value)) {
        return ""
    }

    $safe = $Value.Trim() -replace '[^A-Za-z0-9._-]+', "-"
    $safe = $safe.Trim(".-_".ToCharArray())

    return $safe
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

function Format-ProcessArguments {
    param([string[]]$Arguments)

    return $Arguments
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

function Get-DropSinkFiles {
    param([string]$RunFolder)

    $files = @()
    foreach ($pattern in @("wtg_sink_*.csv", "wtg_sink_*.jsonl")) {
        $files += @(Get-ChildItem -LiteralPath $RunFolder -Filter $pattern -File -ErrorAction SilentlyContinue)
    }

    return $files
}

function Remove-DropSinkFiles {
    param([string]$RunFolder)

    foreach ($file in @(Get-DropSinkFiles -RunFolder $RunFolder)) {
        try {
            Remove-Item -LiteralPath $file.FullName -Force -ErrorAction Stop
        } catch {
        }
    }
}

function Copy-DropSinkFiles {
    param(
        [string]$RunFolder,
        [string]$OutputDir,
        [string]$Prefix
    )

    $copied = @()
    foreach ($file in @(Get-DropSinkFiles -RunFolder $RunFolder)) {
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
        [string]$DisplayFileName,
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
        DisplayFileName = $DisplayFileName
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
        [string]$RunFolder,
        [string]$OutputDir
    )

    $stdoutPath = Join-Path $OutputDir ($Command.Step + ".stdout.txt")
    $stderrPath = Join-Path $OutputDir ($Command.Step + ".stderr.txt")
    $commandPath = Join-Path $OutputDir ($Command.Step + ".command.txt")
    $exitPath = Join-Path $OutputDir ($Command.Step + ".exit-code.txt")
    $displayCommand = Format-CommandLine -FileName $Command.DisplayFileName -Arguments $Command.Arguments
    $exitCode = 127
    $timedOut = $false
    $started = $false
    $harnessError = ""
    $copiedSinks = @()
    $process = $null

    Write-AsciiFile -Path $commandPath -Lines @($displayCommand)

    if ($Command.Sink) {
        Remove-DropSinkFiles -RunFolder $RunFolder
    }

    try {
        if ($Command.TimeoutSeconds -gt 0) {
            $process = Start-Process `
                -FilePath $Command.FileName `
                -ArgumentList (Format-ProcessArguments -Arguments $Command.Arguments) `
                -WorkingDirectory $RunFolder `
                -RedirectStandardOutput $stdoutPath `
                -RedirectStandardError $stderrPath `
                -NoNewWindow `
                -PassThru

            $started = $true
            $timeoutMilliseconds = [int]([Math]::Max(1, $Command.TimeoutSeconds) * 1000)
            if (-not $process.WaitForExit($timeoutMilliseconds)) {
                $timedOut = $true

                try {
                    Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
                } catch {
                }
            }

            try {
                $process.WaitForExit()
            } catch {
            }

            if ($timedOut) {
                $exitCode = -1
            } else {
                try {
                    $exitCode = $process.ExitCode
                } catch {
                    $exitCode = 127
                }
            }
        } else {
            $process = Start-Process `
                -FilePath $Command.FileName `
                -ArgumentList (Format-ProcessArguments -Arguments $Command.Arguments) `
                -WorkingDirectory $RunFolder `
                -RedirectStandardOutput $stdoutPath `
                -RedirectStandardError $stderrPath `
                -NoNewWindow `
                -Wait `
                -PassThru

            $started = $true

            try {
                $exitCode = $process.ExitCode
            } catch {
                $exitCode = 127
            }
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
        $copiedSinks = @(Copy-DropSinkFiles -RunFolder $RunFolder -OutputDir $OutputDir -Prefix $Command.SinkPrefix)
        Remove-DropSinkFiles -RunFolder $RunFolder
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

$ScriptPath = $PSCommandPath
if ([string]::IsNullOrWhiteSpace($ScriptPath)) {
    $ScriptPath = $MyInvocation.MyCommand.Path
}

$ScriptPath = (Resolve-Path -LiteralPath $ScriptPath).Path
$RunFolder = (Resolve-Path -LiteralPath (Split-Path -Parent $ScriptPath)).Path
$ExePath = Join-Path $RunFolder "wtg.exe"

if (-not (Test-Path -LiteralPath $ExePath -PathType Leaf)) {
    throw "wtg.exe was not found next to this script. Expected: $ExePath"
}

$Timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
$ComputerName = $env:COMPUTERNAME
$SafeComputerName = ConvertTo-SafePathSegment -Value $ComputerName

if ([string]::IsNullOrWhiteSpace($SafeComputerName)) {
    $OutputName = "validate_probe_fields_{0}" -f $Timestamp
} else {
    $OutputName = "validate_probe_fields_{0}_{1}" -f $Timestamp, $SafeComputerName
}

$OutputDir = Join-Path $RunFolder $OutputName
$OutputDir = (New-Item -ItemType Directory -Path $OutputDir -Force).FullName

$ExeHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $ExePath).Hash

$oldNoColor = $env:NO_COLOR
$oldCliColor = $env:CLICOLOR
$env:NO_COLOR = "1"
$env:CLICOLOR = "0"

$commands = @()
$commands += New-CommandSpec -Step "01_probe" -Name "probe" -FileName $ExePath -DisplayFileName ".\wtg.exe" -Arguments @("--probe") -Sink $false -SinkPrefix "" -TimeoutSeconds 0 -ExpectedStop $false
$commands += New-CommandSpec -Step "02_probe_sink_csv" -Name "probe_csv" -FileName $ExePath -DisplayFileName ".\wtg.exe" -Arguments @("--probe", "--sink", "csv") -Sink $true -SinkPrefix "02_probe_sink_csv" -TimeoutSeconds 0 -ExpectedStop $false
$commands += New-CommandSpec -Step "03_probe_fields" -Name "probe_fields" -FileName $ExePath -DisplayFileName ".\wtg.exe" -Arguments @("--probe-fields", "--field-id", "74", "--field-id", "78", "--field-id", "83", "--field-id", "94", "--field-id", "95") -Sink $false -SinkPrefix "" -TimeoutSeconds 0 -ExpectedStop $false
$commands += New-CommandSpec -Step "04_once" -Name "once" -FileName $ExePath -DisplayFileName ".\wtg.exe" -Arguments @("--once") -Sink $false -SinkPrefix "" -TimeoutSeconds 0 -ExpectedStop $false
$commands += New-CommandSpec -Step "05_once_stats" -Name "once_stats" -FileName $ExePath -DisplayFileName ".\wtg.exe" -Arguments @("--once", "--stats") -Sink $false -SinkPrefix "" -TimeoutSeconds 0 -ExpectedStop $false

if ($IncludeWatch) {
    $commands += New-CommandSpec -Step "06_watch_jsonl" -Name "watch" -FileName $ExePath -DisplayFileName ".\wtg.exe" -Arguments @("--watch", "--interval", "1000", "--sink", "jsonl") -Sink $true -SinkPrefix "06_watch_jsonl" -TimeoutSeconds $WatchSeconds -ExpectedStop $true
}

$ManifestPath = Join-Path $OutputDir "manifest.txt"
$SummaryPath = Join-Path $OutputDir "summary.txt"

$manifestLines = @(
    "harness name: $HarnessName",
    "timestamp: $Timestamp",
    "computer name: $ComputerName",
    "script path: $ScriptPath",
    "run folder: $RunFolder",
    "output directory: $OutputDir",
    "wtg.exe path: $ExePath",
    "wtg.exe sha256: $ExeHash",
    "",
    "command list:"
)

foreach ($command in $commands) {
    $manifestLines += ("{0}: {1}" -f $command.Step, (Format-CommandLine -FileName $command.DisplayFileName -Arguments $command.Arguments))
}

Write-AsciiFile -Path $ManifestPath -Lines $manifestLines

$results = @()

try {
    foreach ($command in $commands) {
        $results += Invoke-CapturedCommand -Command $command -RunFolder $RunFolder -OutputDir $OutputDir
    }
} finally {
    Remove-DropSinkFiles -RunFolder $RunFolder
    $env:NO_COLOR = $oldNoColor
    $env:CLICOLOR = $oldCliColor
}

$probeResult = Get-Result -Results $results -Name "probe"
$probeCsvResult = Get-Result -Results $results -Name "probe_csv"
$probeFieldsResult = Get-Result -Results $results -Name "probe_fields"
$onceResult = Get-Result -Results $results -Name "once"
$onceStatsResult = Get-Result -Results $results -Name "once_stats"
$watchResult = Get-Result -Results $results -Name "watch"

$summaryLines = @(
    "exe path: $ExePath",
    "exe sha256: $ExeHash",
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
