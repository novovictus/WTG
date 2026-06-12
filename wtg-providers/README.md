# WTG Providers Spike

This crate is an isolated experimental provider boundary for vendor-native probes.

- It is not added to the root workspace.
- It does not modify `wtg-core`.
- It does not modify `wtg-app`, existing WTG output schemas, MQTT, CSV, JSONL, or Home Assistant behavior.

Build independently:

```powershell
cargo build --manifest-path .\wtg-providers\Cargo.toml
```

Run the AMD ADL proof-of-life probe:

```powershell
cargo run --manifest-path .\wtg-providers\Cargo.toml --bin wtg-provider-probe -- amd-adl --once
```

## Current validation status

Validated on dev/0.2.6 after rebasing onto v0.2.5/main.

Local proof-of-life status:

- `cargo build --manifest-path .\wtg-providers\Cargo.toml --bin wtg-provider-probe` passes.
- `wtg-provider-probe.exe amd-adl --watch --interval-ms 1000` emits structured JSON samples.
- ADL loads from `C:\WINDOWS\SYSTEM32\atiadlxx.dll`.
- ADL initialization succeeds.
- ADL enumerates AMD Radeon integrated graphics display records.
- ADL also returns NVIDIA RTX 3080 Laptop GPU display records on the tested hybrid laptop.
- Non-AMD records are preserved with provider warnings rather than normalized into WTG/NVML truth fields.
- The provider remains isolated from `wtg-core`, `wtg-app`, MQTT, CSV, JSONL, and Home Assistant behavior.

This branch is an experimental provider-boundary spike, not a mainline WTG telemetry integration.
