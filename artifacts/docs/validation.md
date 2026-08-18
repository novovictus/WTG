# Validation

WTG uses empirical, provider-aware validation. It preserves observed provider values and avoids attributing root cause without supporting evidence.

## Test conditions

- Record hardware, Windows build, driver version, provider runtime, and power source.
- Capture idle and controlled-load states.
- Use the same hardware and procedure for cross-driver comparisons.
- Treat laptop AC/battery state as a controlled variable.

## Reference commands

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
.\target\release\wtg.exe --provider intel --once
.\target\release\wtg.exe --probe
.\target\release\wtg.exe --probe-fields --field-id 74
```

## Evidence boundaries

- CLI, provider output, probes, and local sink artifacts validate collection and serialization.
- MQTT broker/subscriber captures validate publication and transport.
- Home Assistant device and entity creation validates discovery behavior.
- The desktop UI provides visual corroboration only.
- Redline templates, states, and dashboards validate downstream presentation only.

Do not treat MQTT, Home Assistant, Redline, or the UI as alternate provider authorities. Do not treat AMD or Intel fields as NVIDIA/NVML equivalents.

## NVIDIA memory-controller regression

On affected Windows WDDM and NVIDIA driver combinations, `nvmlDeviceGetUtilizationRates().memory` may report 100 percent at idle while VRAM occupancy, temperature, power, and GPU utilization remain low.

This is visible through both WTG and `nvidia-smi` and has been observed on consumer mobile Ampere beginning with the 580.88 branch. It has not been reproduced on tested desktop or professional Ampere systems.

`util.mem_controller_pct` is memory-controller utilization, not VRAM occupancy. Interpret it alongside `vram.used_mib`, `vram.total_mib`, power, temperature, and workload state.

## Comparison strategy

- Compare NVIDIA output against `nvidia-smi`, WSL NVML, and Windows-reported metrics where useful.
- Compare provider-native output against the corresponding provider surface, not a normalized cross-vendor model.
- Preserve unsupported and unavailable states.
- Capture identical probe and field-value requests across driver versions.
- Avoid inferring causality from one API call succeeding or one display disagreeing.

For automated watch checks, avoid using `Select-Object -First ...` as final evidence because it intentionally closes stdout and may trigger a broken-pipe panic. Manual termination is cleaner for final run notes.
