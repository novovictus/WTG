WTG is licensed under the GNU General Public License v3.0 (GPLv3). See LICENSE.
Copyright (C) 2026 Adam Hooper

# WTG - WhatTheGPU

**Tagline:** Honest GPU compute stats for Windows

## Current Status (unreleased v0.2.4 - Explicit TOML Configuration)

WTG is currently focused on empirical NVML telemetry validation under Windows WDDM.  
Recent testing has identified a driver-branch regression affecting memory-utilization reporting on specific consumer mobile Ampere GPUs (580.88+), not reproduced on tested desktop or professional SKUs.
Findings reflect publicly accessible NVML telemetry behavior under Windows WDDM.
See `artifacts/test-matrix/matrix.md` for empirical results.

WTG v0.2.0-beta5 adds an experimental dual-surface build: the existing CLI validation surface (`wtg.exe`) plus a separate egui desktop frontend (`wtg-ui.exe`). The CLI remains the reference surface for validation evidence; the UI is a visual inspection and demo surface.

WTG v0.2.1 proved the generic experimental MQTT watch sink and expanded payload parity. WTG v0.2.2 added opt-in Home Assistant MQTT discovery on top of that sink. WTG v0.2.3 added MQTT username/password authentication for brokers such as the Home Assistant Mosquitto add-on, retained Home Assistant availability, and an explicit retained discovery cleanup command. Unreleased WTG v0.2.4 adds explicit TOML configuration support for MQTT and Home Assistant settings.

Configuration remains opt-in. WTG does not auto-create `wtg.toml`, does not auto-load `wtg.toml`, and does not change normal `--once`, `--watch`, `--probe`, or sink behavior unless `--config <path>` or `--mqtt-init-config` is explicitly used. WTG remains an MQTT publisher, not a broker: it does not expose a listening network service, configure the broker, open firewall rules, or manage subscriber access.

---

## Project Purpose

Windows lacks native, real-time, per-process CUDA utilization metrics. Task Manager, PerfMon, and Windows Telemetry report engine scheduling and memory residency but do not expose low-level CUDA execution metrics such as SM activity and per-process compute attribution. Existing tools are either:

* Linux-first (nvtop, nvitop, nviwatch), partially compatible on Windows
* Wrappers around `nvidia-smi` (slow, fragile)
* Windows-native but non-CUDA-specific (GPU-Z, HWiNFO), showing only aggregate GPU load

WTG provides a Windows-native NVML telemetry layer focused on CUDA-relevant metrics, operating independently of Windows telemetry abstractions.

---

### How WTG relates to Task Manager

WTG and Windows Task Manager observe the same GPU through the same kernel driver, but at different abstraction layers.

See: `artifacts/abstraction-model/wtg_vs_task_manager_abstraction_model.md`

---

## Empirical Findings (Driver Behavior)

* 576.88 stable baseline; 580.88 branch inflection observed
* Persistent memory-util anomaly on consumer mobile Ampere
* Not reproduced on desktop or professional SKUs

---

## Project Scope (Core Engine)

* **Target**: NVIDIA GPUs, CUDA metrics only
* **Platform**: Windows-native CLI validation binary (`wtg.exe`) with an experimental separate desktop UI binary (`wtg-ui.exe`)
* **Metrics**:

  * Per-process memory attribution and NVML-reported utilization metrics
  * VRAM used/reserved per PID
  * Power draw, clocks (contextual)
  * Exclude WDDM/Task Manager compute % from "truth" layer
* **Refresh Rate**: CLI watch defaults to 1000 ms; empirical targets remain in the 250-500 ms range where appropriate
* **UI**: Current validation is CLI/probe/sink based. The egui UI is an experimental visual frontend, not the validation reference surface.

---

## Current Usage (CLI Engine)

WTG is currently a command-line proof-of-concept focused on validating NVML-based GPU telemetry on Windows.

### Probe, sink, and field-values behavior

WTG includes additional CLI paths for probe, field-value, and sink validation. These diagnostic paths were introduced during beta 4 probe/probe-fields work and remain available in current development builds; they are not broad release-contract guarantees.

The diagnostic CLI scope is intentionally narrow:

- preserve existing `--once`, `--watch`, and `--stats` behavior
- add probe-oriented diagnostic output
- add structured CSV output for snapshot, stats, probe, and probe-fields paths
- add line-oriented JSONL sink output for snapshot, stats, probe, and probe-fields paths
- add experimental MQTT publishing for live `--watch` snapshots
- add optional Home Assistant MQTT discovery for the experimental MQTT watch sink
- add optional MQTT username/password authentication through a password environment variable
- add retained Home Assistant availability and explicit retained discovery cleanup for MQTT
- add explicit opt-in TOML configuration for MQTT/Home Assistant CLI workflows
- add experimental raw NVML field-value probing through explicit field IDs
- avoid interpreting raw field IDs as proof of driver causality in code or documentation

### Modes

- `--once`  
  Capture a single GPU snapshot and exit.

- `--watch`  
  Continuously poll GPU state at a fixed interval.

- `--config <path>`  
  Load an explicit WTG TOML configuration file. WTG never auto-loads `wtg.toml`; the file is ignored unless passed with `--config`.

- `--interval <ms>`  
  Polling interval in milliseconds.  
  Default: `1000`  
  Applies to CLI `--watch` mode. The experimental UI has its own refresh control in the window.

- `--stats`  
  Print the stable key:value stats format. Requires `--once` or `--watch`.

- `--probe`  
  Capture one snapshot and print a minimal probe block for field validation.

- `--probe-fields`  
  Experimental mode that captures one snapshot, prints the normal utilization path, and then queries selected NVML field-value IDs.

- `--field-id <u32>`  
  Repeatable field ID argument for `--probe-fields`. At least one `--field-id` is required when using `--probe-fields`.

- `--sink jsonl`  
  Create a timestamped `wtg_sink_*.jsonl` file. JSONL sinks write `{"line":"..."}` records for supported output lines.

- `--sink csv`  
  Create a timestamped `wtg_sink_*.csv` file. CSV sinks write structured headers and rows for snapshot, stats, probe, and probe-fields output.

- `--sink mqtt`
  Publish live `--watch` snapshot payloads to a user-specified MQTT broker. This sink is experimental, watch-only, QoS 0, and non-retained. It does not create a sink file.

- `--mqtt-host <host>`
  MQTT broker host. Required with `--sink mqtt`, unless supplied by an explicit config file.

- `--mqtt-port <port>`
  MQTT broker port. Default: `1883`.

- `--mqtt-topic-prefix <prefix>`
  MQTT topic prefix. Default: `wtg`.

- `--mqtt-node-id <id>`
  Stable WTG node identifier used in MQTT topics. Required with `--sink mqtt`, unless supplied by an explicit config file.

- `--mqtt-username <user>`
  MQTT username. Requires `--mqtt-password` or `--mqtt-password-env`.

- `--mqtt-password <password>`
  MQTT password. Requires `--mqtt-username`. Convenient for trusted local or home-lab use; see security notes below.

- `--mqtt-password-env <var>`
  Read the MQTT password from the named environment variable. Requires `--mqtt-username`. Safer alternative that keeps the password out of the WTG command line and saved `wtg.toml`.

- `--mqtt-ha-discovery`
  Publish Home Assistant MQTT discovery configs for the MQTT watch sink. Requires active MQTT.

- `--mqtt-ha-prefix <prefix>`
  Home Assistant MQTT discovery prefix. Default: `homeassistant`.

- `--mqtt-ha-remove-discovery`
  Remove retained WTG Home Assistant discovery configs and retained availability from the broker. Requires `--sink mqtt`, `--mqtt-host`, and `--mqtt-node-id`, unless MQTT settings are supplied by an explicit config file.

- `--mqtt-init-config`
  Create a template `wtg.toml` in the current working directory and exit. If `wtg.toml` already exists, WTG refuses to overwrite it.

- `--mqtt-save-config`
  Write `wtg.toml` from explicit MQTT CLI flags and exit. Requires `--mqtt-host` and `--mqtt-node-id`. Does not connect to MQTT or initialize NVML. Does not load an existing config file.

- `--force-config`
  Overwrite an existing `wtg.toml` when used with `--mqtt-save-config`.

- `--mqtt-retain-discovery`
  Retain Home Assistant MQTT discovery configs. State messages remain non-retained. Requires `--mqtt-ha-discovery`, or may be present with `--mqtt-ha-remove-discovery` where it is accepted and ignored.

- `--help`, `-h`
  Print CLI usage information and exit.

- `--version`, `-V`
  Print the WTG / WhatTheGPU version and exit.

### Sink support matrix

| Mode | `--sink jsonl` | `--sink csv` | `--sink mqtt` | Notes |
| --- | --- | --- | --- | --- |
| `--probe` | Supported | Supported | Not supported | CSV emits structured probe records. MQTT does not publish probe output. |
| `--probe-fields` | Supported | Supported | Not supported | CSV emits one self-contained row per GPU field result. MQTT does not publish field-value probe output. |
| `--once` | Supported | Supported | Not supported | CSV emits snapshot rows. MQTT is watch-only in this spike. |
| `--watch` | Supported | Supported | Supported | CSV emits snapshot rows per tick. MQTT publishes live state payloads per tick. `--interval` applies here. |
| `--once --stats` | Supported | Supported | Not supported | CSV emits stats rows. MQTT is watch-only in this spike. |
| `--watch --stats` | Supported | Supported | Supported | CSV emits stats rows per tick. MQTT publishes snapshot payloads, not stats-formatted text. |

Home Assistant discovery is an option on top of active MQTT, not a separate sink. It is available only for the same MQTT-supported watch modes: `--watch` and `--watch --stats`. `--mqtt-ha-remove-discovery` is a one-shot MQTT broker cleanup command for retained WTG discovery config topics and retained availability; it does not publish state.

Note: plain `--watch` and `--watch --stats` currently have different recovery behavior. `--watch --stats` uses a persistent NVML context and attempts to reinitialize after snapshot failures. Plain `--watch` is stricter and exits on snapshot failure. This behavior may be unified later.

CSV sinks emit exactly one header row per sink file. Unsupported optional values are written as `N/A`.

MQTT publishes live snapshot payloads only while WTG is running and connected to the broker. State messages are QoS 0 and not retained, so subscribers receive samples only while connected. Optional username/password authentication is configured with `--mqtt-username` plus either `--mqtt-password` or `--mqtt-password-env`. WTG accepts a direct password for convenience or reads the password from a named environment variable. Optional Home Assistant discovery configs are published once when `--mqtt-ha-discovery` is enabled and may be retained only with `--mqtt-retain-discovery`. When Home Assistant discovery is enabled, WTG uses retained availability on `wtg/<node_id>/status` with an MQTT Last Will and Testament for unexpected disconnects. WTG is an MQTT publisher, not a broker. Broker setup, firewall policy, retention, and subscriber access are outside WTG's scope.

### Experimental MQTT watch sink

The MQTT sink publishes live telemetry from `--watch` to a user-specified broker.

Anonymous local broker example:

```powershell
.\wtg.exe --watch --sink mqtt --mqtt-host 127.0.0.1 --mqtt-port 1884 --mqtt-node-id testnode
```

Authenticated Home Assistant Mosquitto example:

```powershell
$env:WTG_MQTT_PASSWORD = "your-password"

.\target\release\wtg.exe --watch `
  --sink mqtt `
  --mqtt-host homeassistant.local `
  --mqtt-node-id bench1 `
  --mqtt-username wtg `
  --mqtt-password-env WTG_MQTT_PASSWORD `
  --mqtt-ha-discovery `
  --mqtt-retain-discovery
```

Direct-password runtime example:

```powershell
.\target\release\wtg.exe --watch `
  --sink mqtt `
  --mqtt-host homeassistant.local `
  --mqtt-node-id bench1 `
  --mqtt-username wtg `
  --mqtt-password "your-password" `
  --mqtt-ha-discovery `
  --mqtt-retain-discovery
```

Home Assistant discovery cleanup example:

```powershell
.\target\release\wtg.exe `
  --sink mqtt `
  --mqtt-host homeassistant.local `
  --mqtt-node-id bench1 `
  --mqtt-username wtg `
  --mqtt-password-env WTG_MQTT_PASSWORD `
  --mqtt-ha-remove-discovery
```

If discovery was published from a saved config, cleanup can use the same config file:

```powershell
.\wtg.exe --sink mqtt --mqtt-ha-remove-discovery --config .\wtg.toml
```

Topic shape:

```text
wtg/<node_id>/gpu<index>/state
```

Home Assistant discovery topic shape:

```text
homeassistant/sensor/wtg_<node_id>_gpu<index>_<metric>/config
```

Availability topic:

```text
wtg/<node_id>/status
```

Home Assistant discovery notes:

* `--mqtt-ha-discovery` requires active MQTT
* `--mqtt-ha-discovery` still requires an MQTT broker
* Home Assistant Core is not the broker
* a typical Home Assistant setup uses the Mosquitto broker add-on plus the Home Assistant MQTT integration
* discovery configs are published to the broker under the discovery prefix
* Home Assistant entities are backed by retained MQTT discovery config topics and Home Assistant's device/entity registry
* retained discovery is controlled by `--mqtt-retain-discovery`
* `--mqtt-retain-discovery` persists entity definitions across broker and Home Assistant restarts
* `--mqtt-ha-remove-discovery` clears WTG retained discovery configs from the broker by publishing retained empty payloads to WTG-created config topics
* Home Assistant may still require stale device/entity registry entries to be deleted manually after retained discovery cleanup
* Overview dashboard auto-population is Home Assistant dashboard behavior, not WTG behavior
* state messages remain non-retained
* availability topic is `wtg/<node_id>/status`
* WTG publishes discovery configs first, then publishes retained `online` availability, then publishes state
* WTG CONNECT includes an MQTT Last Will and Testament that publishes retained `offline` availability on unexpected disconnect
* graceful shutdown offline publishing is deferred

Example topic:

```text
wtg/testnode/gpu0/state
```

Example payload:

```json
{
  "wtg_version": "0.2.4",
  "payload_schema": 1,
  "tick_seq": 123,
  "tick_ts": "1780420000.123",
  "host": "LAPTOP-8CC8RC3A",
  "node_id": "testnode",
  "gpu_index": 0,
  "gpu_name": "NVIDIA GeForce RTX 3080 Laptop GPU",
  "gpu_uuid": "GPU-...",
  "driver_version": "580.88",
  "cuda_driver_version": "13000",
  "compute_mode": "Default",
  "perf_state": "P8",
  "pci_bus_id": "00000000:01:00.0",
  "temp_c": 50,
  "util_gpu_pct": 0,
  "util_mem_controller_pct": 100,
  "vram_used_mib": 844,
  "vram_total_mib": 16384,
  "power_w": 13.2,
  "power_limit_w": 130.0
}
```

Current unreleased behavior:

* `--sink mqtt` is supported only with `--watch`, unless using the explicit `--mqtt-ha-remove-discovery` cleanup command
* WTG opens an outbound connection to the configured broker
* WTG does not expose a listening network service
* one JSON state payload is published per GPU per watch tick
* payloads include watch tick metadata, `GpuSnapshot` values, and the same probe context fields exposed by probe surfaces
* topic prefix defaults to `wtg`
* payloads are live QoS 0 messages
* payloads are not retained
* anonymous MQTT remains supported when no auth flags are provided
* MQTT username/password auth requires `--mqtt-username` plus either `--mqtt-password` or `--mqtt-password-env`, but not both password sources together
* WTG accepts a direct password from CLI flags or config, or reads the password from the named environment variable before connecting
* Home Assistant discovery is emitted only when `--mqtt-ha-discovery` is set
* discovery configs publish once after MQTT connect and after the first successful snapshot set, before online availability and state publishing
* discovery configs are retained only when `--mqtt-retain-discovery` is set
* when Home Assistant discovery is enabled, WTG publishes discovery configs first, then publishes retained `online` to `wtg/<node_id>/status`, then publishes state
* when Home Assistant discovery is enabled, MQTT CONNECT includes a retained `offline` Last Will and Testament for `wtg/<node_id>/status`
* `--mqtt-ha-remove-discovery` clears WTG retained discovery config topics and retained availability, but does not delete normal state topics
* graceful shutdown offline publishing is deferred
* explicit config file support is included, but WTG does not auto-create or auto-load config files
* WTG does not install, run, or configure the broker

Local broker test shape:

```text
WTG -> local MQTT broker -> local subscriber
```

Observability-stack shape:

```text
WTG host -> existing MQTT broker -> subscribers such as Home Assistant, MQTT Explorer, dashboards, or scripts
```

### Optional TOML configuration

WTG supports an explicit TOML configuration file for MQTT and Home Assistant settings.

This is intentionally conservative:

- WTG does not auto-create a config file.
- WTG does not auto-load `wtg.toml`.
- `--config <path>` is required to load configuration.
- CLI flags override config values.
- Config values override built-in defaults.
- Empty strings in the config template are treated as absent values.
- Normal non-MQTT commands remain unaffected.

Create a template:

```powershell
.\wtg.exe --mqtt-init-config
```

This creates:

```text
.\wtg.toml
```

WTG refuses to overwrite an existing `wtg.toml`.

Save a ready-to-run config from explicit CLI flags:

```powershell
.\wtg.exe --mqtt-save-config `
  --mqtt-host "homeassistant-shop" `
  --mqtt-node-id "bench" `
  --mqtt-username "wtg" `
  --mqtt-password "test" `
  --mqtt-ha-discovery `
  --mqtt-retain-discovery `
  --force-config
```

Environment-variable auth variant:

```powershell
$env:WTG_MQTT_PASSWORD = "test"

.\wtg.exe --mqtt-save-config `
  --mqtt-host "homeassistant-shop" `
  --mqtt-node-id "bench" `
  --mqtt-username "wtg" `
  --mqtt-password-env "WTG_MQTT_PASSWORD" `
  --mqtt-ha-discovery `
  --mqtt-retain-discovery `
  --force-config
```

No-auth broker variant:

```powershell
.\wtg.exe --mqtt-save-config `
  --mqtt-host "broker.local" `
  --mqtt-node-id "bench"
```

`--mqtt-save-config` writes from explicit CLI flags only, validates auth combinations, sets `[mqtt].enabled = true`, and exits before MQTT or NVML initialization. Use `--force-config` to overwrite an existing `wtg.toml`.

Template:

```toml
# WTG CLI configuration.
# WTG never auto-loads this file. Use --config <path> explicitly.
# Leave environment-specific values blank until you are ready to use them.

[mqtt]
enabled = false
host = ""
port = 1883
username = ""
password = ""
password_env = ""
topic_prefix = "wtg"
node_id = ""

[mqtt.home_assistant]
discovery = false
discovery_prefix = "homeassistant"
retain_discovery = true
```

Load config explicitly:

```powershell
.\wtg.exe --watch --config .\wtg.toml
```

Override a config value from the CLI:

```powershell
.\wtg.exe --watch --config .\wtg.toml --mqtt-host "homeassistant-shop"
```

Use config for MQTT cleanup:

```powershell
.\wtg.exe --sink mqtt --mqtt-ha-remove-discovery --config .\wtg.toml
```

Cleanup can use the same config file that published retained Home Assistant discovery, including a config where `[mqtt.home_assistant]` has `discovery = true` and `retain_discovery = true`. Cleanup still requires `--sink mqtt`; config can supply host, node ID, authentication, topic prefix, and Home Assistant discovery prefix.

#### MQTT activation from config

`[mqtt].enabled = true` allows MQTT to activate from config without `--sink mqtt`, but only for `--watch`.

```toml
[mqtt]
enabled = true
```

Then:

```powershell
.\wtg.exe --watch --config .\wtg.toml
```

If `[mqtt].enabled = true` is used without `--watch`, WTG returns a usage error.

If `[mqtt].enabled = false` or absent, loading a config file does not activate MQTT by itself. In that case, MQTT still requires explicit `--sink mqtt`.

Configuration precedence:

```text
CLI flags
  override explicit config values
    override built-in defaults
```

Built-in defaults still include:

```text
mqtt.port = 1883
mqtt.topic_prefix = "wtg"
mqtt.home_assistant.discovery_prefix = "homeassistant"
```

#### Password security notes

- `--mqtt-password` is convenient for trusted local or home-lab use.
- `--mqtt-password` can be visible in the command line, shell history, process listings, logs, and terminal scrollback.
- Saved `wtg.toml` files written with `--mqtt-save-config` and a direct password store the password in plaintext.
- `--mqtt-password-env` keeps the password out of the WTG command line and `wtg.toml`, but setting the environment variable may still expose it depending on the environment.
- TLS and client certificates remain deferred.

The eGUI configurator is not part of v0.2.4. The intended eGUI work should edit, validate, and save the same explicit TOML configuration model used by the CLI.

### Probe context fields

`--probe` and `--probe-fields` include runtime context fields intended to support same-GPU, cross-driver comparisons:

- `wtg.version`
- `driver.version`
- `cuda.driver_version`
- `gpu.compute_mode`
- `gpu.perf_state`
- `gpu.pci.bus_id`

`gpu.perf_state` reports the NVML performance state, such as `P0` through `P15` or `Unknown`. `P0` is the highest-performance state. Higher-numbered states are lower-power states. `N/A` means the query was unsupported or failed.

The structured `--probe --sink csv` and `--probe-fields --sink csv` outputs include the same context as CSV columns, including `gpu_perf_state`.

### Probe field notes

- `util.mem_controller_pct` is NVML memory-controller utilization, not VRAM occupancy.
- VRAM occupancy is reported separately as `vram.used_mib` / `vram.total_mib`.
- On some Windows WDDM / NVIDIA driver combinations, NVML memory utilization may report `100%` at idle or low VRAM occupancy.
- This condition does not mean VRAM is full.

Example interpretation:

```text
util.mem_controller_pct: 100
vram.used_mib: 759
vram.total_mib: 16384
```

This means the NVML memory-utilization counter is pegged while allocated VRAM remains low.

### Probe-fields mode

`--probe-fields` compares WTG's normal NVML utilization path against selected field-values queries through the safe `nvml-wrapper` API for `nvmlDeviceGetFieldValues`.

Example:

```powershell
cargo run -- --probe-fields --field-id 74 --field-id 78 --field-id 83 --field-id 94 --field-id 95
```

The utilization path is printed first:

```text
[probe-fields] gpu=0
gpu.index: 0
gpu.name: NVIDIA GeForce RTX 3080 Laptop GPU
gpu.uuid: GPU-...
driver.version: 580.88
cuda.driver_version: 13000
gpu.compute_mode: Default
gpu.perf_state: P8
gpu.pci.bus_id: 00000000:01:00.0
util.gpu_pct: 0
util.mem_controller_pct: 100
vram.used_mib: 759
vram.total_mib: 16384
```

Each requested field ID is then printed as a separate block:

```text
[field] gpu=0 field.id=83
field.query: ok
field.status: Ok
field.type: u64
field.value: 1280086850
```

`field.query` distinguishes whole-call status from per-field status:

- `ok`: the field-values call returned a per-field result.
- `call_error`: the whole field-values query failed before returning per-field results.
- `field_error`: the field-values call succeeded, but this individual field returned an error.

Field-values queries working for supported field IDs show that the field-values API is callable on the same device/session. This does not by itself prove driver causality. Cross-driver comparison still requires capturing the same `--probe` and `--probe-fields` outputs on different NVIDIA driver versions.

### Diagnostic validation target

Before tagging or packaging a beta build, validate on Windows NVML hardware:

```powershell
cargo fmt --check
cargo test
cargo build -p wtg-app --release
.\target\release\wtg.exe --once
.\target\release\wtg.exe --probe
.\target\release\wtg.exe --probe --sink jsonl
.\target\release\wtg.exe --probe --sink csv
.\target\release\wtg.exe --probe-fields --field-id 74
.\target\release\wtg.exe --probe-fields --field-id 74 --field-id 78 --field-id 83
.\target\release\wtg.exe --watch --sink mqtt --mqtt-host 127.0.0.1 --mqtt-port 1884 --mqtt-node-id testnode
```

For MQTT validation, run a broker and subscribe to:

```text
wtg/testnode/#
```

### Packaging checkpoint helper

The development packaging helper is:

```powershell
.\artifacts\dev\wtg_package_checkpoint.ps1
```

Default behavior:

- derives the package label from the current branch when `-Label` is omitted
- debug build unless `-Release` is supplied
- output under `artifacts\packages`

Useful options:

```powershell
.\artifacts\dev\wtg_package_checkpoint.ps1 -Label probe-fields
.\artifacts\dev\wtg_package_checkpoint.ps1 -Release
.\artifacts\dev\wtg_package_checkpoint.ps1 -Release -CleanPackages
```

`-CleanPackages` refreshes `artifacts\packages` while preserving `.gitkeep`. The checkpoint package captures git/build metadata and CLI validation outputs from `wtg.exe`. On branches that produce `wtg-ui.exe`, the helper passively includes and hashes the UI binary, but it does not launch the UI.

### NVIDIA bug-report collector

`artifacts/dev/nvidia-bug-report.ps1` is a best-effort Windows-native helper modeled after NVIDIA's Linux `nvidia-bug-report.sh` flow and used for NVIDIA Developer bug #6162407.

It is not part of the WTG runtime and is not required for normal CLI or GUI use. It is retained as a development/research artifact for packaging WTG/NVML evidence with Windows and NVIDIA diagnostic context.

Expected run directory:

- `wtg.exe`
- `wtg_test.ps1`
- `nvidia-bug-report.ps1`

Example:

```powershell
powershell -NoProfile -File .\nvidia-bug-report.ps1
```

The script runs `wtg_test.ps1`, detects the generated WTG result file, collects Windows and NVIDIA diagnostic context, enables NVML debug logging for later diagnostic calls, runs an additional `wtg.exe --probe --sink jsonl` capture, writes a collection manifest, and produces a driver-versioned ZIP bundle named like:

```text
nvidia_bug_6162407_driver_<driver>.zip
```

### Examples

One-shot snapshot:

```powershell
.\wtg.exe --once
```

Probe snapshot:

```powershell
.\wtg.exe --probe
```

Probe CSV sink:

```powershell
.\wtg.exe --probe --sink csv
```

Probe JSONL sink:

```powershell
.\wtg.exe --probe --sink jsonl
```

Experimental MQTT watch sink:

```powershell
.\wtg.exe --watch --sink mqtt --mqtt-host 127.0.0.1 --mqtt-port 1884 --mqtt-node-id testnode
```

Explicit TOML config:

```powershell
.\wtg.exe --mqtt-init-config
.\wtg.exe --watch --config .\wtg.toml
```

Probe field-values comparison:

```powershell
cargo run -- --probe-fields --field-id 74 --field-id 78 --field-id 83 --field-id 94 --field-id 95
```

Experimental egui UI:

```powershell
cargo run -p wtg-app --bin wtg-ui
```

The egui UI is a separate experimental binary target. It does not modify the existing `wtg` CLI parser, does not create sink files, and uses the same `wtg-core` NVML snapshot and probe-context paths as the CLI. Unsupported optional values are displayed as `N/A`, and refresh failures are reported in the window without intentionally closing it.

## Experimental UI build notes and blocking behavior

WTG v0.2.0-beta5 includes two executable surfaces:

- `wtg.exe`: the primary CLI validation, capture, probe, and sink interface.
- `wtg-ui.exe`: an experimental egui desktop frontend that displays live telemetry from the same `wtg-core` NVML path.

The UI is experimental. It is intended for live visual inspection, demos, and operator-facing telemetry. It is not the reference surface for regression testing or metric capture. Use the CLI, CSV/JSONL sinks, and probe outputs for validation evidence.

### Building

From the repository root:

```powershell
cargo build -p wtg-app --release
```

This produces:

```text
target\release\wtg.exe
target\release\wtg-ui.exe
```

Run the UI directly:

```powershell
.\target\release\wtg-ui.exe
```

Run the CLI validation surface directly:

```powershell
.\target\release\wtg.exe --once
.\target\release\wtg.exe --probe
.\target\release\wtg.exe --probe-fields --field-id 74 --field-id 78
```

### Windows blocking behavior

`wtg-ui.exe` is currently an unsigned experimental binary. On some Windows systems, especially systems with Smart App Control, Windows Defender Application Control (WDAC), App Control for Business, or other enterprise application-control policies enabled, Windows may block the UI binary from launching.

During bench testing, `wtg-ui.exe` was blocked on a Windows 11 validation system with Code Integrity / Application Control enforcement enabled. Windows reported:

```text
Program 'wtg-ui.exe' failed to run: An Application Control policy has blocked this file.
```

The Code Integrity event log showed Event IDs `3033` and `3077`, including:

```text
wtg-ui.exe did not meet the Enterprise signing level requirements or violated code integrity policy.
```

This is expected behavior for unsigned experimental binaries on policy-enforced systems. It does not mean the UI failed, that NVML failed, or that the executable is malicious.

If `wtg-ui.exe` is blocked, options include:

- build and run on a development machine without restrictive application-control policy
- sign the binary with a trusted certificate
- allowlist the binary or hash according to local policy
- use `wtg.exe` for CLI validation workflows

Do not disable organization-managed application-control policy unless you own and control the system and understand the security impact.

### Validation boundary

`wtg-ui.exe` visualizes telemetry, but formal validation should use:

- `wtg.exe --once`
- `wtg.exe --watch`
- `wtg.exe --probe`
- `wtg.exe --probe-fields`
- `--sink csv`
- `--sink jsonl`

The UI may visually corroborate CLI output, but screenshots of the UI should not replace captured CLI or sink artifacts in regression testing.

---

### Driver Requirements

WTG relies on NVIDIA NVML on Windows. Empirical testing shows:

- Drivers prior to ~470 may ship `nvidia-smi` without a usable `nvml.dll`
- Modern drivers (>=580) consistently expose NVML across tested SKUs
- WTG fails fast when NVML is unavailable and prints an explicit error when possible

---

## Key Decisions

1. **Language**: Rust (memory safe, low overhead, native FFI for NVML)
2. **Backend = Source of Truth**:

   * NVML bindings, polling, aggregation, process attribution, snapshot emission
   * Immutable snapshots, append-only, versioned
   * No UI, MQTT, config, or output-sink logic in backend
3. **Surfaces = Consumers**:

   * `wtg.exe` is the CLI validation, capture, probe, and sink surface.
   * `wtg-ui.exe` is an experimental egui visual surface.
   * MQTT is an experimental watch-mode delivery surface for publishing live telemetry to an existing broker.
   * Explicit TOML config is an app-layer CLI convenience for MQTT/Home Assistant settings.
   * App-layer surfaces consume the same `wtg-core` telemetry path; no backend duplication.
4. **Extensible Metric Model**: trait-based, allows future integration of other vendors (ROCm, DX12)
5. **Phased Approach**:

   * Current: CLI validation and empirical NVML characterization.
   * Spike: experimental egui desktop frontend as a second surface.
   * Later: optional native window, tray integration, and UI polish.
6. **Repository / Licensing**: GitHub-hosted / GPLv3.

---

## Technical Architecture

Current source layout:

```text
wtg-core/                         # Backend / truth layer
  src/                            # NVML context, snapshots, probe context, field queries

wtg-app/
  src/config.rs                   # Explicit TOML config loading and template generation
  src/main.rs                     # Builds wtg.exe, the CLI validation/capture surface
  src/bin/wtg-ui.rs               # Builds wtg-ui.exe, the experimental UI entrypoint
  src/mqtt.rs                     # Experimental MQTT watch sink implementation
  src/ui.rs                       # Current egui UI implementation

wtg-view/                         # Shared view helpers where applicable
```

Future UI organization may split the current `src/ui.rs` into dedicated modules or crates if the UI grows. Earlier roadmap language referred to TUI, `ui-egui/`, and `ui-native/` layouts; those should be treated as planned or historical organization notes, not the current source tree.

**Snapshot Example (Rust)**:

```rust
struct Snapshot {
    timestamp: Instant,
    gpus: Vec<GpuStats>,
    processes: Vec<ProcessStats>,
}
```

* Snapshots are append-only, immutable
* UI layers render snapshots without modifying them
* Sorting, filtering, and column helpers can be shared in `wtg-view/`
* OS-specific differences should stay out of `wtg-core`

---

## NVML Integration

* Direct NVML FFI (`nvml-sys` or custom bindings)
* Dynamically load NVML DLL at runtime
* Graceful failure handling if driver absent
* Per-process aggregation including short-lived kernels

---

## Refresh Loop

* CLI `--watch` uses a fixed interval, defaulting to 1000 ms unless `--interval <ms>` is supplied
* Probe and once modes capture point-in-time snapshots
* The experimental egui UI has its own refresh control in the window
* The experimental MQTT sink publishes one payload per GPU per watch tick
* No smoothing; snapshots reflect raw, instantaneous utilization

---

## Validation Strategy

* Compare WTG CLI output vs `nvidia-smi` and WSL NVML metrics
* Compare NVML telemetry against Windows-reported metrics to characterize abstraction differences
* Use CLI, probe, field-values, and sink artifacts for validation evidence
* Use MQTT broker/subscriber captures to validate transport only; do not treat MQTT as the telemetry source of truth
* Use the experimental UI for visual corroboration and demos only

---

## Early Validation Artifacts

Empirical GPU / driver test results are summarized in `artifacts/test-matrix/matrix.md`.

---

## Architectural Roadmap (Post-Validation)

1. **CLI validation surface**: truth validation, probe output, field-values comparison, and structured sinks
2. **Experimental egui surface**: live desktop view over the same `wtg-core` telemetry path
3. **Experimental MQTT delivery**: app-layer watch sink for publishing live telemetry to an existing broker
4. **Home Assistant discovery**: optional, explicit discovery config generation, retained availability, and retained discovery cleanup after MQTT transport remains stable
5. **Explicit TOML config**: CLI-owned config loading and template generation for MQTT/Home Assistant settings
6. **eGUI configuration**: future panel to edit, validate, and save the same explicit config model
7. **UI refinement**: module split, display polish, and optional charting only after validation workflows remain stable
8. **Native window control**: chrome, always-on-top, keyboard shortcuts
9. **Tray integration**: background polling, popup on demand
10. **Plugin expansion**: ROCm, DX12, vendor-specific metrics
11. **Distribution hardening**: signed binary, single-file EXE, crash-resilient NVML paths
12. **Reference surfaces retained**: CLI remains the validation surface; egui remains a visual/debug/operator surface unless explicitly promoted later

**Key Principle:** Surfaces are interchangeable lenses; backend is the immutable source of truth. This prevents logic drift and double-work while allowing phased expansion.

---

## Milestones

| Version / Branch | Goal |
| ------- | ---- |
| v0.1.x | Truth-layer validation: one GPU, CLI output, correct NVML metrics vs Windows telemetry |
| v0.2.0-beta4 | Probe, sink, field-values, and driver-behavior validation |
| v0.2.0-beta5 | Experimental dual-surface build: CLI validation surface plus `wtg-ui.exe` visual frontend |
| dev/0.2.1 | Experimental MQTT watch sink for publishing live telemetry to a user-specified broker |
| dev/0.2.2 | Optional Home Assistant MQTT discovery for the experimental MQTT watch sink |
| dev/0.2.3 | MQTT username/password authentication, retained Home Assistant availability, and retained discovery cleanup |
| unreleased v0.2.4 (`dev/0.2.4`) | Explicit TOML config support for MQTT/Home Assistant CLI workflows |
| planned v0.2.5 | Planned eGUI MQTT/Home Assistant configurator over the same config model |
| v0.3+ | Optional native UI, tray integration, cross-vendor extensibility |

---

## Next Immediate Step

* Preserve CLI/probe/sink outputs as the formal validation path.
* Validate explicit TOML configuration against the same authenticated MQTT/Home Assistant flows already proven by CLI flags.
* Keep config opt-in: no auto-create, no auto-load, and no config effects on normal non-MQTT commands.
* Use `wtg-ui.exe` for visual corroboration, demos, and operator-facing inspection.
* Plan the future eGUI MQTT/Home Assistant configurator as an editor/tester for the same explicit TOML config model.
* Validate packaging helper behavior on release builds that include both executable surfaces.
* Continue empirical driver-behavior documentation using captured CLI and structured artifacts.
