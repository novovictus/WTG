# Intel Level Zero Provider Scaffold

This spike adds an explicit Intel provider-scoped discovery path selected with `--provider intel`.

The Intel path is a supporting provider witness. It does not change the NVIDIA/NVML primary truth path, does not change the AMD ADL provider path, and does not translate Intel facts into NVML-equivalent fields.

## Current Status

Current branch: `spike/0.2.8-intel-level-zero-provider`

Current version: `0.2.8`

Current implementation status:

```text
Intel provider type: scaffold / discovery spike
Runtime target: Intel Level Zero loader
Runtime DLL: ze_loader.dll
Provider source: wtg.provider.intel.level_zero
Telemetry class: provider_telemetry
Stats schema: wtg.intel_level_zero.stats.v1
```

On the current test machine, the Intel Level Zero runtime is not installed. The provider therefore fails cleanly as unavailable instead of crashing or inventing telemetry.

Observed unavailable result:

```text
Provider status: unavailable
Reason: Intel Level Zero runtime DLL ze_loader.dll was not found.
```

## Current CLI Contract

Supported Intel provider commands:

```powershell
.\target\release\wtg.exe --once --provider intel
.\target\release\wtg.exe --watch --provider intel --interval 1000
.\target\release\wtg.exe --once --stats --provider intel
.\target\release\wtg.exe --watch --stats --provider intel --interval 1000
.\target\release\wtg.exe --probe --provider intel
```

Intentional rejections and omissions:

```text
Intel --probe-fields: rejected; NVML field IDs are provider-specific
Intel sinks: rejected
Intel MQTT/Home Assistant publishing: not implemented
Intel JSONL/CSV sink schema: not committed
Intel telemetry class: provider_telemetry
```

The provider also rejects invalid mixed runtime modes such as `--once --probe --provider intel` through the existing WTG mode validation.

## Mode Alignment

Intel follows the same WTG mode split used by NVIDIA/NVML and AMD ADL:

```text
--once:
  compact human snapshot

--watch:
  repeated compact human snapshot

--stats:
  structured JSON stats/provenance output

--probe:
  compact key:value probe output
```

The Intel stats schema is provider-native:

```text
schema: wtg.intel_level_zero.stats.v1
provider: intel
provider_authority: Intel Level Zero
provider_source: wtg.provider.intel.level_zero
telemetry_class: provider_telemetry
```

This schema is not an NVML compatibility schema. It preserves Intel Level Zero facts, source APIs, states, units, and unavailable/error results without inventing missing NVML-equivalent fields.

## Runtime Loading Behavior

The provider dynamically loads the Intel Level Zero runtime DLL:

```text
ze_loader.dll
```

The current scaffold does not hard-link against Level Zero at build time. It attempts to load exported Level Zero functions at runtime and reports a clean unavailable state when the DLL or required symbols are missing.

Required Level Zero entry points currently loaded:

```text
zeInit
zeDriverGet
zeDeviceGet
zeDeviceGetProperties
```

Current export count:

```text
telemetry_exports_matched: 4
```

## Current Discovery Surface

If Level Zero initializes and returns drivers/devices, the provider currently attempts:

```text
zeInit
zeDriverGet
zeDeviceGet
zeDeviceGetProperties
```

Per-device facts currently derived from `zeDeviceGetProperties` when available:

```text
device_name
device_key
device_type
vendor_id
device_id
core_clock_mhz
uuid, when non-zero
```

The device key is WTG provider-scoped and currently shaped as:

```text
driver=<driver_index>,device=<device_index>,vendor=0x<vendor_id>,device=0x<device_id>
```

Unavailable provider-scoped facts are summarized rather than faked:

```text
activity
memory
power
temperature
```

## Unavailable Runtime Shape

When `ze_loader.dll` is missing, human snapshot output is expected to look like:

```text
WTG snapshot mode (provider: Intel Level Zero)
Provider source: wtg.provider.intel.level_zero
Telemetry class: provider_telemetry

Provider status: unavailable
Reason: Intel Level Zero runtime DLL ze_loader.dll was not found.
```

Probe output is expected to look like:

```text
[probe] provider=intel_level_zero
wtg.version: 0.2.8
provider.authority: Intel Level Zero
provider.source: wtg.provider.intel.level_zero
telemetry.class: provider_telemetry
provider.status: unavailable
reason: Intel Level Zero runtime DLL ze_loader.dll was not found.
```

Stats output remains parseable JSON even when unavailable. It should still include:

```text
schema: wtg.intel_level_zero.stats.v1
provider: intel
provider_authority: Intel Level Zero
provider_source: wtg.provider.intel.level_zero
telemetry_class: provider_telemetry
```

The unavailable reason should be preserved in the relevant provenance wrapper instead of being collapsed into a fake device or fake zero telemetry.

## Expected Available Runtime Shape

When Intel Level Zero is installed and returns devices, compact human output should remain provider-scoped and factual:

```text
WTG snapshot mode (provider: Intel Level Zero)
Provider source: wtg.provider.intel.level_zero
Telemetry class: provider_telemetry

Intel driver records returned: <n>
Intel device records returned: <n>

Intel device 0: <provider-reported name>
  Device key: driver=<driver_index>,device=<device_index>,vendor=0x<vendor_id>,device=0x<device_id>
  Device type: <provider-reported type>
  UUID: <provider-reported UUID if non-zero>
  Core clock: <provider-reported MHz> MHz
  Unavailable: activity, memory, power, temperature
```

The current implementation does not yet claim utilization, memory usage, power, or temperature support.

## Validation Baseline

Validation reported for the scaffold:

```text
cargo fmt
cargo test
cargo build -p wtg-app --release
```

Runtime checks reported:

```powershell
.\target\release\wtg.exe --once --provider intel
.\target\release\wtg.exe --once --stats --provider intel
.\target\release\wtg.exe --probe --provider intel
.\target\release\wtg.exe --watch --provider intel --interval 1000
.\target\release\wtg.exe --watch --stats --provider intel --interval 1000
```

Expected rejections reported:

```powershell
.\target\release\wtg.exe --probe-fields --provider intel --field-id 1
.\target\release\wtg.exe --once --provider intel --sink jsonl
.\target\release\wtg.exe --watch --provider intel --sink csv
.\target\release\wtg.exe --watch --provider intel --sink mqtt
.\target\release\wtg.exe --once --probe --provider intel
```

## Non-Goals

This spike intentionally does not add:

- PDH
- WMI
- DXGI
- ETW
- Windows power APIs
- Task Manager / PerfMon counters
- MQTT / Home Assistant publishing for Intel
- JSONL / CSV sink contract for Intel
- NVML-equivalent Intel field names
- fake zero values for missing Intel facts
- cross-vendor telemetry normalization

## Candidate Future Expansions

Promising next areas, still provider-scoped if added:

- validate on a system with Intel Level Zero installed
- confirm structure type handling against real devices
- add explicit Level Zero / Sysman constants instead of discovery probing where appropriate
- query memory properties / memory state if exposed through a safe Level Zero or Sysman path
- query power, temperature, frequency, and utilization if exposed through safe provider-native calls
- record driver/runtime version if exposed by the provider surface
- add Intel-specific docs once a live-runtime artifact exists

## Documentation Rule

Operational human CLI output should stay compact and factual:

- provider/source/class
- provider status
- driver/device counts
- compact provider-native identity facts
- compact unavailable/error summaries

Structured JSON stats output is allowed to be detailed because it is explicitly requested with `--stats`. It should preserve provider provenance and unavailable/error states without turning Intel Level Zero into an NVML compatibility layer.

## Pruning Rule

If a candidate Intel API needs risky control flow, write behavior, session/shared-memory management, or unclear structure definitions, keep the note here and do not surface it in normal CLI output until it is safely understood.
