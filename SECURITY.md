# Security Policy

WTG is a Windows-native telemetry characterization tool. NVIDIA/NVML remains the primary provider. The current development version also includes an experimental local AMD ADL provider path that runs only when explicitly requested.

The current development version includes:

- `wtg.exe`, the CLI telemetry, validation, capture, and MQTT publisher surface
- `wtg-ui.exe`, the experimental egui viewer/configurator/launcher
- optional experimental AMD ADL provider enumeration behind explicit provider selection
- optional MQTT publishing during `--watch`
- optional Home Assistant MQTT discovery
- optional TOML configuration loaded only when explicitly requested

## Supported Security Posture

WTG is designed as a local telemetry tool and outbound publisher.

WTG does not:

- expose a listening network service
- act as an MQTT broker
- subscribe to MQTT command topics
- accept remote control messages
- configure firewall rules
- fetch remote configuration
- auto-load `wtg.toml`
- execute scripts or shell commands in the MQTT watch path

MQTT support is an outbound publishing feature. Home Assistant discovery is also outbound and opt-in.

The experimental AMD ADL provider path is local provider enumeration. It does not create a listener, subscribe to command topics, or fetch remote provider configuration.

## Remote Attack Surface

Based on the current implementation, WTG does not intentionally expose a direct remote attack surface.

The primary remote-adjacent risks are operational:

- a malicious or unavailable MQTT broker can reject, hang, or disrupt publishing
- plaintext MQTT can expose credentials or telemetry on an untrusted network
- telemetry can be observed or modified by a network attacker if MQTT traffic is not otherwise protected
- downstream consumers such as Home Assistant dashboards are responsible for safely handling the data they consume

These risks do not imply remote code execution in WTG. They are transport, availability, and deployment-environment considerations.

## Credential Handling

WTG supports MQTT username/password authentication for trusted local use.

For better hygiene, prefer environment-variable based MQTT passwords.

Avoid placing MQTT passwords directly on the command line or storing them in plaintext config files unless the deployment environment is trusted and local.

## Configuration Handling

WTG configuration is explicit.

WTG does not auto-create or auto-load `wtg.toml`.

A config file is used only when passed explicitly.

The UI configurator is a convenience surface over the same CLI/config behavior.

## Reporting Security Issues

If you believe WTG implements an exploitable vulnerability, please open a GitHub issue or contact the maintainer directly.

Useful reports should include:

- WTG version or commit
- operating system version
- GPU and NVIDIA driver version
- command line used
- whether MQTT, Home Assistant discovery, config loading, or experimental provider selection was enabled
- expected behavior
- observed behavior
- reproduction steps

WTG findings about GPU telemetry behavior should remain separate from security vulnerability reports unless the issue creates a concrete security impact.
