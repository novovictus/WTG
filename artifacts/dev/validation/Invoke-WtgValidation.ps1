[CmdletBinding()]
param(
    [ValidateRange(1, 60)][int]$WatchSeconds = 3,
    [switch]$IncludeMqtt,
    [string[]]$MqttArguments = @()
)

# WTG 0.3.1 validation evidence collector.  PowerShell 5.1 compatible.
# It records provider-native observations; it does not judge or normalize telemetry.
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-UtcNow { (Get-Date).ToUniversalTime().ToString("o") }

function Format-CommandLine {
    param([string]$Executable, [string[]]$Arguments)
    $quoted = @($Arguments | ForEach-Object {
        if ($_ -match '[\s"]') { '"' + ($_ -replace '"', '\\"') + '"' } else { $_ }
    })
    return ((@($Executable) + $quoted) -join ' ')
}

function Invoke-WtgCommand {
    param(
        [int]$TestNumber,
        [string]$Name,
        [string[]]$Arguments,
        [int]$TimeoutSeconds = 30
    )

    $start = Get-Date
    $beforeSinks = @(Get-ChildItem -LiteralPath $script:WorkDirectory -Filter "wtg_sink_*" -File -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Name)

    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $script:ResolvedWtgPath
    $psi.Arguments = ($Arguments -join ' ')
    $psi.WorkingDirectory = $script:WorkDirectory
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true

    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = $psi
    [void]$process.Start()
    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    $timedOut = -not $process.WaitForExit($TimeoutSeconds * 1000)
    if ($timedOut) {
        $process.Kill()
        $process.WaitForExit()
    }
    $stdout = $stdoutTask.Result
    $stderr = $stderrTask.Result
    $end = Get-Date
    $afterSinks = @(Get-ChildItem -LiteralPath $script:WorkDirectory -Filter "wtg_sink_*" -File -ErrorAction SilentlyContinue | Where-Object { $beforeSinks -notcontains $_.Name })
    $sinkCaptures = @($afterSinks | ForEach-Object {
        [pscustomobject]@{ name = $_.Name; content = [System.IO.File]::ReadAllText($_.FullName) }
    })
    [pscustomobject]@{
        id = ("TEST {0:000}" -f $TestNumber); name = $Name; arguments = @($Arguments)
        command = (Format-CommandLine -Executable $script:ResolvedWtgPath -Arguments $Arguments)
        start_utc = $start.ToUniversalTime().ToString("o"); end_utc = $end.ToUniversalTime().ToString("o")
        duration_ms = [math]::Round(($end - $start).TotalMilliseconds, 3)
        exit_code = $process.ExitCode; timed_out = $timedOut; stdout = $stdout; stderr = $stderr
        sink_output = $sinkCaptures
    }
}

function Add-EvidenceTest {
    param($Test)
    $script:Evidence.Add("============================================================")
    $script:Evidence.Add($Test.id)
    $script:Evidence.Add("Name: " + $Test.name)
    $script:Evidence.Add("Command: " + $Test.command)
    $script:Evidence.Add("Arguments: " + ($Test.arguments -join " "))
    $script:Evidence.Add("Start UTC: " + $Test.start_utc)
    $script:Evidence.Add("End UTC: " + $Test.end_utc)
    $script:Evidence.Add("Duration ms: " + $Test.duration_ms)
    $script:Evidence.Add("Exit code: " + $Test.exit_code)
    $script:Evidence.Add("Timed out and terminated by harness: " + $Test.timed_out)
    $script:Evidence.Add("============================================================")
    $script:Evidence.Add("")
    $script:Evidence.Add("STDOUT:")
    $script:Evidence.Add($Test.stdout)
    $script:Evidence.Add("")
    $script:Evidence.Add("STDERR:")
    $script:Evidence.Add($Test.stderr)
    foreach ($sink in $Test.sink_output) {
        $script:Evidence.Add("")
        $script:Evidence.Add("RAW SINK OUTPUT: " + $sink.name)
        $script:Evidence.Add($sink.content)
    }
    $script:Evidence.Add("")
}

function Test-ProviderHardware { param([object[]]$Adapters, [string]$Pattern) return @($Adapters | Where-Object { ("$($_.Name) $($_.PNPDeviceID)") -match $Pattern }).Count -gt 0 }

$harnessDirectory = (Resolve-Path -LiteralPath $PSScriptRoot).Path
$harnessScriptPath = (Resolve-Path -LiteralPath $PSCommandPath).Path
$harnessScriptName = Split-Path -Leaf $harnessScriptPath
$harnessScriptSha256 = (Get-FileHash -LiteralPath $harnessScriptPath -Algorithm SHA256).Hash
$adjacentWtgPath = Join-Path $harnessDirectory 'wtg.exe'
if (-not (Test-Path -LiteralPath $adjacentWtgPath -PathType Leaf)) {
    throw "wtg.exe must be adjacent to Invoke-WtgValidation.ps1: $adjacentWtgPath. No validation package was created."
}
$script:ResolvedWtgPath = (Resolve-Path -LiteralPath $adjacentWtgPath).Path
$outputDirectory = Join-Path $harnessDirectory 'results'
New-Item -ItemType Directory -Force -Path $outputDirectory | Out-Null
$hostname = [Environment]::MachineName
$tag = (Get-Date).ToUniversalTime().ToString("yyyyMMdd-HHmmss")
$packageName = "wtg_validation_{0}_{1}.zip" -f $hostname, $tag
$packagePath = Join-Path (Resolve-Path -LiteralPath $outputDirectory).Path $packageName
if (Test-Path -LiteralPath $packagePath) { throw "Refusing to overwrite existing package: $packagePath" }
$script:WorkDirectory = Join-Path ([IO.Path]::GetTempPath()) ("wtg-validation-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $script:WorkDirectory | Out-Null
$script:PackageDirectory = Join-Path $script:WorkDirectory 'package'
New-Item -ItemType Directory -Path $script:PackageDirectory | Out-Null

try {
    $videoControllers = @(Get-CimInstance -ClassName Win32_VideoController)
    $computerSystem = Get-CimInstance -ClassName Win32_ComputerSystem
    $operatingSystem = Get-CimInstance -ClassName Win32_OperatingSystem
    $hasNvidia = Test-ProviderHardware $videoControllers 'NVIDIA'
    $hasAmd = Test-ProviderHardware $videoControllers 'AMD|Radeon'
    $hasIntel = Test-ProviderHardware $videoControllers 'Intel'
    $runtime = [ordered]@{
        nvidia_smi_path = @((Get-Command nvidia-smi -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source -First 1), $(if (Test-Path 'C:\Program Files\NVIDIA Corporation\NVSMI\nvidia-smi.exe') { 'C:\Program Files\NVIDIA Corporation\NVSMI\nvidia-smi.exe' })) | Where-Object { $_ } | Select-Object -Unique
        amd_adl_dll_paths = @('C:\Windows\System32\atiadlxx.dll','C:\Windows\System32\atiadlxy.dll' | Where-Object { Test-Path $_ })
        amd_adlx_dll_paths = @('C:\Windows\System32\amdadlx64.dll' | Where-Object { Test-Path $_ })
        intel_level_zero_dll_paths = @('C:\Windows\System32\ze_loader.dll' | Where-Object { Test-Path $_ })
    }
    $gitMetadata = [ordered]@{
        availability = 'not_available'; git_path = $null; command = 'git -C <harness-directory> rev-parse HEAD'
        commit = $null; raw_output = $null
    }
    $gitCommand = Get-Command git -ErrorAction SilentlyContinue
    if ($gitCommand) {
        $gitMetadata.git_path = $gitCommand.Source
        try {
            $gitOutput = & $gitCommand.Source -C $harnessDirectory rev-parse HEAD 2>&1
            if ($LASTEXITCODE -eq 0) {
                $gitMetadata.availability = 'available'
                $gitMetadata.commit = (($gitOutput | Out-String).Trim())
            } else {
                $gitMetadata.raw_output = ($gitOutput | Out-String).Trim()
            }
        } catch {
            $gitMetadata.raw_output = $_.Exception.Message
        }
    }
    $versionProbe = Invoke-WtgCommand -TestNumber 0 -Name 'WTG version provenance' -Arguments @('--version')
    $script:Evidence = New-Object 'System.Collections.Generic.List[string]'
    $script:Evidence.Add('WTG 0.3.1 validation evidence collector')
    $script:Evidence.Add('Run UTC: ' + (Get-UtcNow))
    $script:Evidence.Add('Harness name: ' + $harnessScriptName)
    $script:Evidence.Add('Harness SHA256: ' + $harnessScriptSha256)
    $script:Evidence.Add('Evidence schema: wtg.validation.raw-evidence.v1')
    Add-EvidenceTest $versionProbe

    $tests = New-Object 'System.Collections.Generic.List[object]'
    $testNumber = 1
    $testPlan = New-Object 'System.Collections.Generic.List[object]'
    $testPlan.Add(@{ name = 'NVIDIA default once'; args = @('--once'); applicable = $true })
    $testPlan.Add(@{ name = 'NVIDIA stats once'; args = @('--once','--stats'); applicable = $true })
    if ($hasNvidia) {
        $testPlan.Add(@{ name = 'NVIDIA watch'; args = @('--watch','--interval','1000'); applicable = $true; timeout = $WatchSeconds })
        $testPlan.Add(@{ name = 'NVIDIA probe'; args = @('--probe'); applicable = $true })
        $testPlan.Add(@{ name = 'NVIDIA probe fields 74'; args = @('--probe-fields','--field-id','74'); applicable = $true })
        $testPlan.Add(@{ name = 'NVIDIA stats JSONL sink'; args = @('--once','--stats','--sink','jsonl'); applicable = $true })
        $testPlan.Add(@{ name = 'NVIDIA stats CSV sink'; args = @('--once','--stats','--sink','csv'); applicable = $true })
    }
    $testPlan.Add(@{ name = 'AMD provider once (includes current ADLX diagnostic output when exposed)'; args = @('--provider','amd','--once'); applicable = $true })
    $testPlan.Add(@{ name = 'AMD provider-native stats'; args = @('--provider','amd','--once','--stats'); applicable = $true })
    if ($hasAmd) { $testPlan.Add(@{ name = 'AMD provider watch'; args = @('--provider','amd','--watch','--interval','1000'); applicable = $true; timeout = $WatchSeconds }) }
    $testPlan.Add(@{ name = 'Intel provider once'; args = @('--provider','intel','--once'); applicable = $true })
    $testPlan.Add(@{ name = 'Intel provider-native stats'; args = @('--provider','intel','--once','--stats'); applicable = $true })
    if ($hasIntel) { $testPlan.Add(@{ name = 'Intel provider watch'; args = @('--provider','intel','--watch','--interval','1000'); applicable = $true; timeout = $WatchSeconds }) }
    if ($IncludeMqtt) {
        if ($MqttArguments.Count -eq 0) { throw '-IncludeMqtt requires explicit -MqttArguments for an existing safe local validation path.' }
        if (-not $hasNvidia) { throw '-IncludeMqtt requires detected NVIDIA hardware.' }
        $testPlan.Add(@{ name = 'NVIDIA MQTT watch using caller-supplied local validation arguments'; args = @('--watch','--interval','1000','--sink','mqtt') + $MqttArguments; applicable = $true; timeout = $WatchSeconds })
    }
    foreach ($entry in $testPlan) {
        $timeout = if ($entry.ContainsKey('timeout')) { [int]$entry.timeout } else { 30 }
        $result = Invoke-WtgCommand -TestNumber $testNumber -Name $entry.name -Arguments $entry.args -TimeoutSeconds $timeout
        $tests.Add($result); Add-EvidenceTest $result; $testNumber++
    }

    $adapterFacts = New-Object 'System.Collections.Generic.List[object]'
    foreach ($controller in @($videoControllers)) {
        $adapterFacts.Add([pscustomobject][ordered]@{
            Name = $controller.Name; PNPDeviceID = $controller.PNPDeviceID
            DriverVersion = $controller.DriverVersion; AdapterCompatibility = $controller.AdapterCompatibility
            VideoProcessor = $controller.VideoProcessor; VideoModeDescription = $controller.VideoModeDescription
            CurrentHorizontalResolution = $controller.CurrentHorizontalResolution
            CurrentVerticalResolution = $controller.CurrentVerticalResolution; AdapterRAM = $controller.AdapterRAM
            DriverDate = $controller.DriverDate; Status = $controller.Status; Availability = $controller.Availability
        })
    }
    $manifest = [ordered]@{
        artifact_kind = 'wtg_validation_raw_evidence'
        evidence_format = [ordered]@{ schema = 'wtg.validation.raw-evidence.v1' }
        harness = [ordered]@{ harness_name = $harnessScriptName; harness_sha256 = $harnessScriptSha256; directory = $harnessDirectory; git_metadata = $gitMetadata }
        run = [ordered]@{ package_name = $packageName; hostname = $hostname; started_utc = $versionProbe.start_utc; completed_utc = (Get-UtcNow) }
        system = [ordered]@{ manufacturer = $computerSystem.Manufacturer; model = $computerSystem.Model; windows_caption = $operatingSystem.Caption; windows_version = $operatingSystem.Version; windows_build = $operatingSystem.BuildNumber; os_architecture = $operatingSystem.OSArchitecture; powershell_version = $PSVersionTable.PSVersion.ToString(); process_architecture = [Environment]::Is64BitProcess; environment = @{ COMPUTERNAME = $env:COMPUTERNAME; PROCESSOR_ARCHITECTURE = $env:PROCESSOR_ARCHITECTURE } }
        wtg = [ordered]@{ path = $script:ResolvedWtgPath; location = 'adjacent_to_harness'; relative_path = 'wtg.exe'; binary_sha256 = (Get-FileHash -LiteralPath $script:ResolvedWtgPath -Algorithm SHA256).Hash; version_stdout = $versionProbe.stdout; version_stderr = $versionProbe.stderr; version_exit_code = $versionProbe.exit_code }
        adapters = $adapterFacts.ToArray()
        capability_observations = [ordered]@{ nvidia_hardware_detected = $hasNvidia; amd_hardware_detected = $hasAmd; intel_hardware_detected = $hasIntel; runtime_observations = $runtime; mqtt_requested = [bool]$IncludeMqtt }
        tests = $tests.ToArray()
    }
    $summary = @(
        'WTG validation run', "Package: $packageName", "Host: $hostname", "System: $($computerSystem.Manufacturer) $($computerSystem.Model)", "Windows: $($operatingSystem.Caption) $($operatingSystem.Version) build $($operatingSystem.BuildNumber)", "Evidence schema: $($manifest.evidence_format.schema)", "WTG: $($script:ResolvedWtgPath)", "WTG SHA256: $($manifest.wtg.binary_sha256)", "Harness name: $($manifest.harness.harness_name)", "Harness SHA256: $($manifest.harness.harness_sha256)", "Git metadata availability: $($gitMetadata.availability)", "WTG git commit: $($gitMetadata.commit)", '', 'Detected adapters:'
    ) + @($manifest.adapters | ForEach-Object { "- $($_.Name) | Driver: $($_.DriverVersion) | PNP: $($_.PNPDeviceID)" }) + @('', 'Provider/runtime observations:', "- NVIDIA hardware: $hasNvidia", "- AMD hardware: $hasAmd", "- Intel hardware: $hasIntel", "- NVIDIA-SMI: $($runtime.nvidia_smi_path -join '; ')", "- AMD ADL DLLs: $($runtime.amd_adl_dll_paths -join '; ')", "- AMD ADLX DLLs: $($runtime.amd_adlx_dll_paths -join '; ')", "- Intel Level Zero DLLs: $($runtime.intel_level_zero_dll_paths -join '; ')", '', 'Tests executed:') + @($tests | ForEach-Object { "- $($_.id): exit $($_.exit_code); duration_ms $($_.duration_ms); timed_out $($_.timed_out); $($_.name)" }) + @('', 'Harness-level observations:', '- This package preserves raw provider and WTG output. It does not judge unusual telemetry values or assert cross-provider equivalence.', '- Watch tests are intentionally terminated after the requested capture period; that termination is recorded above.')
    [IO.File]::WriteAllText((Join-Path $script:PackageDirectory 'summary.txt'), ($summary -join [Environment]::NewLine), [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText((Join-Path $script:PackageDirectory 'evidence.txt'), ($script:Evidence -join [Environment]::NewLine), [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText((Join-Path $script:PackageDirectory 'manifest.json'), ($manifest | ConvertTo-Json -Depth 12), [Text.UTF8Encoding]::new($false))
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [IO.Compression.ZipFile]::CreateFromDirectory($script:PackageDirectory, $packagePath)
    Write-Output $packagePath
}
finally {
    if (Test-Path -LiteralPath $script:WorkDirectory) { Remove-Item -LiteralPath $script:WorkDirectory -Recurse -Force }
}
