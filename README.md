WTG is licensed under the GNU General Public License v3.0 (GPLv3). See LICENSE.
Copyright (C) 2026 Adam Hooper

# WTG - WhatTheGPU

**Tagline:** Honest GPU compute stats for Windows

WTG is a Windows-native GPU telemetry and validation tool focused on NVIDIA/NVML provider truth. It exposes driver-reported GPU telemetry from a standalone Windows executable without depending on Task Manager, PerfMon, Docker, Python, or `nvidia-smi` parsing.

## Current Status

Current branch: `dev/0.2.7`

WTG 0.2.7 is the NVML provenance and expanded NVIDIA stats cycle.

WTG remains NVIDIA/NVML-centric. AMD ADL support exists as an explicit provider foundation from 0.2.6, but deeper ADL expansion is deferred until after the NVIDIA/NVML truth layer is stable.

Current development builds include:

- `wtg.exe`, the CLI validation, capture, probe, sink, and MQTT runtime surface
- `wtg-ui.exe`, an experimental egui viewer/configurator/launcher
- optional CSV and JSONL sinks
- optional MQTT watch publishing
- optional Home Assistant MQTT discovery
- explicit opt-in TOML configuration
- expanded `--once --stats` NVIDIA/NVML provenance JSON

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
- **Platform:** Windows-native CLI/runtime binary, with an experimental separate desktop UI binary
- **Reference surface:** `wtg.exe`
- **Experimental visual surface:** `wtg-ui.exe`
- **Transport surface:** optional MQTT watch publishing
- **Validation artifacts:** CLI output, CSV/JSONL sinks, probe output, field-values output, and packaged diagnostic captures

Formal validation remains CLI/probe/sink based. The egui UI is an experimental viewer/configurator/launcher, not the validation reference surface.

## Quick Start

Build release binaries:

```powershell
cargo build -p wtg-app --release
```

Run a concise one-shot snapshot:

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

Write expanded provenance stats to JSONL:

```powershell
.\target\release\wtg.exe --once --stats --sink jsonl
```

Write legacy flat stats CSV:

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
  Continuously poll GPU state at a fixed interval. Default interval is 1000 ms.

- `--interval <ms>`  
  Set the watch polling interval in milliseconds.

- `--stats`  
  With `--once`, emit expanded NVIDIA/NVML provider-truth JSON using schema `wtg.nvml.stats.v1`. With `--watch`, stats remains on the existing legacy path for this release.

- `--probe`  
  Capture one compact probe block for driver and field validation.

- `--probe-fields`  
  Query explicit NVML field IDs for diagnostic comparison.

- `--field-id <u32>`  
  Repeatable field ID argument for `--probe-fields`.

- `--sink jsonl`  
  Create a timestamped JSONL sink file.

- `--sink csv`  
  Create a timestamped CSV sink file.

- `--sink mqtt`  
  Publish live `--watch` snapshots to an existing MQTT broker.

- `--config <path>`  
  Load an explicit WTG TOML configuration file.

- `--provider amd`  
  Include the experimental AMD ADL side-provider output. Deeper AMD work is deferred to a later release.

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
| `--once` | yes | yes | no | concise snapshot |
| `--once --stats` | yes | yes | no | JSONL writes compact provenance JSON; CSV remains legacy flat stats |
| `--watch` | yes | yes | yes | MQTT publishes live snapshot payloads |
| `--watch --stats` | yes | yes | yes | legacy stats/watch behavior for this release |
| `--probe` | yes | yes | no | validation output |
| `--probe-fields` | yes | yes | no | field-ID diagnostics |

Detailed notes: [Sinks](docs/sinks.md)

## MQTT and Home Assistant

The MQTT sink publishes live telemetry from `--watch` to a user-specified broker.

Example:

```powershell
.\wtg.exe --watch --sink mqtt --mqtt-host 127.0.0.1 --mqtt-port 1884 --mqtt-node-id testnode
```

WTG is an MQTT publisher, not a broker. MQTT validates transport only; it is not the telemetry source of truth.

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

It displays live telemetry and provides a convenience layer over the same explicit TOML configuration model used by the CLI.

The UI is not the reference surface for regression testing or metric capture.

Detailed notes:

- [eGUI](docs/egui.md)
- [Windows application control notes](docs/windows-app-control.md)

## Probe and Field Diagnostics

`--probe` and `--probe-fields` are diagnostic validation surfaces used for same-GPU, cross-driver comparisons.

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
  CLI, sinks, MQTT, config loading, JSON/CSV/JSONL formatting

wtg-app/src/bin/wtg-ui.rs
  experimental egui UI entrypoint

wtg-view/
  shared view helpers where useful

wtg-providers/
  experimental provider-side work, including ADL foundation
```

`wtg-app` does not own NVML provider querying. Expanded NVML provenance collection belongs in `wtg-core`.

## Driver Requirements

WTG relies on NVIDIA NVML on Windows.

Empirical testing shows:

- drivers prior to roughly 470 may ship `nvidia-smi` without a usable `nvml.dll`
- modern tested drivers expose NVML across tested SKUs
- individual counters may still show driver/platform-specific behavior
- WTG fails fast when NVML is unavailable and prints an explicit error when possible

## Validation Boundary

Formal validation should use:

```powershell
cargo fmt --check
cargo test
cargo build -p wtg-app --release
.\target\release\wtg.exe --once
.\target\release\wtg.exe --once --stats
.\target\release\wtg.exe --once --stats --sink jsonl
.\target\release\wtg.exe --once --stats --sink csv
.\target\release\wtg.exe --probe
.\target\release\wtg.exe --probe --sink jsonl
.\target\release\wtg.exe --probe --sink csv
.\target\release\wtg.exe --probe-fields --field-id 74
```

Use MQTT broker/subscriber captures to validate transport only. Do not treat MQTT, Home Assistant, or the eGUI as the telemetry source of truth.

## Validation Strategy

- Compare WTG CLI output against `nvidia-smi` and WSL NVML metrics where useful.
- Compare NVML telemetry against Windows-reported metrics to characterize abstraction differences.
- Use CLI, probe, field-values, and sink artifacts for validation evidence.
- Use MQTT broker/subscriber captures to validate transport only.
- Use the experimental UI for visual corroboration and demos only.

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
| v0.2.8+ | Deeper AMD provider work |
| v0.3+ | Optional UI and distribution hardening |

## Next Immediate Step

- Close 0.2.7 around NVML provenance and expanded NVIDIA stats.
- Keep AMD/ADL expansion in 0.2.8 or later.
- Keep README as a project entry point and maintain detailed operational docs under `docs/` and `artifacts/dev/`.
