# AMD ADL Discovery Sandbox

This document records the completed 0.2.8 AMD ADL provider baseline. AMD ADL facts remain provider-scoped. `wtg.exe --provider amd` reports ADL-native facts and does not translate them into NVML field names, infer missing values, or claim cross-vendor parity.

The purpose of the AMD ADL cycle was discovery, not parity. NVIDIA/NVML remains WTG's primary truth provider. AMD ADL is a supporting provider witness selected explicitly through `--provider amd`.

## Current CLI Contract

Supported AMD ADL commands:

```powershell
.\target\release\wtg.exe --once --provider amd
.\target\release\wtg.exe --watch --provider amd --interval 1000
.\target\release\wtg.exe --once --stats --provider amd
.\target\release\wtg.exe --watch --stats --provider amd --interval 1000
.\target\release\wtg.exe --probe --provider amd
```

Current output roles:

```text
AMD --once:
  compact human ADL snapshot

AMD --watch:
  compact repeated human ADL snapshot

AMD --stats:
  ADL-native JSON stats/provenance
  schema: wtg.amd_adl.stats.v1

AMD --watch --stats:
  one ADL-native JSON stats object per tick

AMD --probe:
  compact [probe] key:value output
```

Intentional rejections and omissions:

```text
AMD --probe-fields: rejected; NVML field IDs are provider-specific
AMD sinks: rejected
AMD MQTT/Home Assistant publishing: not implemented
AMD JSONL/CSV sink schema: not committed
AMD telemetry class: provider_telemetry
```

The human CLI output remains intentionally compact. Detailed ADL notes live here instead of in normal human `wtg.exe` output.

## Mode Alignment

AMD ADL now follows the same WTG mode split as the NVIDIA/NVML path:

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

The ADL stats schema is provider-native:

```text
schema: wtg.amd_adl.stats.v1
provider: amd
provider_authority: AMD ADL
provider_source: wtg.provider.amd.adl
telemetry_class: provider_telemetry
```

This schema is not an NVML compatibility schema. It preserves ADL-native adapter/group facts, source APIs, states, units, and unavailable/error results without inventing missing NVML-equivalent fields.

## 0.2.8 Branch Baseline

Baseline branch: `dev/0.2.8-amd-adl-discovery`

Historical spike branch: `spike/0.2.8-amd-adl-provider-modes`

Baseline version: `0.2.8`

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

## Current Human Watch Shape

Representative AMD ADL watch summary:

```text
ADL records: 7 | physical adapter groups: 2 | AMD: 1 | non-AMD: 1 | extended AMD probes: 1
```

Representative AMD adapter watch fields:

```text
active: yes
activity: 99%
engine clock: 2100.0 MHz
memory clock: 1600.0 MHz
perf level raw: -1
bus: 2500 x16 / max x16
observed core clock: 21.0 MHz
vbios: 017.010.000.028 | 113-CEZANNE-017 | 2020/11/13 02:11
asic family: 34 valid=175
unavailable: overdrive caps, temp, fan, memory info
```

Representative non-AMD ADL topology-only watch block:

```text
NVIDIA GeForce RTX 3080 Laptop GPU [vendor=10,bus=1,device=0,function=0]
  seen by ADL only; AMD extended discovery skipped
  ADL logical records: 3, 4, 5, 6
```

The watch path says `physical adapter groups`, not `physical GPUs`, because ADL returns logical adapter records and WTG groups those records into physical adapter groups.

## Current Observed AMD Values

Short watch and snapshot runs on the current test bed have shown:

```text
activity: low single digits at rest, near 99% under sustained load
engine clock: observed at 400.0 MHz idle-ish and 2100.0 MHz loaded
memory clock: 1600.0 MHz on USB-C / OEM AC, 400.0 MHz on battery
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

These observations are platform-specific to the current ASUS G533QS test bed. They should not be generalized as AMD-wide behavior without additional hardware.

## Test Context

The current baseline was captured on the ASUS ROG G533QS using the explicit AMD ADL provider path in the normal `wtg.exe` release build.

Power source is a test variable. On this platform, ADL-reported AMD iGPU memory clock tracks power source more strongly than workload, while ADL-reported activity and engine clock track workload more strongly than power source.

The known-truth reproduction harness is committed at:

```text
artifacts/dev/amd_adl_baseline_expanded.ps1
```

## Known Truth Baseline: Expanded Power/Load Cycle

Known-truth artifact name:

```text
Power-State-Change_and-Full-Load-Cycle-amd_adl_baseline_expanded_LAPTOP-8CC8RC3A_20260701_233412.zip
```

The expanded baseline used six queued phases, each with a 5 second countdown and a 60 second hold window:

```text
1. USB-C REST - no load
2. USB-C LOAD - start load
3. USB-C LOAD CONFIRM - keep USB-C/load steady
4. BATTERY LOAD - remove USB-C
5. BARREL AC LOAD - connect OEM AC
6. BARREL AC COOLDOWN - stop load, stay on OEM AC
```

Harness status:

```text
watch window closed: yes
watch exit observed: true
stderr: clean / header only
samples captured: 401
sample_seq: 0 through 400
```

Phase markers from the known-truth run:

```text
USB-C rest/no-load:       2026-07-01T23:34:31.9317221-04:00
USB-C load start:         2026-07-01T23:35:37.8679534-04:00
USB-C load steady:        2026-07-01T23:36:43.6550149-04:00
Battery load:             2026-07-01T23:37:49.4483542-04:00
Barrel AC load:           2026-07-01T23:38:57.7343620-04:00
Barrel AC cooldown:       2026-07-01T23:40:05.3452108-04:00
End requested:            2026-07-01T23:41:06.1552205-04:00
Watch closed:             2026-07-01T23:41:07.3351664-04:00
```

Per-phase ADL summary from the known-truth run:

```text
USB-C REST - NO LOAD:
  samples: 65, seq 19-83
  activity: min 0%, max 8%, avg 0.45%
  engine clock values: 400.0, 2087.0, 2100.0 MHz
  memory clock values: 1600.0 MHz

USB-C LOAD - START LOAD:
  samples: 65, seq 84-148
  activity: min 0%, max 99%, avg 96.05%
  engine clock values: 400.0, 2100.0 MHz
  memory clock values: 1600.0 MHz

USB-C LOAD CONFIRM - STEADY STATE:
  samples: 64, seq 149-212
  activity: min 98%, max 99%, avg 98.97%
  engine clock values: 2100.0 MHz
  memory clock values: 1600.0 MHz

BATTERY LOAD - REMOVE USB-C:
  samples: 64, seq 213-276
  activity: min 93%, max 99%, avg 98.02%
  engine clock values: 400.0, 2100.0 MHz
  memory clock values: 400.0, 1600.0 MHz

BARREL AC LOAD - CONNECT OEM AC:
  samples: 64, seq 277-340
  activity: min 94%, max 99%, avg 98.78%
  engine clock values: 400.0, 1912.0, 2100.0 MHz
  memory clock values: 400.0, 1600.0 MHz

BARREL AC COOLDOWN - STOP LOAD:
  samples: 60, seq 341-400
  activity: min 0%, max 99%, avg 6.62%
  engine clock values: 400.0, 2100.0 MHz
  memory clock values: 1600.0 MHz
```

Known-truth interpretation:

```text
USB-C / AC-like state:
  memory clock: 1600 MHz

Battery state:
  memory clock: 400 MHz after transition settles

OEM barrel AC restored:
  memory clock: 1600 MHz after transition settles

No load:
  engine clock mostly 400 MHz

Load:
  activity near 99%
  engine clock mostly 2100 MHz

Cooldown on OEM barrel AC:
  activity returns to idle
  engine clock returns mostly to 400 MHz
  memory clock remains 1600 MHz
```

Conclusion for this platform:

```text
ADL activity and engine clock reflect workload.
ADL memory clock reflects power source.
Power-source transitions can briefly delay or gap ADL samples.
```

This is a platform-specific finding from the ASUS G533QS test bed. It should not be generalized as an AMD-wide behavior without additional hardware.

## Known Truth Harness Behavior

The committed harness is the known-good reproduction example for this branch. It intentionally:

- builds `wtg-app` release first
- opens a visible `cmd.exe` watch window
- runs `wtg.exe --provider amd --watch --interval 1000`
- writes UTF-8 metadata, marker, watch, and stderr logs
- uses `cmd.exe /c`, not `/k`
- closes the watch process tree with `taskkill /PID <pid> /T /F`
- queues phase transitions with a 5 second countdown
- holds each phase for 60 seconds
- writes phase markers with ISO-8601 timestamps

Expected output files:

```text
amd_adl_watch.txt
amd_adl_watch.err.txt
phase_markers.txt
run_metadata.txt
run_adl_watch.cmd
```

Do not replace this harness with a background-only process for the known-truth run. The visible watch window is useful during hardware power-state changes.

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

These are the actively bound discovery surfaces today. The compact human CLI summary reports current matched/attempted/result counts rather than printing long symbol lists.

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
- sample gap tracking around power-source transitions

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

Operational human CLI output should stay compact and factual:

- topology counts
- physical adapter grouping
- provider/source/class
- compact export and call result counts
- successful low-risk facts
- compact unavailable/error summaries

Long API rationale, skipped surfaces, future candidates, and test-context caveats belong in this document, not in normal human terminal output.

Structured JSON stats output is allowed to be detailed because it is explicitly requested with `--stats`. It should preserve provider provenance and unavailable/error states without turning ADL into an NVML compatibility layer.

## Pruning Rule

If a candidate API needs risky control flow, write behavior, session/shared-memory management, or unclear structure definitions, keep the note here and do not surface it in normal CLI output until it is safely understood.
