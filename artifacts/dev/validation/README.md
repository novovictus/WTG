# WTG 0.3.1 Validation Harness

`Invoke-WtgValidation.ps1` is the 0.3.1 validation campaign's raw evidence collector. Its workflow is **discover -> invoke -> capture -> preserve**. It is deliberately not a telemetry interpreter, quality scorer, or cross-vendor normalizer.

## Use

Place these two files in the same folder, anywhere on the machine:

```powershell
<any folder>\
    wtg.exe
    Invoke-WtgValidation.ps1
```

Then run, for example from an unzipped Desktop folder:

```powershell
.\Invoke-WtgValidation.ps1
```

The harness resolves its own directory, requires `wtg.exe` in that exact directory, and hashes and executes only that adjacent binary. It does not look in a repository, parent directory, Cargo output, or any other location. It requires neither Rust, Cargo, source files, nor a Git checkout. Git metadata is opportunistic only; when unavailable, that raw state is recorded as `not_available` and the capture continues. Watch capture defaults to three seconds and records that the harness terminated the watch process. MQTT is disabled by default; it can be requested only with `-IncludeMqtt` and explicit `-MqttArguments` for an already-existing safe local broker/subscriber validation path. The harness never creates broker infrastructure.

## Artifact layout

Runs are written beside the script, under `results/`, as exactly one package:

```text
wtg_validation_<hostname>_<yyyyMMdd-HHmmss>.zip
```

Each ZIP contains only:

```text
summary.txt
evidence.txt
manifest.json
```

`evidence.txt` is the complete test transcript with exact stdout, stderr, timestamps, commands, arguments, exit codes, and raw sink content. `manifest.json` distinguishes WTG identity (raw `wtg.exe --version` output and binary SHA256), harness identity (the executing script's name and SHA256), and the evidence-format schema (`wtg.validation.raw-evidence.v1`). It also records opportunistic Git metadata state, detected CIM adapters and PNP identifiers, capability observations, commands, and captured output. The harness has no separate version string. `summary.txt` is the compact human-facing index.

## Capability-aware behavior

The harness inventories CIM video adapters and observed provider runtime paths before selecting optional tests. It always captures the provider once/stats invocations so WTG's own unavailable result is preserved when hardware or runtime is absent. NVIDIA watch, probes, probe-fields, JSONL, and CSV sink tests are included only when NVIDIA hardware is detected. AMD and Intel watch tests are included only for their detected hardware. Current AMD once output includes ADLX diagnostic/runtime output where WTG exposes it.

Provider absence or a runtime-unavailable result is evidence, not a telemetry failure. The harness records it without inventing a shared provider status or comparison model.

## Trust model and exclusions

The harness records execution-level facts: process start/end, exit codes, output, package structure, JSON validity, and raw provider-native values. It does not decide whether a surprising value is correct. For example, an idle `util.mem_controller_pct = 100` is preserved exactly.

It does not automate screenshots, change WTG behavior, rename native fields, translate AMD or Intel data into NVIDIA vocabulary, create a hardware matrix, or modify historical matrix data. Existing `artifacts/dev/wtg_test.ps1` and `artifacts/dev/wtg_providers_test.ps1` remain the historical/lightweight tools. This broader campaign harness complements them; it does not replace them.

A future separate matrix parser should consume `manifest.json` and evidence packages. That parser—not this collector—may construct current or historical views.
