# WTG field and surface provenance

This document maps where provider and WTG facts are represented and where the validation harness preserves them. **Correlation documents provenance and representation. It does not assert cross-provider semantic equivalence.**

The authoritative raw capture for every CLI surface is `manifest.json.tests[].stdout` and `manifest.json.tests[].stderr`, mirrored in the corresponding `evidence.txt` test section. File-sink content is preserved as `manifest.json.tests[].sink_output[]` and in the same evidence section. Each package is produced beneath the harness-local `results/` directory from the `wtg.exe` adjacent to `Invoke-WtgValidation.ps1`; WTG identity is recorded in `manifest.json.wtg` (raw version output and `binary_sha256`), harness identity in `manifest.json.harness` (`harness_name` and `harness_sha256`), and the format in `manifest.json.evidence_format.schema`. The harness does not rename fields.

## NVIDIA / NVML

| WTG source/provider | CLI representation | `--stats` JSON path | JSONL sink path | CSV column | MQTT / eGUI | Harness capture |
| --- | --- | --- | --- | --- | --- | --- |
| NVML `util.mem_controller_pct` | default once/watch, probe context | `util.mem_controller_pct` in `wtg.nvml.stats.v1` | raw JSONL record from `--once --stats --sink jsonl` | legacy stats CSV row | MQTT state `util_mem_controller_pct`; eGUI NVIDIA device view where supported | NVIDIA stats stdout; JSONL/CSV raw sink content |
| NVML `util.gpu_pct` | default once/watch | `util.gpu_pct` | raw JSONL record | legacy stats CSV row | MQTT state `util_gpu_pct`; eGUI NVIDIA view | same |
| NVML VRAM | default once/watch | `vram.used_mib`, `vram.total_mib` | raw JSONL record | `vram_used_mib`, `vram_total_mib` | MQTT `vram_used_mib`, `vram_total_mib`; eGUI NVIDIA view | same |
| NVML power | default once/watch | provider-truth stats power field(s) | raw JSONL record | legacy stats CSV row | MQTT power field(s); eGUI NVIDIA view | stats/stdout and sink captures |
| NVML temperature, performance state, driver/runtime context | once/watch/probe/probe-fields as exposed | provenance/stats object as exposed | raw JSONL record when supported | applicable raw CSV column | MQTT/eGUI only where WTG currently exposes it | default, stats, probe, and probe-fields transcript |

## AMD / ADL and ADLX

| WTG source/provider | CLI representation | `--stats` JSON path | JSONL / CSV / MQTT | eGUI | Harness capture |
| --- | --- | --- | --- | --- | --- |
| AMD ADL (`wtg.provider.amd.adl`) adapter and activity facts | `--provider amd --once`, `--watch`, `--probe` where selected | provider-native `wtg.amd_adl.stats.v1` object | not implemented for AMD provider modes | provider-backed AMD rows where available | AMD once/stats/watch stdout and stderr |
| AMD ADL raw adapter fields, including ADL naming and availability | provider-native rendered output | exact provider-native object paths as emitted | not implemented | where exposed | same; plus CIM provenance in `manifest.json.adapters` |
| AMD ADLX diagnostic/runtime data (`wtg.provider.amd.adlx`) | current AMD once output when exposed by WTG | ADLX provider-native fields/schema when emitted | no AMD file/MQTT sink | where exposed | AMD once stdout/stderr; runtime DLL observations in `capability_observations` |

## Intel / Level Zero and Sysman

| WTG source/provider | CLI representation | `--stats` JSON path | JSONL / CSV / MQTT | eGUI | Harness capture |
| --- | --- | --- | --- | --- | --- |
| Intel Level Zero (`wtg.provider.intel.level_zero`) device facts | `--provider intel --once`, `--watch`, `--probe` where selected | provider-native `wtg.intel_level_zero.stats.v1` object | not implemented for Intel provider modes | provider-backed Intel rows where available | Intel once/stats/watch stdout and stderr |
| Intel Sysman memory, power, engine, frequency, and temperature facts | provider-native once/watch/probe rendering, subject to runtime support | exact provider-native paths as emitted | not implemented | where exposed | same; `ze_loader.dll` observation plus complete raw provider output |

## Transport boundary

NVIDIA MQTT is a live NVIDIA watch transport, not a file sink. The harness invokes it only when the caller explicitly supplies an existing safe local validation path. It captures the exact invocation, stdout, stderr, timing, and exit code; a local subscriber capture remains external evidence and is not fabricated by the harness. AMD and Intel do not currently publish through MQTT.

The table intentionally maps representation locations rather than declaring that similarly named values from different providers mean the same thing.
