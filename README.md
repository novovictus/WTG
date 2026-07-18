WTG is licensed under the GNU General Public License v3.0 (GPLv3). See LICENSE.
Copyright (C) 2026 Adam Hooper

# WTG - WhatTheGPU

**Tagline:** Provider-native GPU telemetry and validation for Windows

WTG is a Windows-native GPU telemetry and validation suite that preserves what each hardware provider actually reports. NVIDIA/NVML remains the primary reference and validation path, while WTG 0.3.0 adds experimental AMD ADL and Intel Level Zero adapter surfaces without normalizing unlike provider fields into false cross-vendor equivalents.

WTG includes a standalone CLI reference tool and an experimental desktop adapter viewer. It can capture live telemetry, provenance, probe results, structured output, and optional MQTT delivery without relying on Task Manager, PerfMon, Python, Docker, or parsing `nvidia-smi` output.

## Current Status

Current release candidate: `release/v0.3.0-rc1`

WTG 0.3.0 is the eGUI provider-backed adapter view workstream. WTG remains NVIDIA/NVML-centric; NVIDIA/NVML is still the primary truth provider and the default eGUI device path.

The 0.3.0 eGUI work keeps the existing Devices pane and right-side detail model while allowing telemetry-capable devices from NVIDIA NVML, AMD ADL, and Intel Level Zero to appear in the same selectable device list. AMD and Intel output remain provider-scoped and are intentionally not translated into NVML field names or cross-vendor equivalence claims.

Current development builds include:

- `wtg.exe`, the CLI validation, capture, probe, sink, MQTT runtime, and experimental provider surface
- `wtg-ui.exe`, an experimental egui viewer/configurator/launcher
- optional CSV and JSONL sinks for NVIDIA/NVML paths
- optional MQTT watch publishing for NVIDIA/NVML watch snapshots
- optional Home Assistant MQTT discovery for NVIDIA/NVML MQTT watch publishing
- explicit opt-in TOML configuration
- expanded `--once --stats` NVIDIA/NVML provenance JSON
- experimental AMD ADL and Intel Level Zero provider output through `--provider amd` and `--provider intel`

WTG does not auto-create `wtg.toml`, does not auto-load `wtg.toml`, does not configure an MQTT broker, and does not expose a listening network service.

See `artifacts/test-matrix/matrix.md` for empirical GPU and driver results.

## Project Purpose

Windows exposes GPU information through multiple abstraction layers. Task Manager and Windows performance counters report WDDM scheduler-level views. WTG focuses on provider-reported device telemetry, with NVIDIA/NVML as the primary validation path and AMD ADL and Intel Level Zero as experimental provider-native paths.

Existing alternatives are often:

- Linux-first tools
- wrappers around `nvidia-smi`
- generic Windows GPU monitors
- dashboards that interpret or normalize telemetry before preserving raw provider context

WTG provides a Windows-native validation surface for inspecting provider telemetry directly and preserving the provider source behind the observed values.

See: `artifacts/abstraction-model/wtg_vs_task_manager_abstraction_model.md`

## Empirical Findings

Current regression research has shown:

- 576.88 stable baseline behavior
- 580.88 branch inflection observed
- persistent NVML memory-utilization anomaly on consumer mobile Ampere
- no reproduction on tested desktop or professional SKUs

## Project Scope

- **Primary provider:** NVIDIA/NVML
- **Experimental providers:** AMD ADL and Intel Level Zero, selected explicitly with `--provider amd` or `--provider intel`
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

Run experimental AMD ADL output:

```powershell
.\target\release\wtg.exe --provider amd --once
.\target\release\wtg.exe --provider amd --watch --interval 1000
```

Run experimental Intel Level Zero output:

```powershell
.\target\release\wtg.exe --provider intel --once
.\target\release\wtg.exe --provider intel --watch --interval 1000
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
  Create a timestamped JSONL sink file for supported NVIDIA/NVML modes. AMD ADL and Intel Level Zero sinks are intentionally rejected.

- `--sink csv`  
  Create a timestamped CSV sink file for supported NVIDIA/NVML modes. AMD ADL and Intel Level Zero sinks are intentionally rejected.

- `--sink mqtt`  
  Publish live NVIDIA/NVML `--watch` snapshots to an existing MQTT broker. AMD ADL and Intel Level Zero MQTT publishing are intentionally not implemented.

- `--config <path>`  
  Load an explicit WTG TOML configuration file.

- `--provider amd`  
  Select the experimental AMD ADL provider for compact `--once`, `--watch`, or provider-scoped `--stats` output.

- `--provider intel`  
  Select the experimental Intel Level Zero provider for compact `--once`, `--watch`, or provider-scoped `--stats` output.

AMD and Intel output use provider-scoped schemas and telemetry class `provider_telemetry`.

## Experimental AMD and Intel Providers

`--provider amd` and `--provider intel` are the experimental non-NVIDIA provider paths for this release candidate.

Supported commands:

```powershell
.\target\release\wtg.exe --provider amd --once
.\target\release\wtg.exe --provider amd --watch --interval 1000
.\target\release\wtg.exe --provider intel --once
.\target\release\wtg.exe --provider intel --watch --interval 1000
```

Intentional boundaries:

```text
AMD --stats: provider-scoped JSON stats/provenance
Intel --stats: provider-scoped JSON stats/provenance
AMD and Intel sinks: rejected
AMD and Intel MQTT/Home Assistant publishing: not implemented
AMD output schema: wtg.amd_adl.stats.v1
Intel output schema: wtg.intel_level_zero.stats.v1
AMD and Intel telemetry class: provider_telemetry
```

The AMD and Intel providers preserve provider-native facts beside NVIDIA/NVML behavior. They do not translate ADL or Level Zero facts into NVML-equivalent names, synthesize missing values, or claim cross-vendor parity.

Detailed notes:

- [AMD ADL discovery](docs/amd-adl-discovery.md)
- [Provider discovery 0.3](docs/provider-discovery-0.3.md)

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
| `--provider intel --once` | no | no | no | compact Intel Level Zero provider telemetry |
| `--provider intel --watch` | no | no | no | compact Intel Level Zero live provider telemetry |
| `--probe` | yes | yes | no | NVIDIA/NVML validation output |
| `--probe-fields` | yes | yes | no | NVIDIA/NVML field-ID diagnostics |

Detailed notes: [Sinks](docs/sinks.md)

## MQTT and Home Assistant

WTG can publish live NVIDIA/NVML `--watch` snapshots to an existing MQTT broker and can publish retained Home Assistant MQTT discovery configuration for those snapshots.

Example:

```powershell
.\wtg.exe --watch --sink mqtt --mqtt-host 127.0.0.1 --mqtt-port 1884 --mqtt-node-id testnode
```

The deployment boundary is:

```text
NVIDIA NVML
  -> WTG collection and MQTT publication
  -> Home Assistant MQTT discovery and WTG entities
  -> optional downstream templates and dashboards
```

WTG owns provider collection, MQTT state publication, availability, discovery configuration, and deterministic Home Assistant device/entity creation. Home Assistant owns the discovered entity registry and local presentation. WTG does not install dashboard cards, create Home Assistant template packages, or classify display states such as `IDLE`, `LOAD`, `MAX`, `LIMIT`, or `SUS`.

WTG publishes one JSON state payload per NVIDIA GPU to a topic shaped like:

```text
wtg/<node_id>/gpu<index>/state
```

Home Assistant discovery creates WTG device and sensor entities from that state path. Entity IDs are deterministic and include the advertised hostname, GPU index, and metric, for example:

```text
sensor.wtg_<hostname>_gpu_0_gpu_0_power
sensor.wtg_<hostname>_gpu_0_gpu_0_gpu_utilization
sensor.wtg_<hostname>_gpu_0_gpu_0_temperature
```

MQTT is a transport and integration surface, not a replacement telemetry authority. The telemetry values remain NVIDIA/NVML observations collected by WTG. Broker captures validate publication and transport behavior; Home Assistant entities validate discovery and integration behavior.

AMD ADL and Intel Level Zero provider output are not currently published through the MQTT or Home Assistant discovery paths.

Detailed notes: [MQTT and Home Assistant](docs/mqtt-home-assistant.md)

## Redline Visualization Layer

[WTG HA Redline](https://github.com/novovictus/wtg-ha-redline) is a separate optional Home Assistant presentation project. It consumes WTG-discovered Home Assistant entities and can derive dashboard-oriented scores, warning states, gauges, and labels without changing the underlying WTG telemetry.

The responsibility split is intentional:

```text
WTG
  provider telemetry
  MQTT state
  availability
  Home Assistant discovery

Home Assistant
  discovered WTG devices and entities

WTG HA Redline
  template sensors
  derived display states
  gauges and dashboard cards
```

Redline-derived states are presentation interpretations, not WTG provider truth. Redline does not replace WTG MQTT discovery and is not required to use WTG's CLI, sinks, MQTT publisher, or experimental desktop UI.

Redline is maintained and reviewed separately. Its installation instructions, entity templates, thresholds, and dashboard behavior belong in the Redline repository rather than in WTG.

## Configuration

WTG supports an explicit TOML configuration file for its MQTT and Home Assistant discovery behavior.

WTG configuration owns settings such as:

- broker host and port
- optional MQTT authentication
- MQTT node identity and topic naming
- Home Assistant discovery enablement
- retained availability and discovery behavior

Configuration is opt-in:

- WTG does not auto-create a config file
- WTG does not auto-load `wtg.toml`
- `--config <path>` is required to load configuration
- CLI flags override config values
- normal non-MQTT commands remain unaffected

WTG configuration does not install or configure Home Assistant dashboards, packages, template sensors, Redline thresholds, or Redline state logic. Those are downstream Home Assistant concerns.

Detailed notes: [Configuration](docs/configuration.md)

## eGUI

`wtg-ui.exe` is an experimental egui desktop frontend.

It displays provider-backed adapter telemetry from NVIDIA NVML, AMD ADL, and Intel Level Zero where the corresponding provider is available. It also provides a convenience layer over the same explicit WTG TOML configuration model used by the CLI.

The UI may assist with WTG runtime and transport configuration, but it does not install Home Assistant dashboards or configure the Redline presentation layer.

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
  experimental provider-side work, including AMD ADL and Intel Level Zero
```

`wtg-app` does not own NVML provider querying. Expanded NVML provenance collection belongs in `wtg-core`. Experimental non-NVIDIA provider work remains provider-scoped under `wtg-providers` and is routed through `wtg.exe` only when explicitly selected.

The runtime and presentation boundary is:

```text
WTG
  provider collection
  CLI validation and local sinks
  MQTT publication
  Home Assistant discovery

Home Assistant
  discovered WTG devices and metric entities

Optional downstream projects
  templates
  derived states
  dashboards
```

WTG owns the collected provider facts and the transport metadata it emits. Downstream consumers may visualize or classify those values, but those interpretations do not become WTG telemetry.

## Driver Requirements

WTG relies on NVIDIA NVML on Windows for the default provider path.

Empirical testing shows:

- drivers prior to roughly 470 may ship `nvidia-smi` without a usable `nvml.dll`
- modern tested drivers expose NVML across tested SKUs
- individual counters may still show driver/platform-specific behavior
- WTG fails fast when NVML is unavailable and prints an explicit error when possible

AMD ADL relies on `atiadlxx.dll` being available on the test system. Intel provider output relies on a usable Intel Level Zero runtime. Unsupported, unavailable, or failing provider calls are reported as provider-scoped facts and are not treated as NVIDIA/NVML failures.

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
.\target\release\wtg.exe --provider intel --once
.\target\release\wtg.exe --provider intel --watch --interval 1000
.\target\release\wtg.exe --probe
.\target\release\wtg.exe --probe-fields --field-id 74
```

Validation boundaries by surface:

- CLI, probe, provider output, and local sink artifacts validate WTG collection and serialization.
- MQTT broker/subscriber captures validate WTG publication and transport.
- Home Assistant device and entity creation validates WTG discovery behavior.
- Redline templates, states, and dashboards validate downstream presentation only.
- The eGUI provides visual corroboration and configuration assistance but is not the formal validation reference.

Do not treat MQTT, Home Assistant, Redline, or the eGUI as alternate provider authorities. Do not treat AMD ADL or Intel Level Zero output as NVIDIA/NVML-equivalent telemetry.

For automated watch checks, avoid `Select-Object -First ...` as final evidence because it intentionally closes stdout and can produce a broken-pipe panic after the selected lines are captured. Manual `Ctrl+C` watch termination is cleaner for final run notes.

## Validation Strategy

- Compare WTG CLI output against provider-native reference surfaces where useful.
- Compare NVIDIA/NVML telemetry against `nvidia-smi`, WSL NVML metrics, and Windows-reported metrics to characterize abstraction differences.
- Use CLI, probe, field-values, provider output, and sink artifacts for collection evidence.
- Use MQTT broker/subscriber captures for publication and transport evidence.
- Use Home Assistant device and entity creation for discovery evidence.
- Use downstream Redline states and dashboards only to validate presentation behavior.
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
| dev/0.2.9 | Intel Level Zero/Sysman provider-boundary evidence routed through `wtg.exe --provider intel` |
| dev/0.3.0 | eGUI provider-backed adapter/device view |
| v0.3+ | Optional UI and distribution hardening |

## Next Immediate Step

- Keep 0.3.0 scoped to the eGUI provider-backed adapter/device view.
- Preserve the existing Devices pane and right-side detail model.
- Show telemetry-capable devices only: NVIDIA NVML devices, AMD ADL AMD adapters, and Intel Level Zero devices.
- Do not add topology-only duplicate rows, provider dropdowns, provider trees, or fake cross-vendor normalization.

## WTG 0.3.x Provider Discovery

WTG 0.3.x moves AMD ADL and Intel Level Zero/Sysman from discovery-grade provider evidence toward usable provider-native telemetry.

See `docs/provider-discovery-0.3.md`.