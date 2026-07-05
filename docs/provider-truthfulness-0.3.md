# WTG 0.3.0 Provider Truthfulness Notes

This document records the provider-correctness checkpoint reached during the `dev/0.3.0-egui-provider-adapters` cycle.

## Scope

WTG 0.3.0 keeps NVIDIA/NVML as the primary/default truth path. AMD ADL and Intel Level Zero/Sysman remain provider-scoped supporting witnesses. Their facts must preserve source, state, unit, and unavailable/error information without being translated into NVML-equivalent claims.

## Landed commits

```text
b9bfb46 fix: mark Intel counter resets unavailable
959431e fix: render missing AMD active state unavailable
e96acfe test: lock Intel idle delta edge cases
406ce77 docs: record 0.3.0 provider truthfulness checkpoint
```

## Intel Level Zero

Intel Sysman power and engine utilization are delta-derived facts.

Current rule:

- timestamp must advance before a delta-derived value is valid
- backward energy counters are reported as `not_available` with a counter-reset reason
- backward engine active-time counters are reported as `not_available` with a counter-reset reason
- the same counter value with an advancing timestamp remains valid idle and resolves to `ok` `0.0`

The test matrix now locks the idle-vs-reset distinction:

```text
same timestamp or backward timestamp
  -> not_available

same counter value + advancing timestamp
  -> ok 0.0

backward counter value + advancing timestamp
  -> not_available
```

Future long-lived Intel provider-session work must preserve and rerun these tests. The current implementation keeps delta baselines in module-level statics while `collect_once` remains one-shot shaped, so changing the lifecycle boundary changes what a provider counter reset means in practice.

## AMD ADL

The current AMD ADL path is snapshot-style, not Intel-style delta-derived. The Intel counter-reset rule does not directly apply to the current AMD path because AMD is not deriving power or engine utilization from monotonic counters here.

The AMD follow-up fixed one human-output fake-value case:

- missing `adapter_active` no longer renders as `no`
- missing `adapter_active` renders as `unavailable`
- `Some(false)` still renders as `no`
- `Some(true)` still renders as `yes`
- provider data shape was not changed

## eGUI impact

No eGUI code change was required for the Intel or AMD truthfulness fixes.

Expected behavior:

- Intel counter resets render as unavailable rather than fake zero values
- AMD missing active state renders as unavailable rather than fake `no`
- NVIDIA/NVML reporting remains visually and semantically unchanged
- AMD and Intel remain provider-scoped supporting witnesses

## Deferred work

Provider lifecycle cleanup remains separate:

- long-lived Intel Level Zero provider session for watch/eGUI
- long-lived AMD ADL provider session for watch/eGUI
- avoid repeated load/init/enumerate/free churn in `collect_once` loops
- preserve Intel idle-vs-reset tests under the new lifecycle

MQTT/Home Assistant redline cleanup remains a separate delivery-lifecycle bucket and should not be mixed with provider math or eGUI adapter-list work.
