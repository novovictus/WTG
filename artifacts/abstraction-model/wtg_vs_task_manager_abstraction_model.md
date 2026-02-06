## Task Manager vs WTG — Abstraction Model (Validated)

This section explains where **WTG** sits in the OS / driver stack relative to **Windows Task Manager**, and why the two tools can legitimately report different-looking GPU behavior while observing the *same hardware*.

---

## Side‑by‑side signal paths (shared root)

```
        TASK MANAGER PATH                          WTG PATH
───────────────────────────────────     ───────────────────────────────────
User                                    User
 │                                       │
 ▼                                       ▼
Task Manager (GUI)                      WTG (CLI / agent)
 │                                       │
 ▼                                       ▼
Windows Performance Model               NVML
(Perf counters / WMI / ETW)             (Vendor mgmt API)
 │                                       │
 ▼                                       ▼
DXGI                                    NVIDIA user‑mode driver
(Graphics abstraction)                  (telemetry plumbing)
 │                                       │
 ▼                                       │
WDDM Scheduler & Accounting              │
(OS time slicing, heuristics)            │
 │                                       │
 └──────────────┐            ┌───────────┘
                ▼            ▼
        Kernel‑mode GPU Driver (nvlddmkm.sys)
                    │
                    ▼
                GPU Hardware
```

---

## What is shared (no smoke)

- **Single kernel driver**: Both Task Manager and WTG ultimately rely on the *same* NVIDIA kernel‑mode GPU driver (`nvlddmkm.sys`).
- **Same hardware**: Both paths terminate at the same physical GPU.
- **Same privilege level**: Both Task Manager and WTG run entirely in **user mode**.

There are no parallel kernel drivers, hidden execution paths, or privileged shortcuts.

---

## Where the paths diverge

### Task Manager

Task Manager observes the GPU through **OS‑owned abstractions**:

- Windows performance frameworks
- DXGI (graphics‑centric, vendor‑neutral)
- WDDM scheduler and memory accounting
- OS heuristics, smoothing, and normalization

By design, these layers:
- aggregate across engines
- average over time windows
- hide bursty or causal detail
- prioritize UX consistency and comparability

Task Manager answers the question:
> *From the OS scheduler’s point of view, how busy does the GPU appear?*


### WTG

WTG bypasses OS performance abstractions and instead queries **NVML**, NVIDIA’s management interface:

- Vendor‑defined, driver‑backed contract
- Exposes driver‑maintained hardware state
- Designed for diagnostics, telemetry, and capacity planning

WTG deliberately skips:
- DXGI
- WDDM scheduler interpretation
- OS heuristics and normalization

WTG answers the question:
> *What does the GPU driver know about what the GPU is actually doing?*

---

## “Closer to the kernel” — precise meaning

WTG is **closer in abstraction distance**, not closer in privilege.

- NVML runs in user mode
- NVML communicates with the kernel driver via supported IOCTLs
- The kernel driver remains the enforcement boundary

WTG does **not**:
- execute in kernel mode
- access hardware directly
- bypass driver safety or OS protections

The advantage is fewer interpretive layers between signal and source, not elevated access.

---

## Canonical takeaway

> Task Manager and WTG observe the same GPU through the same kernel driver, but Task Manager goes up through OS abstractions while WTG goes down through the vendor management interface, removing layers between the signal and the source.

Or, more compactly:

> **Same kernel driver. Same hardware. Fewer abstractions.**

---

## Why this matters for WTG

WTG is not a replacement for Task Manager.

It exists because OS‑level abstractions intentionally hide causality. When GPU workloads are memory‑, latency‑, or scheduling‑bound, Task Manager can appear idle while the GPU is working correctly. WTG explains *why*.

This is the technical justification for WTG’s scope and for its focus on stable, driver‑truth telemetry rather than OS‑derived utilization estimates.

