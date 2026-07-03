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
Latest code commit: e421618 Refine Intel Level Zero device property output
```

The Intel provider has now been validated in both runtime states:

```text
runtime missing:
  provider.status: unavailable
  reason: Intel Level Zero runtime DLL ze_loader.dll was not found.

runtime present on bench:
  provider.status: ok
  driver records: 1
  device records: 1
  zeDeviceGetProperties: ok
```

This remains a discovery/property scaffold. It proves that WTG can load Level Zero, enumerate an Intel GPU device, and emit provider-native property facts. It does not yet claim live Intel utilization, memory, power, or temperature telemetry.

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
device_name, if non-empty
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
name, when Level Zero returns an empty device name
activity
memory
power
temperature
```

## Observed Bench Result

Live bench capture proved the Level Zero path is alive for the bench Intel iGPU. The provider loaded, initialized, enumerated one driver and one device, and returned device properties.

Observed compact human output after `e421618`:

```text
WTG snapshot mode (provider: Intel Level Zero)
Provider source: wtg.provider.intel.level_zero
Telemetry class: provider_telemetry

Intel driver records returned: 1
Intel device records returned: 1

Intel device 0 [driver=0,device=0,vendor=0x8086,device=0x4c8b]
  Device type: gpu
  Vendor ID: 0x8086 (32902)
  Device ID: 0x4c8b (19595)
  UUID: 240000002000000086808b4c04000000
  Core clock: 1300.0 MHz
  Unavailable: name, activity, memory, power, temperature
```

Observed stats/provenance highlights:

```text
provider: intel
provider_authority: Intel Level Zero
provider_source: wtg.provider.intel.level_zero
schema: wtg.intel_level_zero.stats.v1
telemetry_class: provider_telemetry

intel.driver_record_count.raw: 1
intel.device_record_count.raw: 1
intel.telemetry_exports_matched.raw: 4
intel.optional_calls_attempted.raw: 1
intel.optional_calls_ok.raw: 1
intel.optional_calls_error.raw: 0
```

Observed device facts:

```text
device_key.raw: driver=0,device=0,vendor=0x8086,device=0x4c8b
device_type.raw: gpu
vendor_id.raw: 32902
device_id.raw: 19595
uuid.raw: 240000002000000086808b4c04000000
core_clock_mhz.raw: 1300
```

Level Zero returned an empty device name on this bench. WTG now preserves that as a provider-scoped unavailable fact instead of reporting a successful empty string:

```text
device_name.raw: null
device_name.state: not_available
device_name.source_api: zeDeviceGetProperties
device_name.error_message: Level Zero returned an empty device name.
```

## Watch Stats Observation

`--watch --stats --provider intel` emits one structured JSON object per tick and increments `tick_seq` as expected.

Bench watch capture showed stable ticks from `0` through `18` with the same provider identity, schema, driver/device counts, device key, vendor/device IDs, UUID, and `core_clock_mhz` value.

The repeated value:

```text
core_clock_mhz.raw: 1300
source_api: zeDeviceGetProperties
```

should be treated as a Level Zero property value, not live changing frequency telemetry. A GPU stress spike did not create new Intel indicators in the current output because the provider does not yet query Level Zero Sysman or other live activity, power, temperature, memory, or utilization APIs.

Current Intel watch/stats proves:

```text
Level Zero enumeration is stable across ticks
JSON stats emission works across ticks
empty-name provenance remains correct across ticks
provider/source/schema/class remain stable across ticks
```

Current Intel watch/stats does not prove:

```text
Intel utilization telemetry
Intel memory usage telemetry
Intel power telemetry
Intel temperature telemetry
live Intel frequency telemetry
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

## Current Available Runtime Shape

When Intel Level Zero is installed and returns devices, compact human output should remain provider-scoped and factual:

```text
WTG snapshot mode (provider: Intel Level Zero)
Provider source: wtg.provider.intel.level_zero
Telemetry class: provider_telemetry

Intel driver records returned: <n>
Intel device records returned: <n>

Intel device <device_index> [driver=<driver_index>,device=<device_index>,vendor=0x<vendor_id>,device=0x<device_id>]
  Device type: <provider-reported type>
  Vendor ID: 0x<vendor_id> (<vendor_id decimal>)
  Device ID: 0x<device_id> (<device_id decimal>)
  UUID: <provider-reported UUID if non-zero>
  Core clock: <provider-reported MHz> MHz
  Unavailable: <compact unavailable summary>
```

The current implementation does not yet claim utilization, memory usage, power, temperature, or live frequency support.

## Validation Baseline

Validation reported for the scaffold and `e421618` cleanup:

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

- replace any remaining structure-type probing with explicit verified Level Zero constants
- query driver/runtime properties if exposed safely by Level Zero
- query memory properties or memory state if exposed through a safe Level Zero or Sysman path
- query live frequency through provider-native frequency domains if safely exposed
- query power, temperature, engine activity, and memory usage through Level Zero Sysman if exposed through safe provider-native calls
- preserve unavailable/error facts for every missing or unsupported live API
- keep Intel sinks rejected until a deliberate provider-native sink schema exists

Likely future Sysman candidates, not part of this scaffold:

```text
zesDeviceEnumFrequencyDomains / zesFrequencyGetState
zesDeviceEnumTemperatureSensors / zesTemperatureGetState
zesDeviceEnumPowerDomains / zesPowerGetEnergyCounter or power properties
zesDeviceEnumEngineGroups / zesEngineGetActivity
zesDeviceEnumMemoryModules / zesMemoryGetState
```

These calls need careful dynamic symbol loading and structure definitions. They should not be added as a late-night quick pass.

## Documentation Rule

Operational human CLI output should stay compact and factual:

- provider/source/class
- provider status
- driver/device counts
- compact provider-native identity/property facts
- compact unavailable/error summaries

Structured JSON stats output is allowed to be detailed because it is explicitly requested with `--stats`. It should preserve provider provenance and unavailable/error states without turning Intel Level Zero into an NVML compatibility layer.

## Pruning Rule

If a candidate Intel API needs risky control flow, write behavior, session/shared-memory management, or unclear structure definitions, keep the note here and do not surface it in normal CLI output until it is safely understood.
