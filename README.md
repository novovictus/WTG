WTG is licensed under the GNU General Public License v3.0 (GPLv3). See LICENSE.
Copyright (C) 2026 Adam Hooper

# WTG - WhatTheGPU

**Tagline:** Provider-native GPU telemetry and validation for Windows

WTG is a Windows-native GPU telemetry and validation suite that preserves what each hardware provider actually reports. NVIDIA/NVML remains the primary reference and validation path. WTG 0.3.0 also includes experimental AMD ADL and Intel Level Zero provider surfaces without translating unlike provider facts into false cross-vendor equivalents.

WTG includes:

- `wtg.exe`, the CLI validation, capture, probe, sink, MQTT, and explicit provider surface
- `wtg-ui.exe`, an experimental provider-backed desktop viewer and configurator
- CSV and JSONL output for supported NVIDIA/NVML modes
- MQTT watch publishing and Home Assistant discovery for NVIDIA/NVML
- provider-scoped AMD ADL and Intel Level Zero output

Current release: `v0.3.0`

## Provider Model

- **Primary provider:** NVIDIA/NVML
- **Experimental providers:** AMD ADL and Intel Level Zero
- **Reference surface:** `wtg.exe`
- **Experimental visual surface:** `wtg-ui.exe`
- **Transport surface:** optional MQTT publishing for NVIDIA/NVML watch snapshots

WTG preserves provider identity, source API, units, unavailable states, and errors. It does not synthesize missing values or claim that unlike vendor fields are equivalent.

Detailed provider behavior: [Providers](artifacts/docs/providers.md)

## Build

```powershell
cargo build -p wtg-app --release
```

This produces:

```text
target\release\wtg.exe
target\release\wtg-ui.exe
```

## Quick Start

NVIDIA/NVML one-shot snapshot:

```powershell
.\target\release\wtg.exe --once
```

NVIDIA/NVML watch:

```powershell
.\target\release\wtg.exe --watch --interval 1000
```

Expanded NVIDIA/NVML provenance stats:

```powershell
.\target\release\wtg.exe --once --stats
```

AMD ADL provider output:

```powershell
.\target\release\wtg.exe --provider amd --once
.\target\release\wtg.exe --provider amd --watch --interval 1000
```

Intel Level Zero provider output:

```powershell
.\target\release\wtg.exe --provider intel --once
.\target\release\wtg.exe --provider intel --watch --interval 1000
```

Probe and field diagnostics:

```powershell
.\target\release\wtg.exe --probe
.\target\release\wtg.exe --probe-fields --field-id 74
```

Experimental UI:

```powershell
.\target\release\wtg-ui.exe
```

CLI, sink, schema, probe, and stats details: [CLI and Output](artifacts/docs/cli-and-output.md)

## Sink Support

| Mode | JSONL | CSV | MQTT | Notes |
| --- | --- | --- | --- | --- |
| `--once` | yes | yes | no | concise NVIDIA/NVML snapshot |
| `--once --stats` | yes | yes | no | canonical provenance JSON in JSONL; legacy flat CSV |
| `--watch` | yes | yes | yes | MQTT publishes live NVIDIA/NVML snapshots |
| `--watch --stats` | yes | yes | yes | legacy stats/watch behavior for this release |
| `--provider amd ...` | no | no | no | provider-scoped AMD ADL output |
| `--provider intel ...` | no | no | no | provider-scoped Intel Level Zero output |
| `--probe` | yes | yes | no | NVIDIA/NVML validation output |
| `--probe-fields` | yes | yes | no | NVIDIA/NVML field diagnostics |

## MQTT, Home Assistant, and Redline

WTG can publish live NVIDIA/NVML watch snapshots to an existing MQTT broker and can publish retained Home Assistant MQTT discovery configuration.

```powershell
.\wtg.exe --watch --sink mqtt --mqtt-host 127.0.0.1 --mqtt-port 1884 --mqtt-node-id testnode
```

The integration boundary is:

```text
NVIDIA NVML
  -> WTG collection and MQTT publication
  -> Home Assistant discovery and WTG entities
  -> optional downstream templates and dashboards
```

WTG owns provider collection, MQTT state publication, availability, and discovery configuration. Home Assistant owns the discovered entity registry and local presentation.

[WTG HA Redline](https://github.com/novovictus/wtg-ha-redline) is a separate optional Home Assistant presentation project. It consumes WTG-discovered entities and derives dashboard states, scores, gauges, and warnings. Redline interpretations are not WTG provider truth.

AMD ADL and Intel Level Zero output are not currently published through MQTT or Home Assistant discovery.

Detailed integration behavior: [MQTT and Home Assistant](artifacts/docs/mqtt-home-assistant.md)

## Configuration

WTG configuration is explicit and opt-in:

- WTG does not auto-create or auto-load `wtg.toml`
- `--config <path>` is required to load a file
- CLI flags override config values
- configuration can define broker, authentication, node identity, topic naming, discovery, and retained behavior
- WTG configuration does not install Home Assistant dashboards or Redline packages

Detailed configuration behavior: [Configuration](artifacts/docs/configuration.md)

## Desktop UI

`wtg-ui.exe` is an experimental egui frontend. It displays provider-backed adapter telemetry from NVIDIA NVML, AMD ADL, and Intel Level Zero where available. It also wraps the explicit WTG TOML configuration and CLI MQTT launch behavior.

The UI is not the formal regression-testing or metric-capture reference surface.

Detailed UI behavior: [UI](artifacts/docs/ui.md)

Windows execution and application-control notes: [Windows Deployment](artifacts/docs/windows-deployment.md)

## Validation Boundary

Formal validation uses CLI, provider, probe, and sink artifacts.

```powershell
cargo fmt --check
cargo test
cargo build -p wtg-app --release
.\target\release\wtg.exe --once
.\target\release\wtg.exe --watch --interval 1000
.\target\release\wtg.exe --once --stats
.\target\release\wtg.exe --provider amd --once
.\target\release\wtg.exe --provider intel --once
.\target\release\wtg.exe --probe
.\target\release\wtg.exe --probe-fields --field-id 74
```

Validation responsibilities:

- CLI, provider output, probes, and local sinks validate WTG collection and serialization
- broker/subscriber captures validate MQTT publication and transport
- Home Assistant entity creation validates discovery behavior
- Redline validates downstream presentation only
- the UI provides visual corroboration, not formal evidence

Detailed methodology and current known behavior: [Validation](artifacts/docs/validation.md)

## Architecture

```text
wtg-core/
  NVIDIA/NVML collection, snapshots, probes, fields, and provenance

wtg-app/
  CLI, sinks, MQTT, config, formatting, and provider routing

wtg-app/src/bin/wtg-ui.rs
  experimental desktop UI entrypoint

wtg-providers/
  provider-scoped AMD ADL and Intel Level Zero implementations
```

WTG owns collected provider facts and emitted transport metadata. Downstream consumers may visualize or classify those values, but those interpretations do not become WTG telemetry.

## Research and Test Artifacts

Research, matrices, bench records, validation evidence, and maintained product documentation live under `artifacts/`.

Key starting points:

- `artifacts/docs/`
- `artifacts/docs/matrix.md`
- `artifacts/docs/wtg_regression_research.md`
- `artifacts/docs/wtg_vs_task_manager_abstraction_model.md`

## Maintained Documentation

- [Providers](artifacts/docs/providers.md)
- [CLI and Output](artifacts/docs/cli-and-output.md)
- [Configuration](artifacts/docs/configuration.md)
- [MQTT and Home Assistant](artifacts/docs/mqtt-home-assistant.md)
- [UI](artifacts/docs/ui.md)
- [Validation](artifacts/docs/validation.md)
- [Windows Deployment](artifacts/docs/windows-deployment.md)
