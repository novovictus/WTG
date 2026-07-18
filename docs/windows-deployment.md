# Windows Deployment

WTG is Windows-native and does not require Python, Docker, or a listening network service.

## Binaries

A release build produces:

```text
wtg.exe
wtg-ui.exe
```

Keep both files together when using UI launch actions because `wtg-ui.exe` invokes the matching CLI binary.

## Provider runtimes

- NVIDIA default operation requires a usable NVML runtime.
- AMD operation requires a usable `atiadlxx.dll`.
- Intel operation requires a usable Level Zero/Sysman runtime.

Provider absence or runtime failure is reported explicitly and should not be converted into zero-valued telemetry.

## Application control

`wtg-ui.exe` is currently unsigned. Smart App Control, Windows Defender Application Control, App Control for Business, and enterprise code-integrity policy may block it.

Observed policy blocks may reference Code Integrity Event IDs 3033 or 3077. This is expected for an unsigned experimental binary on a policy-enforced system and does not indicate provider failure.

Use a controlled development system, sign or allowlist the binary according to local policy, or use `wtg.exe` for CLI workflows. Do not disable organization-managed policy without authorization.

## Network behavior

WTG opens an outbound connection only when MQTT is explicitly enabled. It does not run a broker, expose a listening service, configure firewall rules, or manage subscriber access.
