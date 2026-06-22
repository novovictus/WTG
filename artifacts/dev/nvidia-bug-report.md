# NVIDIA Bug Report Collector

`artifacts/dev/nvidia-bug-report.ps1` is a best-effort Windows-native helper modeled after NVIDIA's Linux `nvidia-bug-report.sh` flow and used for NVIDIA Developer bug #6162407.

It is not part of the WTG runtime and is not required for normal CLI or GUI use.

It is retained as a development/research artifact for packaging WTG/NVML evidence with Windows and NVIDIA diagnostic context.

## Expected run directory

- `wtg.exe`
- `wtg_test.ps1`
- `nvidia-bug-report.ps1`

## Example

```powershell
powershell -NoProfile -File .\nvidia-bug-report.ps1
```

The script runs `wtg_test.ps1`, detects the generated WTG result file, collects Windows and NVIDIA diagnostic context, enables NVML debug logging for later diagnostic calls, runs an additional `wtg.exe --probe --sink jsonl` capture, writes a collection manifest, and produces a driver-versioned ZIP bundle named like:

```text
nvidia_bug_6162407_driver_<driver>.zip
```
