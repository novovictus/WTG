# WTG 0.3.0 Provider Harness Notes



The supplemental provider harness is separate from the core NVIDIA/NVML matrix.



The core WTG matrix remains focused on valid NVIDIA targets. The supplemental provider harness records AMD ADL and Intel Level Zero behavior as provider-scoped evidence. These outputs may include topology-only records, unavailable fields, runtime availability states, or provider-specific fields that do not map cleanly to NVIDIA/NVML telemetry.



Current harness expectations:



\- `wtg\_test.ps1` validates the default NVIDIA/NVML `--once` path.

\- `wtg\_providers\_test.ps1` validates explicit provider paths:

&#x20; - `wtg.exe --provider amd --once`

&#x20; - `wtg.exe --provider intel --once`



Known source-level follow-up:



\- Provider unavailable states should use a consistent status-to-exit-code contract.

\- AMD ADL and Intel Level Zero currently report unavailable runtime states differently at the process exit-code layer.

\- The default NVIDIA/NVML path should return a structured unavailable report on no-NVIDIA systems rather than hanging or producing incomplete harness output.

\- Supplemental provider token checks should be hardware-aware so absent AMD/Intel hardware does not require positive provider device telemetry.

\- Harness absent-hardware expectations are now explicit:

&#x20; - exit `2` plus `Provider status: unavailable` is a valid PASS when matching hardware/runtime is absent

&#x20; - exit `0` plus device telemetry remains required when matching hardware is present


