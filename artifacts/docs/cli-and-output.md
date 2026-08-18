# CLI and Structured Output

`wtg.exe` is the reference surface for telemetry capture, provider validation, probes, and structured output.

## Core modes

```powershell
.\wtg.exe --once
.\wtg.exe --watch --interval 1000
.\wtg.exe --once --stats
.\wtg.exe --probe
.\wtg.exe --probe-fields --field-id 74
```

The default path uses NVIDIA NVML. AMD and Intel paths must be selected explicitly with `--provider amd` or `--provider intel`.

## NVML provenance stats

`--once --stats` emits provider-truth JSON using schema `wtg.nvml.stats.v1`.

```text
provider: nvidia
provider_source: nvidia.nvml
provider_authority: NVIDIA NVML
telemetry_class: provider_truth
```

Each fact preserves:

```text
source_api
state
raw
unit
error_message
```

Supported state values include `ok`, `unsupported`, `not_available`, `permission_denied`, and `error`. Unavailable facts remain present with `raw: null` rather than being omitted or converted to zero.

Expanded categories include identity, PCIe, clocks, BAR1, power management, media, cooling, and processes, in addition to the base identity, memory, utilization, temperature, and power facts.

## Probe and field diagnostics

`--probe` and `--probe-fields` include context used for same-GPU, cross-driver comparison:

- WTG version
- driver and CUDA driver versions
- compute mode
- performance state
- PCI bus ID

`util.mem_controller_pct` is NVML memory-controller utilization. It is not VRAM occupancy. VRAM occupancy is reported separately through `vram.used_mib` and `vram.total_mib`.

`--probe-fields` compares the normal NVML utilization path against selected `nvmlDeviceGetFieldValues` results. A callable field-values API does not by itself establish driver causality; cross-driver evidence requires the same capture procedure on the same hardware.

## File sinks

```powershell
.\wtg.exe --once --sink jsonl
.\wtg.exe --once --sink csv
.\wtg.exe --probe --sink jsonl
.\wtg.exe --probe-fields --field-id 74 --sink csv
.\wtg.exe --once --stats --sink jsonl
.\wtg.exe --once --stats --sink csv
```

JSONL and CSV sinks create timestamped files. `--once --stats --sink jsonl` writes one compact canonical provenance object. CSV retains the legacy flat stats representation for this release.

## Support matrix

| Mode | JSONL | CSV | MQTT |
| --- | --- | --- | --- |
| `--once` | yes | yes | no |
| `--once --stats` | yes | yes | no |
| `--watch` | yes | yes | yes |
| `--watch --stats` | yes | yes | yes |
| `--probe` | yes | yes | no |
| `--probe-fields` | yes | yes | no |
| AMD/Intel provider modes | no | no | no |

MQTT is documented separately because it is a live transport and discovery surface rather than a file sink.
