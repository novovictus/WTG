# Desktop UI

`wtg-ui.exe` is an experimental egui desktop frontend and launcher.

It displays provider-backed telemetry from NVIDIA NVML, AMD ADL, and Intel Level Zero where the corresponding provider and runtime are available. Telemetry-capable devices share one selectable device list; provider-specific facts remain provider-scoped.

## Build and run

```powershell
cargo build -p wtg-app --release
.\target\release\wtg-ui.exe
```

The release build produces both `wtg.exe` and `wtg-ui.exe`. Keep the matching executables beside each other because UI launch actions invoke `wtg.exe`.

## Device behavior

- NVIDIA devices use the existing NVML path.
- AMD rows represent telemetry-capable AMD ADL adapters, not topology-only duplicates.
- Intel rows represent Level Zero devices.
- Unavailable values remain unavailable rather than rendering as zero.
- The UI does not synthesize cross-vendor parity fields.

## Configuration and launcher behavior

The UI is a convenience layer over the CLI configuration and runtime. Depending on the current build, it can load, save, or generate config; copy equivalent CLI commands; test broker connectivity; clear retained Home Assistant discovery; and launch or stop the CLI publisher.

Launching the publisher uses the saved config on disk. Save edited fields before launch. A launched `wtg.exe` process is detached and can continue after the UI exits.

The UI configures WTG runtime and transport behavior only. It does not install Home Assistant dashboards or configure the Redline presentation layer.

## Validation boundary

The UI is useful for visual corroboration and demos, but it is not the reference surface for regression testing or metric capture. Formal evidence should come from `wtg.exe`, provider output, probes, and structured sinks.

## Windows application control

`wtg-ui.exe` is currently unsigned. Smart App Control, Windows Defender Application Control, App Control for Business, or enterprise code-integrity policy may block it.

A policy block does not imply a provider failure or malicious binary. Use an appropriately controlled development system, sign or allowlist the binary according to local policy, or use `wtg.exe` for CLI validation. Do not disable organization-managed application-control policy without authorization.
