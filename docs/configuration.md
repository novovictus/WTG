# Configuration

WTG supports an explicit TOML configuration file for MQTT and Home Assistant discovery behavior.

Configuration is intentionally opt-in:

- WTG does not auto-create a config file.
- WTG does not auto-load `wtg.toml`.
- `--config <path>` is required to load configuration.
- CLI flags override config values.
- Config values override built-in defaults.
- Normal non-MQTT commands remain unaffected.

## Create a template

```powershell
.\wtg.exe --mqtt-init-config
```

This creates `wtg.toml` and refuses to overwrite an existing file.

## Template

```toml
[mqtt]
enabled = false
host = ""
port = 1883
username = ""
password = ""
password_env = ""
topic_prefix = "wtg"
node_id = ""

[mqtt.home_assistant]
discovery = false
discovery_prefix = "homeassistant"
retain_discovery = true
```

## Save config from CLI flags

```powershell
.\wtg.exe --mqtt-save-config `
  --mqtt-host "homeassistant-shop" `
  --mqtt-node-id "bench" `
  --mqtt-username "wtg" `
  --mqtt-password-env "WTG_MQTT_PASSWORD" `
  --mqtt-ha-discovery `
  --mqtt-retain-discovery `
  --force-config
```

`--mqtt-save-config` writes only from explicit CLI flags, validates authentication combinations, sets `[mqtt].enabled = true`, and exits before MQTT or provider initialization.

## Load and override

```powershell
.\wtg.exe --watch --config .\wtg.toml
.\wtg.exe --watch --config .\wtg.toml --mqtt-host "homeassistant-shop"
```

When `[mqtt].enabled = true`, loading the config activates MQTT only for `--watch`. Other modes return a usage error rather than starting an unexpected publisher.

## Discovery cleanup

```powershell
.\wtg.exe --sink mqtt --mqtt-ha-remove-discovery --config .\wtg.toml
```

Cleanup can reuse the same configuration that published retained discovery. It still requires `--sink mqtt`.

## Precedence

```text
CLI flags
  override explicit config values
    override built-in defaults
```

Built-in defaults include MQTT port `1883`, topic prefix `wtg`, and Home Assistant discovery prefix `homeassistant`.

## Responsibility boundary

WTG configuration owns broker connection settings, optional authentication, node identity, topic naming, discovery enablement, and retained discovery/availability behavior.

WTG configuration does not install Home Assistant packages or dashboards and does not define Redline templates, thresholds, scores, or display states.

## Password handling

`--mqtt-password` may be visible in shell history, process listings, logs, and terminal scrollback. A direct password saved to TOML remains plaintext. `--mqtt-password-env` keeps the password out of the WTG command line and config file, but environment-variable exposure still depends on the host environment.

TLS and client certificates remain deferred.
