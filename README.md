# WTG — WhatTheGPU

**Tagline:** Honest GPU compute stats for Windows

---

## Project Purpose

Windows lacks native, real-time, per-process CUDA utilization metrics. Task Manager, PerfMon, and Windows Telemetry report engine scheduling and memory residency but **do not report actual GPU SM occupancy, kernel execution, per-process CUDA utilization, or stream concurrency**. Existing tools are either:

* Linux-first (nvtop, nvitop, nviwatch), partially compatible on Windows
* Wrappers around `nvidia-smi` (slow, fragile)
* Windows-native but non-CUDA-specific (GPU-Z, HWiNFO), showing only aggregate GPU load

WTG fills the gap: **real-time, Windows-native, CUDA-specific GPU monitoring**, providing honest per-process metrics and bypassing Windows telemetry abstractions.

---

## Project Scope (v0.1)

* **Target**: NVIDIA GPUs, CUDA metrics only
* **Platform**: Windows-native, single executable (`wtg.exe`)
* **Metrics**:

  * Per-process GPU utilization (SM-level via NVML)
  * VRAM used/reserved per PID
  * Power draw, clocks (contextual)
  * Exclude WDDM/Task Manager compute % from “truth” layer
* **Refresh Rate**: 250–500 ms
* **UI**: Initially TUI (text interface) for truth validation, later minimal egui window

---

## Current Usage (v0.1)

WTG is currently a command-line proof-of-concept focused on validating NVML-based GPU telemetry on Windows.

### Modes

- `--once`  
  Capture a single GPU snapshot and exit.

- `--watch`  
  Continuously poll GPU state at a fixed interval.

- `--interval <ms>`  
  Polling interval in milliseconds.  
  Default: `1000`  
  Only applies when `--watch` is specified.

### Examples

One-shot snapshot:
```bash
wtg.exe --once

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

   * Phase 1: TUI validation, metric truth locked
   * Phase 2: Minimal egui table window
   * Phase 3: Optional tray integration and UI polish
6. **Repository / Licensing**: MIT or Apache-2.0, open-source, GitHub-hosted

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
* Verify short vs dense kernels, proving Windows telemetry is inaccurate
* TUI allows fast, honest metric validation

---

## Phased UI Progression

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
| v0.1    | Proof-of-truth: one GPU, TUI window, correct NVML metrics vs Windows telemetry  |
| v0.2    | Minimal egui table window, sub-second refresh, immutable snapshot visualization |
| v0.3+   | Optional full Windows-native UI, tray integration, cross-vendor extensibility   |

---

## Next Immediate Step

* Implement **v0.1 TUI** in Rust
* Validate NVML metric pipeline, refresh loop, per-process attribution
* Capture screenshots to prove correctness
* Lock snapshot structure and contract for downstream UI layers