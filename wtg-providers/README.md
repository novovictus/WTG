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
