# WTG 0.3.0 Provider Harness Notes

The supplemental provider harness is separate from the core NVIDIA/NVML matrix.

The core WTG matrix remains focused on valid NVIDIA targets. The supplemental provider harness records AMD ADL and Intel Level Zero behavior as provider-scoped evidence. These outputs may include topology-only records, unavailable fields, runtime availability states, or provider-specific fields that do not map cleanly to NVIDIA/NVML telemetry.

## Current Harness Expectations

- `wtg_test.ps1` validates the default NVIDIA/NVML `--once` path.
- `wtg_providers_test.ps1` validates explicit provider paths:
  - `wtg.exe --provider amd --once`
  - `wtg.exe --provider intel --once`

Harness absent-hardware expectations are explicit:

- exit `2` plus `Provider status: unavailable` is a valid PASS when matching hardware/runtime is absent
- exit `0` plus device telemetry remains required when matching hardware is present

## Remote RC Smoke Harness

`artifacts/dev/wtg_rc_multi_host_smoke.ps1` is the multi-host orchestration harness for release-candidate and provider-spike smoke runs.

The remote harness is responsible for orchestration only:

- staging the packaged WTG payload on the orchestrator and remote targets
- purging stale local and remote destination folders before a run
- running `wtg_test.ps1` and `wtg_providers_test.ps1` locally and over SSH
- capturing stdout logs separately from evidence files
- pulling source-of-truth result files back to the orchestrator
- writing a run-level `manifest.json`

The remote harness must not rename evidence files. Evidence filenames are owned by the smoke scripts.

Expected run layout:

```text
remote_runs/
  <run_id>/
    manifest.json
    logs/
      orchestrator.log
    stdout/
      local.wtg_test.stdout.txt
      local.wtg_providers_test.stdout.txt
      bench.connectivity.stdout.txt
      bench.prepare.stdout.txt
      bench.expand.stdout.txt
      bench.run.stdout.txt
      surface.*
      nuc.*
    results/
      wtg_*.txt
      wtg_providers_*.txt
```

Run policy:

```text
evidence_naming = script-owned
result_collection = flat
transport_zip = temporary
orchestrator_renames_evidence = false
```

The timestamped `remote_runs/<run_id>/results` directory is the collected source-of-truth result set for a harness run. The staging `share/0.3.0-rc1/results` directory is only a local mirror/convenience path.

## Current Follow-Up

- Keep provider outputs provider-scoped.
- Keep absent-hardware handling explicit.
- Keep remote orchestration metadata separate from source-of-truth evidence files.
- Do not translate AMD ADL or Intel Level Zero telemetry into NVIDIA/NVML-equivalent fields.
- Do not synthesize fake zeros or fake unavailable values to create cross-vendor parity.
