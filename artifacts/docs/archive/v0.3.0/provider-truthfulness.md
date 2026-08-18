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

- Timestamp must advance before a delta-derived value is valid.
- Backward energy or engine counters are `not_available` counter-reset evidence.
- Equal counters with an advancing timestamp are valid idle and resolve to `ok 0.0`.

Future long-lived Intel provider-session work must preserve these tests because changing the lifecycle boundary changes what a counter reset means.

## AMD ADL

The AMD path is snapshot-style rather than Intel-style delta-derived. Missing `adapter_active` renders as unavailable; `Some(false)` remains `no` and `Some(true)` remains `yes`.

## Deferred work

- Long-lived Intel and AMD provider sessions.
- Avoid repeated load/init/enumerate/free churn.
- Preserve Intel idle-versus-reset tests under the new lifecycle.
- Keep MQTT/Home Assistant and Redline cleanup separate from provider math and adapter-list work.
