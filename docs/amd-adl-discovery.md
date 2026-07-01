# AMD ADL Discovery Sandbox

This branch keeps AMD ADL facts provider-scoped. `wtg.exe --provider amd` reports ADL-native facts and does not translate them into NVML field names, infer missing values, or claim cross-vendor parity.

The purpose of this branch is discovery, not parity. NVIDIA/NVML remains WTG's primary truth provider. AMD ADL is a supporting provider witness selected explicitly through `--provider amd`.

## Current CLI Contract

Supported AMD ADL commands:

```powershell
.\target\release\wtg.exe --provider amd --once
.\target\release\wtg.exe --provider amd --watch --interval 1000
```

Intentional rejections and omissions:

```text
AMD --stats: rejected
AMD sinks: rejected
AMD MQTT/Home Assistant publishing: not implemented
AMD output schema: no stable schema field
AMD telemetry class: provider_telemetry
```

The CLI output is intentionally compact. Detailed ADL notes live here instead of in normal `wtg.exe` output.

## Current Branch Baseline

Current branch: `dev/0.2.8-amd-adl-discovery`

Current version: `0.2.8`

Observed on the ASUS ROG G533QS hybrid laptop with AMD Radeon(TM) Graphics and NVIDIA GeForce RTX 3080 Laptop GPU:

```text
ADL adapter records returned: 7
Physical adapter groups: 2
AMD physical adapters: 1
Non-AMD physical adapters seen through ADL: 1
Extended AMD discovery ran: 1
```

Current compact ADL discovery summary:

```text
telemetry exports matched: 10
optional calls attempted: 11
ok: 5
unsupported: 1
not_available: 1
error: 4
```

Current physical adapter grouping:

```text
vendor=1002,bus=6,device=0,function=0
  AMD Radeon(TM) Graphics
  logical ADL record indexes: 0, 1, 2
  extended AMD discovery attempted: yes

vendor=10,bus=1,device=0,function=0
  NVIDIA GeForce RTX 3080 Laptop GPU
  logical ADL record indexes: 3, 4, 5, 6
  extended AMD discovery attempted: no
```

ADL can surface non-AMD adapter identity/topology records. This branch preserves those records as ADL observations but does not treat them as NVIDIA telemetry and does not route them through the NVIDIA/NVML truth path.

## Current Observed AMD Values

Short watch and snapshot runs on the current test bed have shown:

```text
activity: low single digits
engine clock: observed at both 400.0 MHz and 2100.0 MHz
memory clock: 1600.0 MHz
observed core clock: 21.0 MHz
observed memory clock: 16.0 MHz
bus: 2500 x16 / max x16
```

Currently unavailable or failing through the bound calls on this platform:

```text
temperature
fan info
fan speed
legacy memory info
some Overdrive capability surfaces
```

The observed `engine clock` value can jump between idle-ish and max-ish values even while activity remains low. That should be treated as provider-reported ADL behavior, not as proof of sustained workload.

## Test Context

The current baseline was captured while the laptop was on reduced USB-C power rather than the full OEM AC adapter.

Power source should be treated as a test variable before drawing conclusions about sustained clocks, throttling, provider completeness, or hybrid GPU residency. A full OEM AC validation cycle is intentionally deferred.

## Observed `atiadlxx.dll` Surface On This Branch

Required core ADL entry points currently used:

- `ADL_Main_Control_Create`
- `ADL_Main_Control_Destroy`
- `ADL_Adapter_NumberOfAdapters_Get`
- `ADL_Adapter_AdapterInfo_Get`

Optional low-risk telemetry entry points currently matched and queried when exported:

- `ADL_Adapter_Active_Get`
- `ADL_Overdrive_Caps`
- `ADL_Overdrive5_CurrentActivity_Get`
- `ADL_Adapter_ObservedClockInfo_Get`
- `ADL_Overdrive5_Temperature_Get`
- `ADL_Overdrive5_ODParameters_Get`
- `ADL_Overdrive5_FanSpeedInfo_Get`
- `ADL_Overdrive5_FanSpeed_Get`
- `ADL_Adapter_MemoryInfo_Get`
- `ADL_Adapter_VideoBiosInfo_Get`
- `ADL_Adapter_ASICFamilyType_Get`

These are the actively bound discovery surfaces today. The compact CLI summary reports current matched/attempted/result counts rather than printing long symbol lists.

## APIs Currently Queried

For the primary logical record of each physical AMD adapter group, the provider currently attempts:

- adapter active state
- Overdrive caps
- Overdrive5 current activity
- observed core and memory clocks
- Overdrive5 temperature
- Overdrive5 OD parameters
- Overdrive5 fan info
- Overdrive5 fan speed in percent and RPM
- legacy memory info
- VBIOS strings
- ASIC family / valids

For duplicate AMD logical records, extended discovery is skipped and the provider emits a provider-scoped dedup record instead.

For non-AMD adapter records surfaced by ADL, the provider preserves the record and queries only `ADL_Adapter_Active_Get`.

## Intentionally Skipped For Now

The following candidates are not bound in the normal CLI path on this branch:

- `ADL_Overdrive5_ThermalDevices_Enum`
- `ADL_Overdrive5_PowerControl_Caps`
- `ADL_Overdrive5_PowerControl_Get`
- `ADL_Overdrive5_PowerControlInfo_Get`
- `ADL_Adapter_MemoryInfo2_Get`
- `ADL_Adapter_MemoryInfo3_Get`
- `ADL_Adapter_ObservedGameClockInfo_Get`
- `ADL2_Adapter_VRAMUsage_Get`
- `ADL2_Adapter_DedicatedVRAMUsage_Get`
- `ADL_Overdrive6_Capabilities_Get`
- `ADL_Overdrive6_CurrentStatus_Get`
- `ADL_Overdrive6_Temperature_Get`
- `ADL_Overdrive6_FanSpeed_Get`
- `ADL_Overdrive6_PowerControl_Get`
- `ADL_Overdrive6_VoltageControl_Get`

## Candidate Future Expansions

Promising next areas, still provider-scoped if added:

- PMLog
- Overdrive8 PMLog shared memory
- OverdriveN
- OD6 power / temp / voltage
- VRAM usage
- `MemoryInfo2` / `MemoryInfo3`
- PowerXpress
- SmartAccessMemory
- provider watch min/max tracking for short-run spike visibility

## Why These Are Skipped

This sandbox avoids:

- `Set` APIs
- `Reset` APIs
- `Start` / `Stop` APIs
- control APIs
- shared-memory PMLog APIs
- APIs with uncertain struct layout
- APIs that require ADL2 context/session handling not already established here

The goal is to keep `wtg.exe --provider amd` read-only, low-risk, and operational on the test bed.

## Documentation Rule

Operational CLI output should stay compact and factual:

- topology counts
- physical adapter grouping
- provider/source/class
- compact export and call result counts
- successful low-risk facts
- compact unavailable/error summaries

Long API rationale, skipped surfaces, future candidates, and test-context caveats belong in this document, not in normal terminal output.

## Pruning Rule

If a candidate API needs risky control flow, write behavior, session/shared-memory management, or unclear structure definitions, keep the note here and do not surface it in normal CLI output until it is safely understood.
