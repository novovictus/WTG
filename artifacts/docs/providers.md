# Provider Surfaces

WTG preserves the meaning, source, state, unit, and availability behavior reported by each provider. It does not translate unlike provider fields into false cross-vendor equivalents.

## Provider roles

- **NVIDIA NVML:** primary/default provider and formal validation reference path.
- **AMD ADL:** experimental provider-native telemetry selected with `--provider amd`.
- **Intel Level Zero/Sysman:** experimental provider-native telemetry selected with `--provider intel`.

## Commands

```powershell
.\target\release\wtg.exe --provider amd --once
.\target\release\wtg.exe --provider amd --watch --interval 1000
.\target\release\wtg.exe --provider amd --once --stats

.\target\release\wtg.exe --provider intel --once
.\target\release\wtg.exe --provider intel --watch --interval 1000
.\target\release\wtg.exe --provider intel --once --stats
```

The raw diagnostic probe may expose first-sample unavailable states that are intentionally hidden from primed user-facing Intel once/watch/stats output.

## Provider-scoped schemas

```text
NVIDIA: wtg.nvml.stats.v1
AMD:    wtg.amd_adl.stats.v1
Intel:  wtg.intel_level_zero.stats.v1
```

NVIDIA expanded stats use telemetry class `provider_truth`. AMD and Intel use provider-scoped telemetry class `provider_telemetry`.

## Truthfulness rules

- Preserve unsupported, unavailable, permission-denied, and error states.
- Do not render missing values as zero.
- Do not render missing AMD adapter activity as `no`; render it as unavailable.
- Intel power and engine utilization require valid advancing counter timestamps.
- Backward Intel counters are unavailable counter-reset evidence, not zero activity.
- Equal Intel counters with an advancing timestamp are valid idle and may resolve to `0.0`.
- Do not add `delta` to human-facing Intel power or engine labels.

## Runtime requirements

AMD ADL requires a usable `atiadlxx.dll`. Intel requires a usable Level Zero/Sysman runtime. Provider runtime failure or absent matching hardware is reported as provider-scoped unavailable output and is not treated as an NVIDIA/NVML failure.

## Current boundaries

AMD and Intel provider output supports provider-scoped once, watch, and stats behavior. File sinks, MQTT publishing, and Home Assistant discovery are not implemented for these providers in this release candidate.

The experimental UI may display telemetry-capable NVIDIA, AMD, and Intel devices in one device list. It must not add topology-only duplicate rows, fake gauges, or synthetic cross-vendor parity fields.
