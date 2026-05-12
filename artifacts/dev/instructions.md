> Internal working instructions.
> Not user-facing. Subject to change as WTG matures.

# WTG Development Instructions

## Current branch status

Branch: `spike/beta-5`

Current focus: repeatable empirical validation of NVML memory-utilization behavior under Windows WDDM.

This branch is no longer in the original CFP/package-freeze state. The previous v0.1.2/v0.2.0-beta2 task list has been completed or superseded. Current work is centered on validating and documenting the probe/probe-fields instrumentation as the `v0.2.0-beta4` probe-fields checkpoint.

Current validated capabilities:

- `--once` normal snapshot output remains available.
- `--watch --interval <ms>` normal watch output remains available.
- `--stats` output remains unchanged.
- `--sink jsonl` writes `{"line":"..."}` records for `--once`, non-stats `--watch`, and `--probe`.
- `--sink csv` writes structured header + row output for `--probe` only.
- `--once --sink csv` and non-stats `--watch --sink csv` create zero-byte placeholder CSV files.
- `--probe` emits context-rich one-shot probe output.
- `--probe-fields --field-id <u32>` compares the normal NVML utilization path against selected `nvmlDeviceGetFieldValues` results using safe `nvml-wrapper` APIs.

Current probe context fields:

- `wtg.version`
- `driver.version`
- `cuda.driver_version`
- `gpu.compute_mode`
- `gpu.perf_state`
- `gpu.pci.bus_id`

Current key evidence fields:

- `util.gpu_pct`
- `util.mem_controller_pct`
- `vram.used_mib`
- `vram.total_mib`
- `gpu.perf_state`

Important interpretation:

- `util.mem_controller_pct` is NVML memory-controller utilization, not VRAM occupancy.
- VRAM occupancy is shown separately as `vram.used_mib` / `vram.total_mib`.
- `gpu.perf_state` reports the NVML performance state, such as `P0` through `P15` or `Unknown`.
- `P0` is the highest-performance state; higher-numbered states are lower-power states. `N/A` means the query was unsupported or failed.
- `gpu.perf_state` is useful for confirming low-power or idle state during captures.
- On some Windows WDDM / NVIDIA driver combinations, `util.mem_controller_pct` may report `100` even when VRAM occupancy is low.
- This branch can show that condition and can also show whether selected NVML field-values queries work in the same device/session.
- This branch does not infer driver causality in code.

---

## Required starting checks for any dev session

Run from the repository root:

```powershell
cd C:\Users\plays\source\github_wtg\WTG
git status -sb
git log --oneline --decorate -12
cargo build
```

Expected clean state:

```text
## spike/beta-5...origin/spike/beta-5
```

If generated sink files exist, remove them before committing:

```powershell
Remove-Item .\wtg_sink_*.csv, .\wtg_sink_*.jsonl -ErrorAction SilentlyContinue
git status -sb
```

Do not commit generated sink files, validation bundles, backup archives, or local review artifacts unless explicitly intended.

---

## Current validation test

Run after code or documentation changes that could affect CLI behavior:

```powershell
cargo build

cargo run -- --once
cargo run -- --once --stats
cargo run -- --once --sink jsonl
cargo run -- --once --sink csv

cargo run -- --probe
cargo run -- --probe --sink csv
cargo run -- --probe --sink jsonl

cargo run -- --probe-fields --field-id 74 --field-id 78 --field-id 83 --field-id 94 --field-id 95

cargo run -- --probe-fields
cargo run -- --field-id 83
cargo run -- --probe-fields --probe
```

Expected behavior:

- `cargo build` passes.
- `--once` output remains human-readable snapshot output.
- `--once --stats` remains the existing stats contract.
- `--once --sink jsonl` writes a non-empty JSONL file with `{"line":"..."}` records.
- `--once --sink csv` creates a zero-byte placeholder CSV file.
- `--probe` prints one context-rich probe block.
- `--probe --sink csv` writes one structured CSV header + row.
- `--probe --sink jsonl` writes a JSONL `{"line":"..."}` probe record.
- `--probe-fields --field-id ...` prints the utilization path and one field block per requested field ID.
- Invalid invocations exit with code `2` from the child app. `cargo run` may report the child failure as a command failure.

Windows watch interruption note:

```powershell
cargo run -- --watch --interval 1000 --sink jsonl
```

Stop after a few ticks with Ctrl+C. Through `cargo run`, Windows may report `STATUS_CONTROL_C_EXIT`. That is expected for manual interruption. The JSONL sink should contain flushed records.

---

## Current useful probe-fields batch

The local `nvml-wrapper-sys` field constants identified these useful supported field IDs on the RTX 3080 Laptop test system:

| ID | Constant | Notes |
|---:|---|---|
| 74 | `NVML_FI_DEV_PERF_POLICY_POWER` | Supported, `u64` |
| 75 | `NVML_FI_DEV_PERF_POLICY_THERMAL` | Supported, `u64` |
| 76 | `NVML_FI_DEV_PERF_POLICY_SYNC_BOOST` | Supported, `u64` |
| 77 | `NVML_FI_DEV_PERF_POLICY_BOARD_LIMIT` | Supported, `u64` |
| 78 | `NVML_FI_DEV_PERF_POLICY_LOW_UTILIZATION` | Supported, `u64` |
| 79 | `NVML_FI_DEV_PERF_POLICY_RELIABILITY` | Supported, `u64` |
| 80 | `NVML_FI_DEV_PERF_POLICY_TOTAL_APP_CLOCKS` | Supported, `u64` |
| 81 | `NVML_FI_DEV_PERF_POLICY_TOTAL_BASE_CLOCKS` | Supported, `u64` |
| 82 | `NVML_FI_DEV_MEMORY_TEMP` | Field error on tested 3080 Laptop |
| 83 | `NVML_FI_DEV_TOTAL_ENERGY_CONSUMPTION` | Supported, `u64` |
| 94 | `NVML_FI_DEV_PCIE_REPLAY_COUNTER` | Supported, `u32` |
| 95 | `NVML_FI_DEV_PCIE_REPLAY_ROLLOVER_COUNTER` | Supported, `u32` |

Useful command:

```powershell
cargo run -- --probe-fields --field-id 74 --field-id 78 --field-id 83 --field-id 94 --field-id 95
```

Broader field batch:

```powershell
cargo run -- --probe-fields `
  --field-id 74 `
  --field-id 75 `
  --field-id 76 `
  --field-id 77 `
  --field-id 78 `
  --field-id 79 `
  --field-id 80 `
  --field-id 81 `
  --field-id 82 `
  --field-id 83 `
  --field-id 94 `
  --field-id 95
```

Current interpretation of field-values results:

- Supported field-values queries returning typed values show that `nvmlDeviceGetFieldValues` is callable on the same device/session.
- The tested local bindings do not expose a direct current memory-utilization field-value counterpart.
- Therefore, current results can weaken a caller-side field-ID/header mismatch explanation, but do not fully disprove an internal NVIDIA field mapping or driver-path change.
- Cross-driver comparison is still required before making a regression claim.

---

## Repeatable test harness goal

Next practical work item: create a repeatable validation harness for same-branch captures across systems and driver versions.

Targets:

1. Development laptop:
   - RTX 3080 Laptop GPU
   - Current known regression state: `driver.version: 580.88`, `cuda.driver_version: 13000`, `gpu.perf_state: P8`, `util.mem_controller_pct: 100`, low VRAM occupancy.

2. Bench system:
   - RTX 3060 Ti desktop GPU
   - Same WTG branch and same command set.
   - Same driver version first if available, then driver-branch comparison as needed.

Harness requirements:

- PowerShell 5.1 compatible.
- ASCII-clean output.
- Run from repository root or from a portable drop folder.
- Capture command, stdout, stderr, exit code, timestamp, git commit, and generated sink files.
- Preserve `--probe` and `--probe-fields` outputs as plain text.
- Preserve `--probe --sink csv` as structured CSV.
- Do not require Codex or external network access.
- Do not modify repo state except writing ignored artifact output under an explicit validation folder.

Suggested artifact directory:

```text
artifacts/validation/<timestamp>_<machine>_<driver>/
```

Suggested minimum capture set:

```powershell
cargo build
cargo run -- --probe
cargo run -- --probe --sink csv
cargo run -- --probe-fields --field-id 74 --field-id 78 --field-id 83 --field-id 94 --field-id 95
```

Optional watch capture:

```powershell
cargo run -- --watch --interval 1000 --sink jsonl
```

Stop after several ticks and preserve the JSONL records.

---

## Next development priorities

1. Build the repeatable validation harness.
2. Run the harness on the dev laptop.
3. Run the same harness on the bench RTX 3060 Ti system.
4. Compare same-driver behavior across mobile 3080 Laptop and desktop 3060 Ti.
5. If possible, repeat across a known-good older driver branch and a known-bad newer branch.
6. Update `artifacts/test-matrix/matrix.md` with results.
7. Use this branch as the `v0.2.0-beta4` probe-fields checkpoint before adding the repeatable harness.

Do not version/tag until:

- The harness is repeatable.
- At least one full artifact capture is preserved.
- README and internal instructions match current behavior.
- `cargo build` and validation checks pass from a clean worktree.

---

## Historical completed tasks

These are retained only for continuity. Do not treat them as current work items.

- Repo hygiene and guardrails completed.
- `wtg-core` crate documentation alignment completed.
- Snapshot authority and semantic alignment completed.
- Fake value removal completed for temperature and relevant snapshot path.
- Monotonic tick counter work completed where applicable.
- NVML context reuse completed.
- Probe/sink branch work completed through:
  - JSONL sink creation/write/flush
  - probe mode
  - probe CSV rows
  - sink/probe module split
  - `wtg.version` in probe output
  - experimental `--probe-fields`
  - driver/runtime context fields
  - `gpu.perf_state` in probe context
  - README documentation for probe-fields behavior

---

## Standing constraints

- No Unicode punctuation in CLI/log output. Keep CLI/log output ASCII-clean for PowerShell 5.1 portability.
- Keep debug build as default for validation (`cargo build`). Use release build only for performance/shipping checks.
- Treat WTG as the engine-dyno truth layer compared with Task Manager's higher-level abstraction model.
- Do not infer causality in code. Emit raw observations and let the test matrix support the conclusion.
- Do not merge or tag experimental probe work until validation artifacts support the release claim.

