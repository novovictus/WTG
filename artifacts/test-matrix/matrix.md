# WTG Test Matrix (Early Validation)

This matrix records **empirical test results** collected during early
WTG development. It documents observed behavior only and does not imply
official support or compatibility guarantees.

Raw run logs are not published to avoid exposing host-specific identifiers.

---

## Tested GPU / Driver Combinations

| GPU Model            | Architecture | Driver Version | CUDA Version | OS        | NVML Load | --once | --watch | Notes |
|---------------------|--------------|----------------|--------------|-----------|-----------|--------|---------|-------|
| RTX 3080 Laptop GPU | Ampere       | 566.07         | 12.7         | Win11 x64 | ✅        | ✅     | ✅      | Stable |
| RTX 4070 Laptop GPU | Ada          | 591.59         | 13.1         | Win11 x64 | ✅        | ✅     | ✅      | Stable |
| GTX 1050 Ti         | Pascal       | 465.89         | 11.3         | Win10 x64 | ❌        | ❌     | N/A     | NVML DLL not loadable |
| GTX 1050 Ti         | Pascal       | 581.80         | 13.0         | Win10 x64 | ✅        | ✅     | ✅      | Power cap fields partially unsupported |

---

## Notes

- NVML availability on Windows is **driver-version dependent**.
- Presence of `nvidia-smi` does not guarantee NVML is loadable by third-party tools.
- Legacy GPUs (Pascal / GTX 10-series) function correctly with modern drivers.
- WTG fails fast and explicitly when NVML is unavailable.
