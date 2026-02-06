> Internal working instructions.
> Not user-facing. Subject to change as WTG matures.

# instructions.md
WTG Next Steps Task List (triaged: easiest → hardest)

Context snapshot (do not skip)
- Project is Windows-native. No WSL.
- Test harness is PowerShell 5.1–compatible and assumes current working directory contains `wtg.exe` (portable “drop” model).
- Unicode em dash was normalized to ASCII hyphen to avoid mojibake in PS 5.1 logs.
- `.gitattributes` added to enforce LF for source/docs and CRLF for Windows scripts.
- Current focus is correctness + repeatable validation; performance/ship builds come later.

---

## 1) Repo hygiene and guardrails (fast, low risk)
Goal: lock down reproducibility and avoid “future churn” diffs.

Steps:
1. Verify `.gitattributes` exists at repo root and includes:
   - `*.rs`, `*.toml`, `*.md` => `eol=lf`
   - `*.ps1`, `*.bat`, `*.cmd` => `eol=crlf`
2. Confirm the working tree has no unintended EOL churn:
   - `git diff` should show only intentional changes.
3. Document the build/test flow in README (short section):
   - Debug build: `cargo build` → `target\debug\wtg.exe`
   - Portable test drop: copy `wtg.exe` next to `wtg_test.ps1`
   - Run: `powershell -NoProfile -ExecutionPolicy Bypass -File .\wtg_test.ps1`
4. Ensure `results/` output folder used by test script is gitignored (if not already).

Acceptance:
- `git status` clean after normal edits.
- No CRLF/LF warnings on routine diffs.

---

## 2) Fix `wtg-core` crate docs (easy, cleanup; prevents model drift)
Goal: remove stale TODOs and align crate-level comments with reality.

Steps:
1. Update `wtg-core/src/lib.rs` module docs to reflect current state:
   - NVML integration exists
   - snapshot struct exists and is authoritative
2. Delete or rewrite outdated “TODO: implement NVML bootstrap” language.
3. Keep docs short and accurate; avoid aspirational text that will go stale.

Acceptance:
- `cargo doc` (optional) reads correctly.
- No misleading TODOs about already-implemented features.

---

## 3) Eliminate “fake values” in telemetry (easy/medium; correctness)
Goal: stop silently converting “missing/unsupported” into valid-looking numbers.

Known issue patterns:
- Using `unwrap_or(0)` for temperature makes failures look like `0 C`.

Steps:
1. Change `GpuSnapshot.temp_c: u32` → `Option<u32>` (or a dedicated enum if preferred).
2. Treat unsupported/read-failed as `None` and print `N/A`.
3. Apply the same rule to any other “unwrap_or default” fields that represent real-world quantities.

Acceptance:
- When a metric is unsupported, output is `N/A` (not a plausible numeric value).
- No panics; no misleading “0” or empty strings masquerading as truth.

---

## 4) Add a monotonic tick counter (easy; improves testability)
Goal: make logs analyzable without relying on float timestamps.

Steps:
1. Maintain a `tick_seq: u64` incremented each watch loop iteration.
2. In `--stats` output, emit:
   - `tick.seq: <u64>`
   - keep `tick.ts` as-is (useful), but `tick.seq` becomes canonical for counting/drops.

Acceptance:
- Logs show tick number increasing by 1 per tick.
- Easy to detect missed ticks and quantify cadence jitter.

---

## 5) Reduce polling jitter by reusing NVML init + device handles (medium; biggest “bang”)
Goal: stop re-initializing NVML and re-enumerating devices every tick.

Current behavior (likely):
- Each tick calls `Nvml::init()`, `device_count`, `device_by_index`, etc.

Plan (minimal refactor):
1. Introduce an NVML context object constructed once:
   - `struct NvmlContext { nvml: Nvml, devices: Vec<Device> }`
2. Build it at program start (or first call) and reuse it in the watch loop:
   - `fn snapshot_all(ctx: &NvmlContext) -> Result<Vec<GpuSnapshot>, Error>`
3. Keep per-tick work limited to metric queries:
   - utilization, temp, mem, power, uuid, etc.
4. Ensure graceful handling if GPU count changes (rare):
   - simplest: rebuild context on recoverable errors or if `device_count` mismatches.

Acceptance:
- Effective tick cadence improves (less overhead/jitter), especially sub-100ms.
- Behavior unchanged in terms of what metrics are queried and how they print (except for any intentional cleanup like “N/A vs 0”).

---

## 6) Tighten output contract for ingestion (medium; “truth layer” stability)
Goal: make `--stats` output stable and machine-parsable without awkward headers.

Current pattern (likely):
- A bracketed header line like `[stats] gpu=N` plus key/value lines.

Steps:
1. Decide a strict contract:
   - Either all lines are `key: value`
   - Or adopt a consistent section delimiter that won’t confuse parsers
2. Recommended minimal change:
   - Keep a clear per-GPU delimiter line, but also include `gpu.index: N` as a key.
3. Ensure every printed field has a stable name:
   - `gpu.name`, `gpu.uuid`, `gpu.util.gpu_pct`, `gpu.util.mem_pct`, etc.

Acceptance:
- A simple parser can split ticks, then split GPUs, then parse `key: value` pairs deterministically.
- Output does not depend on terminal formatting.

---

## 7) (Optional) Empirical cadence artifact generator (medium; helps demo claims)
Goal: produce a simple quantitative summary of actual tick deltas and metric update cadence.

Steps:
1. Add a `--watch --stats` capture run recommendation:
   - Run for N seconds at 50ms/100ms/250ms
2. Add a tiny post-process script (PowerShell 5.1) that:
   - Extracts `tick.ts` and computes min/mean/p50/p95 deltas
   - Writes a summary block into `results/`
3. Keep it out of core logic if you want minimal code footprint.

Acceptance:
- You can quote numbers like “effective cadence p50/p95” from a reproducible artifact.

---

## 8) Add `--stats --full` and `--stats --experimental` presets (last item)
Goal: implement the planned three-level stats presets with the same snapshot loop/output shape.

Canonical plan:
- `--stats` (core/reliable fields only)
- `--stats --full` (broad set of widely supported NVML fields; print `N/A` when unsupported)
- `--stats --experimental` (best-effort query of everything NVML exposes; never crash; `N/A` for unsupported)

Design constraints:
- Implement as `StatsLevel` enum presetting which optional NVML calls to attempt.
- Same snapshot loop, same printing; only the set of queried/emitted fields changes.
- No heavy new dependencies; optional calls only; never crash.

Include (as available, best-effort):
- CUDA driver version (global)
- per-GPU compute capability (major/minor) if accessible
- compute mode
- best-effort “compute running processes” (may be empty on WDDM)
- accounting mode/stats where available

Acceptance:
- `wtg.exe --stats` works everywhere NVML works (current behavior).
- `wtg.exe --stats --full` emits more fields but still robust.
- `wtg.exe --stats --experimental` never panics; it may print lots of `N/A`, but it runs.

---

Notes to preserve for future context
- Do not reintroduce Unicode punctuation in CLI/log output; keep CLI ASCII-clean for PS 5.1 portability.
- Keep debug build as default for validation (`cargo build`), release build for performance/shipping only.
- Continue treating WTG as the “engine-dyno” truth layer vs Task Manager’s abstraction model (canonical framing).
