WTG is licensed under the GNU General Public License v3.0 (GPLv3). See LICENSE.
Copyright (C) 2026 Adam Hooper

# WTG - WhatTheGPU

**Tagline:** Honest GPU compute stats for Windows

## Current Status (v0.2.0-beta3 – Empirical Validation Phase)

WTG is currently focused on empirical NVML telemetry validation under Windows WDDM.  
Recent testing has identified a driver-branch regression affecting memory-utilization reporting on specific consumer mobile Ampere GPUs (580.88+), not reproduced on tested desktop or professional SKUs.
Findings reflect publicly accessible NVML telemetry behavior under Windows WDDM.
See `artifacts/test-matrix/matrix.md` for empirical results.

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
* **Platform**: Windows-native, single executable (`wtg.exe`)
* **Metrics**:

  * Per-process memory attribution and NVML-reported utilization metrics
  * VRAM used/reserved per PID
  * Power draw, clocks (contextual)
  * Exclude WDDM/Task Manager compute % from “truth” layer
* **Refresh Rate**: 250–500 ms
* **UI**: Initially TUI (text interface) for truth validation, later minimal egui window

---

## Current Usage (CLI Engine)

WTG is currently a command-line proof-of-concept focused on validating NVML-based GPU telemetry on Windows.

### Current probe/sink branch behavior

This branch includes additional CLI paths for probe, field-value, and sink validation. These are current development behaviors, not release-contract guarantees.

### Modes

- `--once`  
  Capture a single GPU snapshot and exit.

- `--watch`  
  Continuously poll GPU state at a fixed interval.

- `--interval <ms>`  
  Polling interval in milliseconds.  
  Default: `1000`  
  Only applies when `--watch` is specified.

- `--stats`  
  Print the stable key:value stats format. Requires `--once` or `--watch`.

- `--probe`  
  Capture one snapshot and print a minimal probe block for field validation.

- `--probe-fields`  
  Experimental console-only mode that captures one snapshot, prints the normal utilization path, and then queries selected NVML field-value IDs.

- `--field-id <u32>`  
  Repeatable field ID argument for `--probe-fields`. At least one `--field-id` is required when using `--probe-fields`.

- `--sink jsonl`  
  Create a timestamped `wtg_sink_*.jsonl` file. JSONL sinks currently write `{"line":"..."}` records for `--once`, non-stats `--watch`, and `--probe`.

- `--sink csv`  
  Create a timestamped `wtg_sink_*.csv` file. CSV currently writes structured header + row output for `--probe` only. With `--once` or non-stats `--watch`, CSV files are created but no rows are written yet.

### Probe context fields

`--probe` and `--probe-fields` include runtime context fields intended to support same-GPU, cross-driver comparisons:

- `wtg.version`
- `driver.version`
- `cuda.driver_version`
- `gpu.compute_mode`
- `gpu.pci.bus_id`

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

Probe field-values comparison:

```powershell
cargo run -- --probe-fields --field-id 74 --field-id 78 --field-id 83 --field-id 94 --field-id 95
```

---

### Driver Requirements

WTG relies on NVIDIA NVML on Windows. Empirical testing shows:

- Drivers prior to ~470 may ship `nvidia-smi` without a usable `nvml.dll`
- Modern drivers (≥580) consistently expose NVML across tested SKUs
- WTG fails fast when NVML is unavailable and prints an explicit error when possible

---

## Key Decisions

1. **Language**: Rust (memory safe, low overhead, native FFI for NVML)
2. **Backend = Source of Truth**:

   * NVML bindings, polling, aggregation, process attribution, snapshot emission
   * Immutable snapshots, append-only, versioned
   * No UI logic in backend
3. **UI Layers = Consumers**:

   * TUI for fast validation
   * egui for minimal Windows-native window
   * Optional full Windows-native UI later (Win32/WinUI)
   * Both UI layers consume the same snapshot format; no backend duplication
4. **Extensible Metric Model**: trait-based, allows future integration of other vendors (ROCm, DX12)
5. **Phased Approach**:

   * Phase 1 (In progress): TUI validation and snapshot contract stabilization.
   * Phase 2 (Planned): Minimal egui table window.
   * Phase 3: Optional tray integration and UI polish.
6. **Repository / Licensing**: GitHub-hosted / GPLv3.

---

## Technical Architecture

```
wtg-core/         # Backend / truth layer
  nvml/           # NVML bindings
  metrics/        # Metric providers & trait definitions
  scheduler/      # Refresh loop
  snapshot/       # Immutable Snapshot structs & process mapping

wtg-app/
  ui-egui/        # Immediate-mode table, minimal window
  ui-native/      # Optional full Windows-native UI (Win32/WinUI)
wtg-view/         # Shared view helpers: sorting, filtering, formatting
```

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
* Sorting, filtering, and column helpers shared in `wtg-view/`
* OS-specific differences handled only in UI layers

---

## NVML Integration

* Direct NVML FFI (`nvml-sys` or custom bindings)
* Dynamically load NVML DLL at runtime
* Graceful failure handling if driver absent
* Per-process aggregation including short-lived kernels

---

## Refresh Loop

* Fixed timestep (250–500 ms)
* No smoothing; snapshots reflect raw, instantaneous utilization
* Diff snapshots for UI efficiency

---

## Validation Strategy

* Compare WTG output vs `nvidia-smi` and WSL NVML metrics
* Compare NVML telemetry against Windows-reported metrics to characterize abstraction differences.
* TUI allows fast, metric validation

---

## Early Validation Artifacts

Empirical GPU / driver test results are summarized in `artifacts/test-matrix/matrix.md`.

---

## Architectural Roadmap (Post-Validation)

1. **TUI (Phase 1)**: truth validation, fast refresh, developer-grade snapshots
2. **egui (Phase 2)**: minimal window, table view only, sub-second refresh
3. **Native window control**: chrome, always-on-top, keyboard shortcuts
4. **Tray integration**: background polling, popup on demand
5. **Optional native panels**: use egui for tables, native only for tray/notifications/startup hooks
6. **Plugin expansion**: ROCm, DX12, vendor-specific metrics
7. **Distribution hardening**: signed binary, single-file EXE, crash-resilient NVML paths
8. **Reference UI retained**: egui remains the canonical dev/debug interface

**Key Principle:** UI layers are interchangeable lenses; backend is the immutable source of truth. This prevents logic drift and double-work while allowing phased expansion.

---

## Milestones

| Version | Goal                                                                            |
| ------- | ------------------------------------------------------------------------------- |
| v0.1    | Truth-layer validation: one GPU, TUI window, correct NVML metrics vs Windows telemetry  |
| v0.2    | Minimal egui table window, sub-second refresh, immutable snapshot visualization |
| v0.3+   | Optional full Windows-native UI, tray integration, cross-vendor extensibility   |

---

## Next Immediate Step

* Complete v0.1 TUI implementation and finalize snapshot contract
* Validate NVML metric pipeline, refresh loop, per-process attribution
* Capture screenshots to prove correctness
* Lock snapshot structure and contract for downstream UI layers
