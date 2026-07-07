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

- staging the packaged WTG payload from a controller-only workspace
- purging stale local result mirrors and remote destination folders before a run
- running `wtg_test.ps1` and `wtg_providers_test.ps1` on every target over SSH
- treating the controller host as a normal SSH target rather than a special local runner
- capturing stdout logs separately from evidence files
- pulling source-of-truth result files back to the orchestrator
- removing remote transport zip files after result collection
- writing a run-level `manifest.json`

The remote harness must not rename evidence files. Evidence filenames are owned by the smoke scripts.

## Controller Host as Remote Target

The controller shell may run on ROG, but ROG is still executed through the same SSH/SCP path as every other node.

```text
ROG controller shell
  -> builds payload zip in controller-only workspace
  -> SSH/SCP to rog
  -> purges rog destination
  -> expands payload
  -> runs smoke scripts remotely
  -> zips results remotely
  -> pulls results back
```

This deliberate SSH-inception model validates the same transport, staging, purge, execution, result collection, and cleanup path on all nodes.

The controller payload must not live inside the remote staging/share tree. In particular, do not use this as the controller payload directory:

```text
C:\Users\plays\Desktop\share\0.3.0-rc1
```

The harness uses a controller-only workspace instead:

```text
C:\Users\plays\Desktop\wtg_batch_controller_0.3.0-rc1
```

The local share path remains only a result mirror/convenience path:

```text
C:\Users\plays\Desktop\share\0.3.0-rc1\results
```

## Expected Run Layout

```text
remote_runs/
  <run_id>/
    manifest.json
    logs/
      orchestrator.log
    stdout/
      rog.connectivity.stdout.txt
      rog.prepare.stdout.txt
      rog.expand.stdout.txt
      rog.run.stdout.txt
      rog.cleanup.stdout.txt
      bench.*
      surface.*
      nuc.*
    results/
      wtg_*.txt
      wtg_providers_*.txt
```

Run policy:

```text
execution_model = all-targets-over-ssh
controller_host_is_remote_target = true
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
