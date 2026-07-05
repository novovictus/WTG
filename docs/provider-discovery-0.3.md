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

provider_raw: raw facts, handles, buffers, exact source_api, unavailable/error states

provider_typed: decoded provider-native facts such as Intel engine activity, memory state, power energy counters, frequency state, AMD adapter activity/memory/power/temp

provider_usable: small nullable surface for eGUI/redline that may include utilization_pct, memory_used_bytes, memory_total_bytes, power_watts, temperature_c, core_clock_mhz, and quality/status.

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

### Intel truthfulness checkpoint

The 0.3.0 Intel provider distinguishes idle from reset for delta-derived Sysman facts:

- power and engine delta facts require an advancing timestamp
- backward energy counters are reported as `not_available` with a counter-reset reason
- backward engine active-time counters are reported as `not_available` with a counter-reset reason
- the same counter value with an advancing timestamp remains valid idle and resolves to `ok` `0.0`
- tests lock both reset and idle edge cases so future session work does not collapse them into the same state

The current Intel delta baselines live in module-level statics while `collect_once` is still one-shot shaped. A future long-lived Intel provider session for watch/eGUI must preserve and rerun the idle-vs-reset delta tests because the lifecycle boundary changes what a counter reset means in practice.

## AMD Follow-On

AMD ADL should follow the same pattern after Intel:

- preserve raw ADL facts
- add typed ADL-native facts beside raw facts
- expose usable nullable telemetry only where ADL provides sufficient evidence
- do not synthesize NVML-equivalent fields

### AMD truthfulness checkpoint

The current AMD ADL path is snapshot-style, not Intel-style delta-derived. The Intel counter-reset fix does not directly apply to the present AMD path because AMD is not deriving power/engine deltas from monotonic provider counters here.

The 0.3.0 AMD follow-up fixed one human-output fake-value case:

- missing `adapter_active` no longer renders as `no`
- missing `adapter_active` renders as `unavailable`
- `Some(false)` still renders as `no`
- `Some(true)` still renders as `yes`
- the AMD provider data shape remains unchanged

A future long-lived AMD ADL provider session for watch/eGUI remains deferred. That work should avoid repeated ADL load/init/free churn without changing provider-native meaning or introducing fake values.

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

The 0.3.0 eGUI provider-backed adapter view consumes provider-scoped AMD and Intel facts without changing the NVIDIA/NVML path. No eGUI code change was required for the Intel counter-reset or AMD missing-active-state fixes.

## Deferred Work

Provider lifecycle work remains separate from provider truthfulness fixes:

- long-lived Intel Level Zero provider session for watch/eGUI
- long-lived AMD ADL provider session for watch/eGUI
- preservation/revalidation of Intel idle-vs-reset delta tests under the new lifecycle

MQTT/Home Assistant redline work remains a separate delivery-lifecycle bucket.

## Non-Goals

0.3.x provider discovery does not add:

- MQTT/Home Assistant publishing for AMD or Intel
- cross-vendor normalized telemetry claims
- NVML-equivalent field names for AMD or Intel
- Task Manager / PerfMon / PDH interpretation
- UI redline behavior before provider_usable is stable
