# NVML Provenance Stats

WTG 0.2.7 establishes `--once --stats` as the expanded NVIDIA/NVML provider-truth surface.

## Command

```powershell
.\wtg.exe --once --stats
```

## Payload identity

`--once --stats` emits structured JSON using schema:

```text
wtg.nvml.stats.v1
```

Top-level provider identity:

```text
provider: nvidia
provider_source: nvidia.nvml
provider_authority: NVIDIA NVML
telemetry_class: provider_truth
```

WTG keeps the user-facing provider name vendor-simple while preserving the exact NVML source authority inside the payload.

## Fact shape

Each fact reports raw provider data with source and state metadata:

```text
source_api
state
raw
unit
error_message
```

`error_message` is present only when the query fails, is unavailable, or is unsupported.

## State values

```text
ok
unsupported
not_available
permission_denied
error
```

Unsupported and unavailable facts are emitted explicitly with `raw: null`, `source_api`, `state`, and, when available, `error_message`.

## Expanded groups

The expanded NVML stats payload includes:

```text
identity
pcie
clocks
bar1
power_management
media
cooling
processes
```

The base device facts remain present, including device identity, memory, utilization, temperature, and power facts.

## Raw provider-truth boundary

`--stats` reports raw provider values. It does not normalize values for dashboard presentation.

Visualization-oriented normalization belongs in downstream consumers such as Redline, Home Assistant dashboards, or the UI.

## Sink behavior

`--once --stats` prints pretty JSON to stdout.

`--once --stats --sink jsonl` writes one compact canonical provenance JSON object to the timestamped JSONL sink and still prints pretty JSON to stdout.

`--once --stats --sink csv` keeps legacy flat stats CSV behavior and still prints pretty JSON to stdout.

`--watch --stats` remains legacy for this release.

No `--out` flag is added. Sink files use existing timestamped sink naming.
