# WTG Providers

This crate contains experimental provider-boundary work for vendor-native telemetry sources outside the primary NVML path.

WTG provider priority remains:

1. NVIDIA / NVML as the primary provider.
2. AMD / ADL as a secondary experimental provider.
3. Intel later.

The AMD ADL path introduced in 0.2.6 is a foundation, not a complete AMD telemetry implementation. It is included in the workspace for build and version alignment, and it is invoked from `wtg-app` only when explicitly requested with `--provider amd`.

The provider boundary preserves source semantics. ADL adapter records are exposed as ADL adapter records. They are not translated into NVML devices, Task Manager GPU numbers, or cross-provider parity fields.

## Build

Run the provider crate check from the repository root:

```powershell
cargo check --manifest-path .\wtg-providers\Cargo.toml
```

Run the AMD ADL proof-of-life probe from the repository root:

```powershell
cargo run --manifest-path .\wtg-providers\Cargo.toml --bin wtg-provider-probe -- amd-adl --once
```

## Current validation status

Validated during the 0.2.6 development cycle.

Local proof-of-life status:

- The provider crate check passes.
- The AMD ADL proof-of-life probe emits structured JSON samples.
- ADL loads from the installed Windows AMD driver library.
- ADL initialization succeeds.
- ADL enumerates AMD Radeon integrated graphics display records.
- ADL may also return non-AMD display records on hybrid systems.
- Non-AMD records are preserved with provider warnings rather than normalized into WTG/NVML truth fields.

## Scope boundary

The 0.2.6 ADL work establishes a secondary provider foundation only.

It does not change:

- NVML as the primary provider.
- CSV behavior.
- JSONL behavior.
- MQTT behavior.
- Home Assistant discovery behavior.
- Redline semantics.

Further ADL telemetry expansion is intentionally shelved until NVML provenance and expanded NVML stats are implemented in the 0.2.7 cycle.
