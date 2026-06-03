WTG is licensed under the GNU General Public License v3.0 (GPLv3). See LICENSE.
Copyright (C) 2026 Adam Hooper

# WTG - WhatTheGPU

**Tagline:** Honest GPU compute stats for Windows

## Current Status (v0.2.0-beta6 - Experimental MQTT Watch Sink)

WTG is currently focused on empirical NVML telemetry validation under Windows WDDM.  
Recent testing has identified a driver-branch regression affecting memory-utilization reporting on specific consumer mobile Ampere GPUs (580.88+), not reproduced on tested desktop or professional SKUs.
Findings reflect publicly accessible NVML telemetry behavior under Windows WDDM.
See `artifacts/test-matrix/matrix.md` for empirical results.

WTG v0.2.0-beta5 adds an experimental dual-surface build: the existing CLI validation surface (`wtg.exe`) plus a separate egui desktop frontend (`wtg-ui.exe`). The CLI remains the reference surface for validation evidence; the UI is a visual inspection and demo surface.

WTG v0.2.0-beta6 adds an experimental MQTT watch sink. WTG can publish live `--watch` telemetry to a user-specified MQTT broker using predictable topics. WTG is an MQTT publisher, not a broker: it does not expose a listening network service, configure the broker, open firewall rules, or manage subscriber access.

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

WTG includes additional CLI paths for probe, field-value, and sink validation. These diagnostic paths were introduced during beta 4 probe/probe-fields work and remain available in beta 6; they are not broad release-contract guarantees.

The diagnostic CLI scope is intentionally narrow:

- preserve existing `--once`, `--watch`, and `--stats` behavior
- add probe-oriented diagnostic output
- add structured CSV output for snapshot, stats, probe, and probe-fields paths
- add line-oriented JSONL sink output for snapshot, stats, probe, and probe-fields paths
- add experimental MQTT publishing for live `--watch` snapshots
- add experimental raw NVML field-value probing through explicit field IDs
- avoid interpreting raw field IDs as proof of driver causality in code or documentation

### Modes

- `--once`  
  Capture a single GPU snapshot and exit.

- `--watch`  
  Continuously poll GPU state at a fixed interval.

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
  MQTT broker host. Required with `--sink mqtt`.

- `--mqtt-port <port>`
  MQTT broker port. Default: `1883`.

- `--mqtt-topic-prefix <prefix>`
  MQTT topic prefix. Default: `wtg`.

- `--mqtt-node-id <id>`
  Stable WTG node identifier used in MQTT topics. Required with `--sink mqtt`.

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

Note: plain `--watch` and `--watch --stats` currently have different recovery behavior. `--watch --stats` uses a persistent NVML context and attempts to reinitialize after snapshot failures. Plain `--watch` is stricter and exits on snapshot failure. This behavior may be unified later.

CSV sinks emit exactly one header row per sink file. Unsupported optional values are written as `N/A`.

MQTT publishes live snapshot payloads only while WTG is running and connected to the broker. Messages are QoS 0 and not retained, so subscribers receive samples only while connected. WTG is an MQTT publisher, not a broker. Broker setup, firewall policy, retention, and subscriber access are outside WTG's scope.

### Experimental MQTT watch sink

The MQTT sink publishes live telemetry from `--watch` to a user-specified broker.

Example:

```powershell
.\wtg.exe --watch --sink mqtt --mqtt-host 127.0.0.1 --mqtt-port 1884 --mqtt-node-id testnode
```

Topic shape:

```text
wtg/<node_id>/gpu<index>/state
```

Example topic:

```text
wtg/testnode/gpu0/state
```

Example payload:

```json
{
  "wtg_version": "0.2.0-beta6",
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

Current beta behavior:

* `--sink mqtt` is supported only with `--watch`
* WTG opens an outbound connection to the configured broker
* WTG does not expose a listening network service
* one JSON state payload is published per GPU per watch tick
* payloads include watch tick metadata, `GpuSnapshot` values, and the same probe context fields exposed by probe surfaces
* topic prefix defaults to `wtg`
* payloads are live QoS 0 messages
* payloads are not retained
* no Home Assistant discovery is emitted
* no config file support is included
* WTG does not install, run, or configure the broker

Local broker test shape:

```text
WTG -> local MQTT broker -> local subscriber
```

Observability-stack shape:

```text
WTG host -> existing MQTT broker -> subscribers such as Home Assistant, MQTT Explorer, dashboards, or scripts
```

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
   * No UI, MQTT, or output-sink logic in backend
3. **Surfaces = Consumers**:

   * `wtg.exe` is the CLI validation, capture, probe, and sink surface.
   * `wtg-ui.exe` is an experimental egui visual surface.
   * MQTT is an experimental watch-mode delivery surface for publishing live telemetry to an existing broker.
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
4. **Home Assistant discovery**: optional, explicit discovery config generation after MQTT transport remains stable
5. **UI refinement**: module split, display polish, and optional charting only after validation workflows remain stable
6. **Native window control**: chrome, always-on-top, keyboard shortcuts
7. **Tray integration**: background polling, popup on demand
8. **Plugin expansion**: ROCm, DX12, vendor-specific metrics
9. **Distribution hardening**: signed binary, single-file EXE, crash-resilient NVML paths
10. **Reference surfaces retained**: CLI remains the validation surface; egui remains a visual/debug/operator surface unless explicitly promoted later

**Key Principle:** Surfaces are interchangeable lenses; backend is the immutable source of truth. This prevents logic drift and double-work while allowing phased expansion.

---

## Milestones

| Version / Branch | Goal |
| ------- | ---- |
| v0.1.x | Truth-layer validation: one GPU, CLI output, correct NVML metrics vs Windows telemetry |
| v0.2.0-beta4 | Probe, sink, field-values, and driver-behavior validation |
| v0.2.0-beta5 | Experimental dual-surface build: CLI validation surface plus `wtg-ui.exe` visual frontend |
| v0.2.0-beta6 | Experimental MQTT watch sink for publishing live telemetry to a user-specified broker |
| v0.3+ | Optional native UI, tray integration, cross-vendor extensibility |

---

## Next Immediate Step

* Preserve CLI/probe/sink outputs as the formal validation path.
* Validate the experimental MQTT sink against a local broker and subscriber before adding Home Assistant discovery.
* Use `wtg-ui.exe` for visual corroboration, demos, and operator-facing inspection.
* Validate packaging helper behavior on release builds that include both executable surfaces.
* Continue empirical driver-behavior documentation using captured CLI and structured artifacts.
