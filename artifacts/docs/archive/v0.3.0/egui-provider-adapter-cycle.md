# WTG 0.3.0 eGUI Provider Adapter Cycle Notes

Archived operational notes for the 0.2.9 to 0.3.0 transition and the eGUI provider-backed adapter view.

## Scope

- NVIDIA/NVML remains the primary truth path.
- AMD ADL and Intel Level Zero remain provider-scoped supporting paths.
- The eGUI keeps a single Devices pane and selected-device detail pane.
- No provider dropdown, provider tree, topology-only duplicate rows, or cross-vendor normalization.

## Provider-backed device view

The eGUI uses the existing provider implementations:

- NVIDIA through `wtg_core::nvml`
- AMD through `amd_adl::collect_once`
- Intel through `intel_level_zero::collect_visible_sample`

Telemetry-capable rows only are shown. Unavailable facts must not render as zero, and provider-native fields are not relabeled as NVML equivalents.

## Packaging lesson

`wtg-ui.exe` and `wtg.exe` must be distributed together because the UI launches the CLI for MQTT workflows. If `wtg.exe` is missing, the UI must report that explicitly.

## Deferred MQTT and Home Assistant work

The observed Home Assistant unavailable-state behavior was not introduced by the provider-backed eGUI work. Retained discovery, availability/LWT, state-topic alignment, publisher lifecycle, and stale entity cleanup remain separate integration work.
