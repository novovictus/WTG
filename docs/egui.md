# eGUI

`wtg-ui.exe` is an experimental egui desktop frontend.

It displays live telemetry and provides a convenience layer over the same explicit TOML configuration model used by the CLI.

The UI is not the reference surface for regression testing or metric capture. Use the CLI, CSV/JSONL sinks, and probe outputs for validation evidence.

## Build

```powershell
cargo build -p wtg-app --release
```

This produces:

```text
target\release\wtg.exe
target\release\wtg-ui.exe
```

## Run

```powershell
.\target\release\wtg-ui.exe
```

On Windows, `wtg-ui.exe` is built as a GUI-subsystem binary and should not open a console window when double-clicked.

## MQTT / Home Assistant configurator

The configurator can:

- load config
- save config
- generate a default config
- generate/copy the equivalent CLI command
- test broker connection
- clear retained Home Assistant discovery
- launch the CLI MQTT publisher
- stop all `wtg.exe` processes

Home Assistant discovery, retained discovery cleanup, retained availability / LWT, TOML config support, and MQTT authentication remain the same CLI/runtime behaviors. The UI is a wrapper over those behaviors, not a second implementation.

## Launch behavior

When `Launch CLI MQTT publisher` is clicked, the UI starts `wtg.exe` with the saved config file, equivalent to:

```powershell
.\wtg.exe --watch --config .\wtg.toml
```

Launch uses the saved config file on disk. If fields were edited in the UI, click `Save config` before launch or the running publisher will continue using the previously saved file contents.

The launched publisher is detached and independent of `wtg-ui.exe`. It can continue running in the background after the UI exits.

`Stop all wtg.exe processes` is intentionally broad. It terminates all running `wtg.exe` processes rather than trying to identify only a publisher launched by the UI.

## Validation boundary

`wtg-ui.exe` visualizes telemetry, but formal validation should use:

- `wtg.exe --once`
- `wtg.exe --watch`
- `wtg.exe --probe`
- `wtg.exe --probe-fields`
- `--sink csv`
- `--sink jsonl`

The UI may visually corroborate CLI output, but screenshots of the UI should not replace captured CLI or sink artifacts in regression testing.
