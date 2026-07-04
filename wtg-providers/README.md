# WTG Providers

This crate contains experimental provider-boundary work for vendor-native telemetry sources outside the primary NVML path.

WTG provider priority remains:

1. NVIDIA / NVML as the primary provider.
2. AMD / ADL as a completed secondary experimental provider foundation.
3. Intel / Level Zero as the completed 0.2.9 discovery provider path and active 0.3.x usable-telemetry target.

The AMD ADL path was expanded through 0.2.8 as a provider-scoped telemetry foundation. The Intel Level Zero path is active in 0.2.9. It is included in the workspace for build and version alignment, and AMD ADL is invoked with `--provider amd`; Intel Level Zero is invoked with `--provider intel`.

The provider boundary preserves source semantics. ADL adapter records are exposed as ADL adapter records. They are not translated into NVML devices, Task Manager GPU numbers, or cross-provider parity fields.

## Build

Run the provider crate check from the repository root:

```powershell
cargo check --manifest-path .\wtg-providers\Cargo.toml
```


## Current validation status

Validated across the 0.2.6 through 0.2.9 provider development cycles.

Local proof-of-life status:

- The provider crate check passes.
- The AMD ADL provider path emits structured JSON samples through `wtg.exe --provider amd --stats`.
- ADL loads from the installed Windows AMD driver library.
- ADL initialization succeeds.
- ADL enumerates AMD Radeon integrated graphics display records.
- ADL may also return non-AMD display records on hybrid systems.
- Non-AMD records are preserved with provider warnings rather than normalized into WTG/NVML truth fields.

## Scope boundary

The provider work keeps AMD ADL and Intel Level Zero isolated from the primary NVIDIA/NVML path.

It does not change:

- NVML as the primary provider.
- CSV behavior.
- JSONL behavior.
- MQTT behavior.
- Home Assistant discovery behavior.
- Redline semantics.

Further provider telemetry expansion should remain provider-scoped and must not change NVIDIA/NVML truth semantics.

