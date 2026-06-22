# Probe and Probe-Fields

`--probe` and `--probe-fields` are diagnostic validation surfaces.

## Probe context fields

`--probe` and `--probe-fields` include runtime context fields intended to support same-GPU, cross-driver comparisons:

- `wtg.version`
- `driver.version`
- `cuda.driver_version`
- `gpu.compute_mode`
- `gpu.perf_state`
- `gpu.pci.bus_id`

`gpu.perf_state` reports the NVML performance state, such as `P0` through `P15` or `Unknown`. `P0` is the highest-performance state. Higher-numbered states are lower-power states. `N/A` means the query was unsupported or failed.

Structured `--probe --sink csv` and `--probe-fields --sink csv` outputs include the same context as CSV columns, including `gpu_perf_state`.

## Memory-controller utilization

`util.mem_controller_pct` is NVML memory-controller utilization, not VRAM occupancy.

VRAM occupancy is reported separately as:

```text
vram.used_mib
vram.total_mib
```

On some Windows WDDM / NVIDIA driver combinations, NVML memory utilization may report `100%` at idle or low VRAM occupancy.

This condition does not mean VRAM is full.

Example:

```text
util.mem_controller_pct: 100
vram.used_mib: 759
vram.total_mib: 16384
```

This means the NVML memory-utilization counter is pegged while allocated VRAM remains low.

## Probe-fields mode

`--probe-fields` compares WTG's normal NVML utilization path against selected field-values queries through the safe `nvml-wrapper` API for `nvmlDeviceGetFieldValues`.

Example:

```powershell
cargo run -p wtg-app --bin wtg -- --probe-fields --field-id 74 --field-id 78 --field-id 83 --field-id 94 --field-id 95
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

Field-values queries working for supported field IDs show that the field-values API is callable on the same device/session. This does not by itself prove driver causality.

Cross-driver comparison still requires capturing the same `--probe` and `--probe-fields` outputs on different NVIDIA driver versions.
