# WTG Test Matrix (Early Validation)

This matrix records **empirical test results** collected during early WTG development. It documents observed behavior only and does not imply official support or compatibility guarantees.

Raw run logs are not published to avoid exposing host-specific identifiers.

---

## Tested GPU / Driver Combinations

## Tested GPU / Driver Combinations

| GPU Model                  | Architecture   | Driver Version(s)     | CUDA Version | OS        | NVML Load | NVML Mem Util (Idle) | --once | --watch | Notes                                                                 |
| -------------------------- | -------------- | --------------------- | ------------ | --------- | --------- | -------------------- | ------ | ------- | --------------------------------------------------------------------- |
| RTX 3080 Laptop GPU       | Ampere         | 566.07                | 12.7         | Win11 x64 | ✅         | OK                   | ✅      | ✅       | Stable                                                                 |
| RTX 3080 Laptop GPU       | Ampere         | 577.00                | 12.9         | Win11 x64 | ✅         | OK                   | ✅      | ✅       | Stable                                                                 |
| RTX 3080 Laptop GPU       | Ampere         | 580.88+               | 13.0+        | Win11 x64 | ✅         | 100% @ idle (first observed 580.88) | ✅ | ✅ | Driver-branch regression; normal thermals/power; WDDM consumer SKU    |
| RTX 4070 Laptop GPU       | Ada            | 591.59                | 13.1         | Win11 x64 | ✅         | OK                   | ✅      | ✅       | Stable; transient nvidia-smi formatting anomaly observed              |
| RTX 3060 Laptop GPU       | Ampere         | 512.74                | 11.6         | Win11 x64 | ✅         | OK                   | ✅      | ✅       | Power draw available; cap unavailable                                  |
| RTX 3060 Ti (Desktop OEM) | Ampere         | 581.04                | 13.0         | Win11 x64 | ✅         | OK                   | ✅      | ✅       | Desktop reference; 200W cap exposed; load validated (199W/200W)       |
| RTX A3000 12GB Laptop GPU | Ampere (Pro)   | 580.92 / 591.59       | 13.0 / 13.1  | Win11 x64 | ✅         | OK                   | ✅      | ✅       | Power draw and cap present; stable NVML across driver branches         |
| GTX 1050 Ti               | Pascal         | 465.89                | 11.3         | Win10 x64 | ❌         | N/A                  | ❌      | N/A     | NVML DLL not loadable                                                  |
| GTX 1050 Ti               | Pascal         | 581.80                | 13.0         | Win10 x64 | ✅         | OK                   | ⚠️      | ⚠️       | NVML loads; power field unsupported (no cap exposure)                  |
| GT 1030                   | Pascal         | 581.04                | 13.0         | Win11 x64 | ✅         | OK                   | ✅      | ✅       | Power reported as N/A; expected for low-power SKU                      |

---

## Notes

* **NVML Mem Util (Idle)** reflects `nvmlDeviceGetUtilizationRates().memory` observed at system idle.
* For regressions, the **lowest driver version where the behavior is observed** is recorded (e.g., `580.88+`). `nvmlDeviceGetUtilizationRates().memory` observed at system idle.
* `100% @ idle` indicates a driver-reported saturation value with normal power, VRAM usage, and thermals; this does **not** imply actual memory bandwidth saturation.
* Behavior has been observed to change discretely across driver branches on certain mobile GeForce platforms.
* NVML availability on Windows is **driver-version dependent**.
* Presence of `nvidia-smi` does not guarantee NVML is loadable by third-party tools.
* Power telemetry is split: **draw is common; caps are optional**, even on RTX laptops.
* WTG fails fast and explicitly when NVML is unavailable.

* Desktop RTX 3060 Ti (581.04) confirms clean NVML memory utilization reporting and proper power-cap exposure under full load (≈199W / 200W).
* RTX 3080 Laptop GPU memory-utilization regression is confined (so far) to 580.88+ branch under WDDM on consumer mobile SKU; not reproduced on desktop Ampere or professional Ampere (A3000).
* Professional Ampere (RTX A3000) exhibits stable NVML telemetry across multiple driver branches including 59x.
* Pascal low-end SKUs (GT 1030, GTX 1050 Ti) may expose draw but omit cap fields; WTG handles optional power fields explicitly.
* Observed behavior reinforces segmentation hypothesis: consumer mobile WDDM branch behavior can diverge discretely from desktop and professional SKUs.
