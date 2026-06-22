# Windows Application Control Notes

`wtg-ui.exe` is currently an unsigned experimental binary.

On some Windows systems, especially systems with Smart App Control, Windows Defender Application Control, App Control for Business, or enterprise application-control policies enabled, Windows may block the UI binary from launching.

During bench testing, `wtg-ui.exe` was blocked on a Windows 11 validation system with Code Integrity / Application Control enforcement enabled.

Windows reported:

```text
Program 'wtg-ui.exe' failed to run: An Application Control policy has blocked this file.
```

The Code Integrity event log showed Event IDs `3033` and `3077`, including:

```text
wtg-ui.exe did not meet the Enterprise signing level requirements or violated code integrity policy.
```

This is expected behavior for unsigned experimental binaries on policy-enforced systems. It does not mean the UI failed, that NVML failed, or that the executable is malicious.

If `wtg-ui.exe` is blocked, options include:

- build and run on a development machine without restrictive application-control policy
- sign the binary with a trusted certificate
- allowlist the binary or hash according to local policy
- use `wtg.exe` for CLI validation workflows

Do not disable organization-managed application-control policy unless you own and control the system and understand the security impact.
