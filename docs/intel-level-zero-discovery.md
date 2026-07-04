# Intel Level Zero / Sysman Provider Discovery

This branch adds an explicit Intel provider-scoped discovery path selected with `--provider intel`.

The Intel path is a supporting provider witness. It does not change the NVIDIA/NVML primary truth path, does not change the AMD ADL provider path, and does not translate Intel facts into NVML-equivalent fields.

## Current Status

Current branch: `dev/0.3.0-provider-discovery`

Current version: `0.2.9`

Current implementation status:

```text
Intel provider type: Level Zero / Sysman discovery provider
Runtime target: Intel Level Zero loader
Runtime DLL: ze_loader.dll
Provider source: wtg.provider.intel.level_zero
Telemetry class: provider_telemetry
Stats schema: wtg.intel_level_zero.stats.v3
```

Bench validation has confirmed that the clean bench runtime exposes Intel Level Zero and Sysman through `ze_loader.dll`.

Validated bench target:

```text
Host: INSPIRON3861 / bench
Intel device: Intel UHD 730
Vendor ID: 0x8086
Device ID: 0x4c8b
UUID: 240000002000000086808b4c04000000
Core clock: 1300.0 MHz
```

Validated provider state:

```text
provider.status: ok
intel.driver_records: 1
intel.device_records: 1
intel.telemetry_exports_matched: 4
intel.sysman_exports_matched: 16
intel.sysman.zesInit_result: ok (raw=0)
```

## Current CLI Contract

Supported Intel provider commands:

```powershell
.\target\release\wtg.exe --once --provider intel
.\target\release\wtg.exe --watch --provider intel --interval 1000
.\target\release\wtg.exe --once --stats --provider intel
.\target\release\wtg.exe --watch --stats --provider intel --interval 250
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
schema: wtg.intel_level_zero.stats.v3
provider: intel
provider_authority: Intel Level Zero
provider_source: wtg.provider.intel.level_zero
telemetry_class: provider_telemetry
```

This schema is not an NVML compatibility schema. It preserves Intel Level Zero and Sysman facts, source APIs, states, units, unavailable results, and error results without inventing missing NVML-equivalent fields.

## Runtime Loading Behavior

The provider dynamically loads the Intel Level Zero runtime DLL:

```text
ze_loader.dll
```

The implementation does not hard-link against Level Zero at build time. It attempts to load exported Level Zero and Sysman functions at runtime and reports a clean unavailable state when the DLL or required symbols are missing.

Required base Level Zero entry points currently loaded:

```text
zeInit
zeDriverGet
zeDeviceGet
zeDeviceGetProperties
```

Optional Sysman entry points currently probed and used when available:

```text
zesInit
zesDeviceEnumEngineGroups
zesEngineGetProperties
zesEngineGetActivity
zesDeviceEnumMemoryModules
zesMemoryGetProperties
zesMemoryGetState
zesDeviceEnumPowerDomains
zesPowerGetProperties
zesPowerGetEnergyCounter
zesDeviceEnumTemperatureSensors
zesTemperatureGetProperties
zesTemperatureGetState
zesDeviceEnumFrequencyDomains
zesFrequencyGetProperties
zesFrequencyGetState
```

Validated bench export counts:

```text
telemetry_exports_matched: 4
sysman_exports_matched: 16
```

## Current Discovery Surface

If Level Zero initializes and returns drivers/devices, the provider attempts:

```text
zeInit
zeDriverGet
zeDeviceGet
zeDeviceGetProperties
```

Per-device facts derived from `zeDeviceGetProperties` when available:

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

After successful `zesInit`, the provider enumerates these Sysman domain groups per Level Zero device:

```text
engine_groups
memory_modules
power_domains
temperature_sensors
frequency_domains
```

For each domain group, the provider emits provider-scoped count/status facts. When handles exist and calls succeed, it emits raw handle/property/state facts using the exact Sysman API name in `source_api`.

Validated bench domain counts:

```text
sysman.engine_groups.count: 3
sysman.memory_modules.count: 1
sysman.power_domains.count: 1
sysman.temperature_sensors.count: 0
sysman.frequency_domains.count: 1
```

Validated bench domain status:

```text
sysman.engine_groups.status: ok
sysman.memory_modules.status: ok
sysman.power_domains.status: ok
sysman.temperature_sensors.status: not_available
sysman.frequency_domains.status: ok
```

The bench returned zero temperature sensor handles. WTG reports this as `not_available` and keeps temperature in the unavailable summary. It does not synthesize a temperature value.

Unavailable provider-scoped facts on the current bench:

```text
name
temperature
```

## Available Runtime Shape

On the validated bench, compact human output remains provider-scoped and factual:

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
  Unavailable: name, temperature
```

Probe output includes both the base Level Zero facts and Sysman domain facts:

```text
device.sysman.engine_groups.count: ok (raw=3)
device.sysman.memory_modules.count: ok (raw=1)
device.sysman.power_domains.count: ok (raw=1)
device.sysman.temperature_sensors.count: ok (raw=0)
device.sysman.temperature_sensors.status: not_available (raw=null) [zesDeviceEnumTemperatureSensors returned zero handles.]
device.sysman.frequency_domains.count: ok (raw=1)
```

Stats and watch-stats output include the same provider-scoped Sysman facts under the Intel device object with schema `wtg.intel_level_zero.stats.v3`.

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
wtg.version: 0.2.9
provider.authority: Intel Level Zero
provider.source: wtg.provider.intel.level_zero
telemetry.class: provider_telemetry
provider.status: unavailable
reason: Intel Level Zero runtime DLL ze_loader.dll was not found.
```

Stats output remains parseable JSON even when unavailable. It should still include:

```text
schema: wtg.intel_level_zero.stats.v3
provider: intel
provider_authority: Intel Level Zero
provider_source: wtg.provider.intel.level_zero
telemetry_class: provider_telemetry
```

The unavailable reason should be preserved in the relevant provenance wrapper instead of being collapsed into a fake device or fake zero telemetry.

## Validation Baseline

Local validation reported for the 0.2.9 Intel Sysman provider:

```text
cargo fmt
cargo test
cargo build -p wtg-app --bin wtg --release
git grep -n "wtg-provider-probe\|src/bin/wtg-provider-probe\|--bin wtg-provider-probe"
```

Bench runtime checks reported:

```powershell
.\wtg.exe --version
.\wtg.exe --once
.\wtg.exe --once --provider intel
.\wtg.exe --probe --provider intel
.\wtg.exe --once --stats --provider intel
.\wtg.exe --watch --stats --provider intel --interval 250
```

Bench validation result:

```text
version: WTG - WhatTheGPU v0.2.9
NVIDIA default --once: ok
Intel --once: ok
Intel --probe: ok, includes Sysman domain facts
Intel --once --stats: ok, valid JSON, schema v3
Intel --watch --stats: ok, repeated valid JSON objects
state=error facts observed: 0
```

Expected rejections:

```powershell
.\target\release\wtg.exe --probe-fields --provider intel --field-id 1
.\target\release\wtg.exe --once --provider intel --sink jsonl
.\target\release\wtg.exe --watch --provider intel --sink csv
.\target\release\wtg.exe --watch --provider intel --sink mqtt
.\target\release\wtg.exe --once --probe --provider intel
```

## Non-Goals

This provider cycle intentionally does not add:

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
- standalone provider probe binaries

## Candidate Future Expansions

Promising next areas, still provider-scoped if added:

- decode selected Sysman buffers into typed Intel-native facts only after structure layouts are confirmed
- record driver/runtime version if exposed by the provider surface
- validate on additional Intel hardware and driver/runtime combinations
- compare behavior across integrated Intel graphics and discrete Intel Arc hardware
- decide whether selected Sysman facts should remain raw-only or gain typed provider-native wrappers


## 0.2.9 Closure

This document records the closed 0.2.9 Intel Level Zero/Sysman discovery milestone.

The 0.2.9 checkpoint is:

```text
checkpoint/0.2.9-intel-sysman-m2
```

0.2.9 proves that WTG can safely load Intel Level Zero/Sysman, enumerate Intel devices and Sysman domains, and emit provider-scoped raw facts through normal WTG CLI paths.

0.2.9 does not claim Intel usable telemetry parity with NVML. Usable AMD/Intel provider-native telemetry moves to the 0.3.x provider discovery-to-usable-telemetry cycle.

## Documentation Rule

Operational human CLI output should stay compact and factual:

- provider/source/class
- provider status
- driver/device counts
- compact provider-native identity facts
- compact unavailable/error summaries

Structured JSON stats output is allowed to be detailed because it is explicitly requested with `--stats`. It should preserve provider provenance and unavailable/error states without turning Intel Level Zero or Sysman into an NVML compatibility layer.

## Pruning Rule

If a candidate Intel API needs risky control flow, write behavior, session/shared-memory management, or unclear structure definitions, keep the note here and do not surface it in normal CLI output until it is safely understood.
