WTG is licensed under the GNU General Public License v3.0 (GPLv3). See LICENSE.
Copyright (C) 2026 Adam Hooper

# WTG - WhatTheGPU

**Tagline:** Honest GPU compute stats for Windows

WTG is a Windows-native GPU telemetry and validation tool focused on NVIDIA/NVML provider truth. It exposes driver-reported GPU telemetry from a standalone Windows executable without depending on Task Manager, PerfMon, Docker, Python, or `nvidia-smi` parsing.

## Current Status

Current branch: `dev/0.2.9-intel-level-zero-provider`

WTG 0.2.9 is the Intel Level Zero/Sysman provider exploration cycle. WTG remains NVIDIA/NVML-centric; NVIDIA/NVML is still the primary truth provider and the only provider with the stable expanded `--once --stats` JSON surface.

AMD ADL is the completed 0.2.8 experimental provider path selected with `--provider amd`. Intel Level Zero is the active 0.2.9 experimental provider path selected with `--provider intel`. AMD output is provider-scoped, read-only, and intentionally not translated into NVML field names or cross-vendor equivalence claims. The AMD path currently supports compact snapshot and watch output only.

Current development builds include:

- `wtg.exe`, the CLI validation, capture, probe, sink, MQTT runtime, and experimental provider surface
- `wtg-ui.exe`, an experimental egui viewer/configurator/launcher
- optional CSV and JSONL sinks for NVIDIA/NVML paths
- optional MQTT watch publishing for NVIDIA/NVML watch snapshots
- optional Home Assistant MQTT discovery for NVIDIA/NVML MQTT watch publishing
- explicit opt-in TOML configuration
- expanded `--once --stats` NVIDIA/NVML provenance JSON
- experimental AMD ADL `--provider amd --once` and `--provider amd --watch` output

WTG does not auto-create `wtg.toml`, does not auto-load `wtg.toml`, does not configure an MQTT broker, and does not expose a listening network service.

See `artifacts/test-matrix/matrix.md` for empirical GPU and driver results.

## Project Purpose

Windows exposes GPU information through multiple abstraction layers. Task Manager and Windows performance counters report WDDM scheduler-level views. WTG focuses on NVIDIA/NVML driver-reported device telemetry.

Existing alternatives are often:

- Linux-first tools
- wrappers around `nvidia-smi`
- generic Windows GPU monitors
- dashboards that interpret or normalize telemetry before preserving raw provider context

WTG provides a Windows-native validation surface for inspecting NVIDIA/NVML telemetry directly and preserving the provider source behind the observed values.

See: `artifacts/abstraction-model/wtg_vs_task_manager_abstraction_model.md`

## Empirical Findings

Current regression research has shown:

- 576.88 stable baseline behavior
- 580.88 branch inflection observed
- persistent NVML memory-utilization anomaly on consumer mobile Ampere
- no reproduction on tested desktop or professional SKUs

## Project Scope

- **Primary provider:** NVIDIA/NVML
- **Experimental side provider:** AMD ADL, selected explicitly with `--provider amd`
- **Platform:** Windows-native CLI/runtime binary, with an experimental separate desktop UI binary
- **Reference surface:** `wtg.exe`
- **Experimental visual surface:** `wtg-ui.exe`
- **Transport surface:** optional MQTT watch publishing for NVIDIA/NVML snapshots
- **Validation artifacts:** CLI output, CSV/JSONL sinks, probe output, field-values output, provider output, and packaged diagnostic captures

Formal validation remains CLI/probe/sink/provider based. The egui UI is an experimental viewer/configurator/launcher, not the validation reference surface.

## Quick Start

Build release binaries:

```powershell
cargo build -p wtg-app --release
```

Run a concise NVIDIA/NVML one-shot snapshot:

```powershell
.\target\release\wtg.exe --once
```

Run a probe snapshot:

```powershell
.\target\release\wtg.exe --probe
```

Run expanded NVIDIA/NVML provenance stats:

```powershell
.\target\release\wtg.exe --once --stats
```

Run experimental AMD ADL snapshot output:

```powershell
.\target\release\wtg.exe --provider amd --once
```

Run experimental AMD ADL watch output:

```powershell
.\target\release\wtg.exe --provider amd --watch --interval 1000
```

Write expanded NVIDIA/NVML provenance stats to JSONL:

```powershell
.\target\release\wtg.exe --once --stats --sink jsonl
```

Write legacy flat NVIDIA/NVML stats CSV:

```powershell
.\target\release\wtg.exe --once --stats --sink csv
```

Run the experimental UI:

```powershell
.\target\release\wtg-ui.exe
```

## Core Commands

- `--once`  
  Capture one concise NVIDIA/NVML snapshot and exit.

- `--watch`  
  Continuously poll NVIDIA/NVML GPU state at a fixed interval. Default interval is 1000 ms.

- `--interval <ms>`  
  Set the watch polling interval in milliseconds.

- `--stats`  
  With NVIDIA/NVML `--once`, emit expanded provider-truth JSON using schema `wtg.nvml.stats.v1`. With NVIDIA/NVML `--watch`, stats remains on the existing legacy path for this release. AMD and Intel provider `--stats` output is provider-scoped and not NVML-equivalent.

- `--probe`  
  Capture one compact NVIDIA/NVML probe block for driver and field validation.

- `--probe-fields`  
  Query explicit NVIDIA/NVML field IDs for diagnostic comparison.

- `--field-id <u32>`  
  Repeatable field ID argument for `--probe-fields`.

- `--sink jsonl`  
  Create a timestamped JSONL sink file for supported NVIDIA/NVML modes. AMD ADL sinks are intentionally rejected.

- `--sink csv`  
  Create a timestamped CSV sink file for supported NVIDIA/NVML modes. AMD ADL sinks are intentionally rejected.

- `--sink mqtt`  
  Publish live NVIDIA/NVML `--watch` snapshots to an existing MQTT broker. AMD ADL MQTT publishing is intentionally not implemented.

- `--config <path>`  
  Load an explicit WTG TOML configuration file.

- `--provider amd`  
  Select the experimental AMD ADL provider for compact `--once` or `--watch` output. AMD and Intel output use provider-scoped schemas and telemetry class `provider_telemetry`.

## AMD ADL Discovery

`--provider amd` is the experimental AMD ADL provider path for this branch.

Supported commands:

```powershell
.\target\release\wtg.exe --provider amd --once
.\target\release\wtg.exe --provider amd --watch --interval 1000
```

Intentional boundaries:

```text
AMD --stats: rejected
AMD sinks: rejected
AMD MQTT/Home Assistant publishing: not implemented
AMD output schema: no stable schema field
AMD telemetry class: provider_telemetry
```

The AMD provider preserves ADL-native facts beside NVIDIA/NVML behavior. It does not translate ADL facts into NVML-equivalent names, does not synthesize missing values, and does not claim cross-vendor parity.

Detailed notes: [AMD ADL discovery](docs/amd-adl-discovery.md)

## NVML Provenance Stats

`--once --stats` is the expanded NVIDIA/NVML provider-truth surface.

Top-level identity:

```text
schema: wtg.nvml.stats.v1
provider: nvidia
provider_source: nvidia.nvml
provider_authority: NVIDIA NVML
telemetry_class: provider_truth
```

Fact objects use this shape:

```text
source_api
state
raw
unit
error_message
```

`--stats` reports raw provider values. It does not normalize values for dashboards. Redline, Home Assistant, and UI surfaces are responsible for visualization-oriented normalization.

Expanded NVML categories currently include:

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

Unsupported or unavailable NVML facts are emitted explicitly with `raw: null`, a state such as `unsupported` or `not_available`, and an error message when available.

Detailed notes: [NVML provenance stats](docs/nvml-provenance-stats.md)

## Sink Summary

| Mode | JSONL | CSV | MQTT | Notes |
| --- | --- | --- | --- | --- |
| `--once` | yes | yes | no | concise NVIDIA/NVML snapshot |
| `--once --stats` | yes | yes | no | JSONL writes compact NVIDIA/NVML provenance JSON; CSV remains legacy flat stats |
| `--watch` | yes | yes | yes | MQTT publishes live NVIDIA/NVML snapshot payloads |
| `--watch --stats` | yes | yes | yes | legacy NVIDIA/NVML stats/watch behavior for this release |
| `--provider amd --once` | no | no | no | compact AMD ADL provider telemetry |
| `--provider amd --watch` | no | no | no | compact AMD ADL live provider telemetry |
| `--probe` | yes | yes | no | NVIDIA/NVML validation output |
| `--probe-fields` | yes | yes | no | NVIDIA/NVML field-ID diagnostics |

Detailed notes: [Sinks](docs/sinks.md)

## MQTT and Home Assistant

The MQTT sink publishes live NVIDIA/NVML telemetry from `--watch` to a user-specified broker.

Example:

```powershell
.\wtg.exe --watch --sink mqtt --mqtt-host 127.0.0.1 --mqtt-port 1884 --mqtt-node-id testnode
```

WTG is an MQTT publisher, not a broker. MQTT validates transport only; it is not the telemetry source of truth. AMD ADL provider output is not currently published to MQTT or Home Assistant.

Detailed notes: [MQTT and Home Assistant](docs/mqtt-home-assistant.md)

## Configuration

WTG supports an explicit TOML configuration file for MQTT and Home Assistant settings.

Configuration is opt-in:

- WTG does not auto-create a config file
- WTG does not auto-load `wtg.toml`
- `--config <path>` is required to load configuration
- CLI flags override config values
- normal non-MQTT commands remain unaffected

Detailed notes: [Configuration](docs/configuration.md)

## eGUI

`wtg-ui.exe` is an experimental egui desktop frontend.

It displays live NVIDIA/NVML telemetry and provides a convenience layer over the same explicit TOML configuration model used by the CLI.

The UI is not the reference surface for regression testing or metric capture.

Detailed notes:

- [eGUI](docs/egui.md)
- [Windows application control notes](docs/windows-app-control.md)

## Probe and Field Diagnostics

`--probe` and `--probe-fields` are diagnostic NVIDIA/NVML validation surfaces used for same-GPU, cross-driver comparisons.

Important distinction:

```text
util.mem_controller_pct = NVML memory-controller utilization
vram.used_mib / vram.total_mib = VRAM occupancy
```

A pinned `util.mem_controller_pct` value does not mean VRAM is full.

Detailed notes: [Probe and probe-fields](docs/probe-fields.md)

## Development Artifacts

Development and research helpers are documented separately:

- [Packaging checkpoint helper](artifacts/dev/packaging-checkpoint.md)
- [NVIDIA bug-report collector](artifacts/dev/nvidia-bug-report.md)

## Architecture Summary

Current source layout:

```text
wtg-core/
  NVIDIA/NVML context, snapshots, probe context, field queries, and provenance stats

wtg-app/
  CLI, sinks, MQTT, config loading, JSON/CSV/JSONL formatting, and explicit provider routing

wtg-app/src/bin/wtg-ui.rs
  experimental egui UI entrypoint

wtg-view/
  shared view helpers where useful

wtg-providers/
  experimental provider-side work, including AMD ADL discovery
```

`wtg-app` does not own NVML provider querying. Expanded NVML provenance collection belongs in `wtg-core`. Experimental non-NVIDIA provider work remains provider-scoped under `wtg-providers` and is routed through `wtg.exe` only when explicitly selected.

## Driver Requirements

WTG relies on NVIDIA NVML on Windows for the default provider path.

Empirical testing shows:

- drivers prior to roughly 470 may ship `nvidia-smi` without a usable `nvml.dll`
- modern tested drivers expose NVML across tested SKUs
- individual counters may still show driver/platform-specific behavior
- WTG fails fast when NVML is unavailable and prints an explicit error when possible

AMD ADL discovery relies on `atiadlxx.dll` being available on the test system. Unsupported, unavailable, or failing ADL calls are reported as provider-scoped facts and are not treated as NVIDIA/NVML failures.

## Validation Boundary

Formal validation should use:

```powershell
cargo fmt --check
cargo test
cargo build -p wtg-app --release
.\target\release\wtg.exe --once
.\target\release\wtg.exe --watch --interval 1000
.\target\release\wtg.exe --once --stats
.\target\release\wtg.exe --once --stats --sink jsonl
.\target\release\wtg.exe --once --stats --sink csv
.\target\release\wtg.exe --provider amd --once
.\target\release\wtg.exe --provider amd --watch --interval 1000
.\target\release\wtg.exe --probe
.\target\release\wtg.exe --probe --sink jsonl
.\target\release\wtg.exe --probe --sink csv
.\target\release\wtg.exe --probe-fields --field-id 74
```

Use MQTT broker/subscriber captures to validate transport only. Do not treat MQTT, Home Assistant, AMD ADL, or the eGUI as the NVIDIA/NVML telemetry source of truth.

For automated watch checks, avoid `Select-Object -First ...` as final evidence because it intentionally closes stdout and can produce a broken-pipe panic after the selected lines are captured. Manual `Ctrl+C` watch termination is cleaner for final run notes.

## Validation Strategy

- Compare WTG CLI output against `nvidia-smi` and WSL NVML metrics where useful.
- Compare NVML telemetry against Windows-reported metrics to characterize abstraction differences.
- Use CLI, probe, field-values, provider output, and sink artifacts for validation evidence.
- Use MQTT broker/subscriber captures to validate transport only.
- Use the experimental UI for visual corroboration and demos only.
- Treat laptop power source as a controlled variable when interpreting hybrid GPU telemetry.

## Milestones

| Version / Branch | Goal |
| --- | --- |
| v0.1.x | Initial NVIDIA/NVML truth-layer validation |
| v0.2.0-beta4 | Probe, sink, field-values, and driver-behavior validation |
| v0.2.0-beta5 | Experimental dual-surface build with `wtg.exe` and `wtg-ui.exe` |
| dev/0.2.1 | Experimental MQTT watch sink |
| dev/0.2.2 | Optional Home Assistant MQTT discovery |
| dev/0.2.3 | MQTT auth, retained HA availability, retained discovery cleanup |
| v0.2.4 | Explicit TOML config support |
| dev/0.2.6 | Experimental AMD ADL provider foundation |
| dev/0.2.7 | NVML provenance and expanded NVIDIA stats |
| dev/0.2.8 | AMD ADL discovery routed through `wtg.exe --provider amd` |
| v0.3+ | Optional UI and distribution hardening |

## Next Immediate Step

- Keep 0.2.9 scoped to Intel Level Zero/Sysman provider-boundary evidence.
- Capture full-power AC versus reduced USB-C behavior as a later validation cycle.
- Keep NVIDIA/NVML as the primary truth provider while ADL remains provider-scoped experimental telemetry.
- Keep README as a project entry point and maintain detailed operational docs under `docs/` and `artifacts/dev/`.
