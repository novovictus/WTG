# AMD ADL Discovery Sandbox

This branch keeps AMD ADL facts provider-scoped. `wtg.exe --provider amd` reports ADL-native facts and does not translate them into NVML field names or invent missing values.

## Current CLI Contract

- `wtg.exe --provider amd --once`
- `wtg.exe --provider amd --watch --interval <ms>`
- `wtg.exe --provider amd --stats` is intentionally rejected

The CLI output is intentionally compact. Detailed ADL notes live here instead of in normal `wtg.exe` output.

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

These are the surfaces this sandbox actively binds today. They are the only ones included in the compact CLI call summary.

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

## Pruning Rule

If a candidate API needs risky control flow, write behavior, session/shared-memory management, or unclear structure definitions, keep the note here and do not surface it in normal CLI output until it is safely understood.
