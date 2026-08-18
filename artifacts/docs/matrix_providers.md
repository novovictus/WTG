# WTG Provider Matrix (v0.3.0 Smoke Validation)

WTG version tested baseline: v0.3.0 provider smoke harness

This matrix records **empirical provider behavior** observed during v0.3.0 provider-harness validation. It documents observed behavior only and does not imply official support, vendor parity, or compatibility guarantees.

NVIDIA NVML remains the primary WTG truth path. AMD ADL and Intel Level Zero are supplemental provider-scoped witnesses. A provider-unavailable result is recorded as a valid observed state when hardware or runtime is absent and WTG returns an explicit unavailable report.

Raw run logs are retained as smoke artifacts and may contain host-specific identifiers.

---

## Tested Provider / Host Combinations

| Host | System | Provider | Hardware Observed by CIM | Driver / Runtime | Provider State | Expected Exit | Smoke Result | Notes |
| ---- | ------ | -------- | ------------------------ | ---------------- | -------------- | ------------- | ------------ | ----- |
| INSPIRON3861 | Dell Inc. Inspiron 3891 | NVIDIA NVML | NVIDIA GeForce RTX 3060 Ti | Windows 32.0.15.9595 / NVIDIA 595.95 | Available | 0 | PASS | Positive NVIDIA baseline; WTG reports NVML device 0, UUID, temperature, utilization, VRAM, and power. |
| INSPIRON3861 | Dell Inc. Inspiron 3891 | AMD ADL | AMD Radeon (TM) R9 M360 | Windows 27.20.20913.2000 | Available | 0 | PASS | ADL reports one AMD physical adapter; NVIDIA and Intel adapters are visible through ADL as topology-only witnesses. Overdrive caps and memory info unavailable. |
| INSPIRON3861 | Dell Inc. Inspiron 3891 | Intel Level Zero | Intel(R) UHD Graphics 730 | Windows 32.0.101.7085 | Available | 0 | PASS | Level Zero reports Intel device 0, UUID, core clock, memory, power, engine activity, and frequency. Temperature unavailable: zero handles. |
| LAPTOP-8CC8RC3A | ASUSTeK COMPUTER INC. ROG Strix G533QS_G533QS | NVIDIA NVML | NVIDIA GeForce RTX 3080 Laptop GPU | Windows 32.0.15.9636 / NVIDIA 596.36 | Available | 0 | PASS | WTG reports NVML device 0, UUID, temperature, utilization, VRAM, and power. Known idle memory-controller anomaly persists: 0% GPU / 100% memory. |
| LAPTOP-8CC8RC3A | ASUSTeK COMPUTER INC. ROG Strix G533QS_G533QS | AMD ADL | AMD Radeon(TM) Graphics | Windows 30.0.13002.19003 | Available | 0 | PASS | ADL reports one AMD physical adapter; NVIDIA is visible through ADL as topology-only. Overdrive caps, temperature, fan, and memory info unavailable. |
| LAPTOP-8CC8RC3A | ASUSTeK COMPUTER INC. ROG Strix G533QS_G533QS | Intel Level Zero | N/A | ze_loader.dll not found | Unavailable | 2 | PASS | No Intel GPU reported by CIM; WTG returns explicit provider-unavailable report with reason. |
| DAD-SURFACE | Microsoft Corporation Surface Pro 7 | NVIDIA NVML | N/A | NVML LoadLibrary failure | Unavailable | 2 | PASS | No NVIDIA GPU reported by CIM; WTG returns explicit provider-unavailable report with reason. |
| DAD-SURFACE | Microsoft Corporation Surface Pro 7 | AMD ADL | N/A | ADL DLL not found | Unavailable | 2 | PASS | No AMD GPU reported by CIM; WTG returns explicit provider-unavailable report with reason. |
| DAD-SURFACE | Microsoft Corporation Surface Pro 7 | Intel Level Zero | Intel(R) Iris(R) Plus Graphics | Windows 31.0.101.2130 | Available, Sysman unavailable | 0 | PASS | Level Zero reports Intel device 0 and UUID. Memory, power, engine activity, frequency, and temperature are unavailable because Sysman is unavailable. |

---

## Provider Exit Semantics Validated

| Provider State | Meaning | Expected Exit | Harness Treatment |
| -------------- | ------- | ------------- | ----------------- |
| Available / ok | Provider completed and emitted a valid report for the observed hardware/runtime state. | 0 | PASS when required evidence tokens are present. |
| Unavailable | Provider runtime or matching hardware is absent, and WTG emits `Provider status: unavailable` plus `Reason:`. | 2 | PASS when absent hardware/runtime is expected from CIM/runtime evidence. |
| Error | Provider or device operation failed after provider selection. | 3 | FAIL unless intentionally being tested as an error case. |
| CLI/config error | Invalid user input or configuration. | 1 | FAIL for smoke harness provider validation. |
| Internal/unrecognized status | Unknown status outside the provider contract. | 5 | FAIL. |

---

## Notes

* Host names are recorded exactly as reported by the smoke artifacts: INSPIRON3861, LAPTOP-8CC8RC3A, and DAD-SURFACE.
* This matrix is provider-scoped. It does not replace the NVIDIA-focused WTG test matrix.
* NVIDIA NVML is the primary WTG truth path; supplemental providers are not normalized into NVML-equivalent claims.
* AMD ADL can expose non-AMD adapters as topology-only records. These records are useful as provider-scoped context, not as AMD telemetry for those devices.
* Intel Level Zero telemetry depends on both Level Zero runtime availability and Sysman support. A visible Intel device with Sysman unavailable is a valid partial-provider observation.
* Provider-unavailable is not automatically a failure. It is a valid smoke result when the harness confirms absent hardware/runtime and WTG returns a clear unavailable status and reason.
* The LAPTOP-8CC8RC3A RTX 3080 Laptop GPU still exhibits the known NVML memory-controller anomaly at idle under the tested driver branch.

---

## Observed Behavioral Deltas

* INSPIRON3861 provides the strongest positive multi-provider validation point: NVIDIA NVML, AMD ADL, and Intel Level Zero all return valid provider reports on the same host.
* LAPTOP-8CC8RC3A validates mixed NVIDIA/AMD hardware with no Intel GPU present. Intel Level Zero returns unavailable because the runtime DLL is absent, and the harness treats that explicit unavailable report as a valid absent-provider state.
* DAD-SURFACE validates absent NVIDIA and AMD provider paths while also validating a partial Intel Level Zero path where device identity is available but Sysman-backed metrics are unavailable.
* v0.3.0 provider smoke validation confirms that unavailable provider states can be represented explicitly without hanging, silently disappearing, or being collapsed into fake zero telemetry.
