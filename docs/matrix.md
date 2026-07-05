# WTG Test Matrix (Early Validation)

WTG version tested baseline: v0.1.2+ (truth-layer hardened)

This matrix records **empirical test results** collected during early WTG development. It documents observed behavior only and does not imply official support or compatibility guarantees.

Raw run logs are not published to avoid exposing host-specific identifiers.

---

## Tested GPU / Driver Combinations

| GPU Model                 | Architecture | Driver Version(s)        | CUDA Version(s)    | OS        | NVML Load   | NVML mem util (Idle)                | --once   | --watch   | Notes                                                                                                    |
| ------------------------- | ------------ | ------------------------ | ------------------ | --------- | ----------- | ----------------------------------- | -------- | --------- | -------------------------------------------------------------------------------------------------------- |
| RTX 3080 Laptop GPU       | Ampere       | 566.07                   | 12.7               | Win11 x64 | Yes         | OK                                  | Yes      | Yes       | Stable                                                                                                   |
| RTX 3080 Laptop GPU       | Ampere       | 577.00                   | 12.9               | Win11 x64 | Yes         | OK                                  | Yes      | Yes       | Stable                                                                                                   |
| RTX 3080 Laptop GPU       | Ampere       | 580.88+ (conf. to 591.86)| 13.0+              | Win11 x64 | Yes         | 100% @ idle (first observed 580.88) | Yes      | Yes       | Driver-branch regression; normal thermals/power; WDDM consumer mobile SKU                                |
| RTX 4070 Laptop GPU       | Ada          | 591.59                   | 13.1               | Win11 x64 | Yes         | OK                                  | Yes      | Yes       | Stable; transient `nvidia-smi` formatting anomaly observed                                               |
| RTX 3060 Laptop GPU       | Ampere       | 512.74 / 580.88 / 591.86 | 11.6 / 13.0 / 13.1 | Win11 x64 | Yes         | OK                                  | Yes      | Yes       | 512: cap unavailable; 580: 105W cap exposed; 591: 80W cap (policy shift); no mem-util regression         |
| RTX 3060 Ti (Desktop OEM) | Ampere       | 580.88 / 581.04          | 13.0               | Win11 x64 | Yes         | OK                                  | Yes      | Yes       | Desktop reference; 200W cap exposed; idle + sustained load validated (≈199W / 200W); no mem-util anomaly |
| RTX A3000 12GB Laptop GPU | Ampere (Pro) | 580.92 / 591.59          | 13.0 / 13.1        | Win11 x64 | Yes         | OK                                  | Yes      | Yes       | Professional SKU; power draw and cap present; stable NVML across driver branches                         |
| GTX 1050 Ti               | Pascal       | 465.89                   | 11.3               | Win10 x64 | No          | N/A                                 | No       | N/A       | NVML DLL not loadable                                                                                    |
| GTX 1050 Ti               | Pascal       | 581.80                   | 13.0               | Win10 x64 | Yes         | OK                                  | Partial  | Partial   | NVML loads; power field unsupported (no cap exposure)                                                    |
| GT 1030                   | Pascal       | 581.04                   | 13.0               | Win11 x64 | Yes         | OK                                  | Yes      | Yes       | Low-power SKU; power draw N/A; expected NVML limitations                                                 |
| GTX 745                   | Maxwell      | 576.97                   | 12.9               | Win10 x64 | Yes         | OK                                  | Yes      | Yes       | Full NVML telemetry; baseline working legacy card; power field N/A                                       |
| GT 730                    | Kepler       | 475.14                   | 11.4               | Win10 x64 | Partial     | N/A                                 | No       | N/A       | NVML loads but utilization unsupported; nvmlDeviceGetUtilizationRates fails; power/temp invalid (0C/N/A) |
| 8400 GS                   | Tesla        | 342.xx (CIM 21.21)       | N/A                | Win10 x64 | No          | N/A                                 | No       | N/A       | NVML unavailable; nvidia-smi non-functional; WTG crash observed; below NVML support floor                |

---

## Notes

* NVML Mem Util (Idle) reflects nvmlDeviceGetUtilizationRates().memory observed at system idle.
* For regressions, the lowest driver version where the behavior is observed is recorded (e.g., 580.88+).
* 100% @ idle indicates a driver-reported saturation value with normal power, VRAM usage, and thermals; this does not imply actual memory bandwidth saturation.
* Behavior has been observed to change discretely across driver branches on certain mobile GeForce platforms.
* NVML availability on Windows is driver-version dependent.
* Presence of nvidia-smi does not guarantee NVML is loadable by third-party tools.
* Power telemetry is split: draw is common; caps are optional, even on RTX laptops.
* WTG fails fast and explicitly when NVML is unavailable; legacy GPUs may require additional guardrails to avoid instability.

## Observed Behavioral Deltas
* RTX 3080 Laptop GPU memory-utilization regression is observed on the 580.88+ branch under WDDM on tested consumer mobile RTX 3080 Laptop systems; not reproduced on tested desktop Ampere or professional Ampere (A3000).
* Additional same-platform validation: the RTX 3080 Laptop GPU regression has been reproduced on two distinct ASUS ROG Strix G533QS systems with different serials and GPU UUIDs. Both systems report sane idle NVML memory utilization on older drivers (566.07 / 577.00) and anomalous 100% idle memory-controller utilization on 580.88. Pending Alienware mobile results, treat the current evidence as ASUS ROG Strix G533QS / RTX 3080 Laptop platform-family reproducibility rather than a claim across all RTX 3080 Laptop GPUs or all mobile Ampere GPUs.
* RTX 3060 Laptop GPU shows branch-dependent power-cap policy shift (105W → 80W between 580 and 591), while maintaining correct idle memory-utilization reporting.
* Desktop Ampere (RTX 3060 Ti) validates clean NVML telemetry across 580 and 581 branches under both idle and sustained compute load.
* Professional Ampere (RTX A3000) exhibits stable NVML telemetry across multiple driver branches including 59x.
* Pascal low-end SKUs (GT 1030, GTX 1050 Ti) may expose draw but omit cap fields; WTG handles optional power fields explicitly.
* Observed behavior suggests discrete branch divergence between consumer mobile WDDM SKUs and desktop/professional SKUs.
* Kepler low-end (GT 730 class) exhibits partial NVML surfaces where core APIs load but key telemetry (utilization, power) returns NOT_SUPPORTED; consistent across OEM variants on legacy driver branches.
* Pre-Fermi/Tesla GPUs (8400 GS) fall below NVML support floor; nvidia-smi is non-functional and NVML calls require explicit guarding.
* Maxwell OEM cards (GTX 745) represent a practical lower bound for stable NVML telemetry on modern Windows drivers, with expected omissions (power caps).