# WTG 0.3.x Provider Discovery Intent

WTG 0.3.x moves AMD ADL and Intel Level Zero/Sysman from discovery-grade provider evidence toward usable provider-native telemetry.

## Goal

Convert AMD/Intel provider discovery into usable provider-native telemetry without:

- NVML parity claims
- fake zero values
- cross-vendor semantic flattening
- changes to NVIDIA/NVML primary truth behavior

## Provider Truth Model

NVIDIA/NVML remains the primary WTG truth path.

AMD ADL and Intel Level Zero/Sysman remain supporting provider witnesses. They should expose what their provider surfaces actually report, with provenance and unavailable/error states preserved.

## 0.3.x Working Layers

```text
provider_raw:
  raw facts, handles, buffers, exact source_api, unavailable/error states

provider_typed:
  decoded provider-native facts such as Intel engine activity, memory state,
  power energy counters, frequency state, AMD adapter activity/memory/power/temp

provider_usable:
  small nullable surface for eGUI/redline:
    utilization_pct
    memory_used_bytes
    memory_total_bytes
    power_watts
    temperature_c
    core_clock_mhz
    quality/status
```

## Intel Focus

Intel Level Zero/Sysman is the first 0.3.x target because 0.2.9 validated a live Sysman runtime on the bench.

Initial Intel work should focus on:

- typed decoding of safe Sysman structures
- delta-derived engine utilization from activity counters
- delta-derived watts from energy counters
- typed memory state where safely exposed
- typed frequency state where safely exposed
- temperature only when Sysman exposes sensor handles

First-sample delta facts should report `not_available` with a clear reason such as `requires previous sample`.

## AMD Follow-On

AMD ADL should follow the same pattern after Intel:

- preserve raw ADL facts
- add typed ADL-native facts beside raw facts
- expose usable nullable telemetry only where ADL provides sufficient evidence
- do not synthesize NVML-equivalent fields

## eGUI / Redline Gate

eGUI and redline expansion should consume `provider_usable`, not raw discovery blobs.

A provider fact is eGUI/redline-ready only when it has:

- stable provider-scoped meaning
- source API provenance
- unit
- state
- nullability for unavailable/error
- no fake zeros
- no cross-provider semantic relabeling

## Non-Goals

0.3.x provider discovery does not add:

- MQTT/Home Assistant publishing for AMD or Intel
- cross-vendor normalized telemetry claims
- NVML-equivalent field names for AMD or Intel
- Task Manager / PerfMon / PDH interpretation
- UI redline behavior before provider_usable is stable
