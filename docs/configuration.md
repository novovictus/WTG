# WTG Configuration

WTG supports an explicit TOML configuration file for MQTT and Home Assistant settings.

Configuration is intentionally conservative:

- WTG does not auto-create a config file.
- WTG does not auto-load `wtg.toml`.
- `--config <path>` is required to load configuration.
- CLI flags override config values.
- Config values override built-in defaults.
- Empty strings in the config template are treated as absent values.
- Normal non-MQTT commands remain unaffected.

## Create a template

```powershell
.\wtg.exe --mqtt-init-config
```

This creates:

```text
.\wtg.toml
```

WTG refuses to overwrite an existing `wtg.toml`.

## Save config from CLI flags

```powershell
.\wtg.exe --mqtt-save-config `
  --mqtt-host "homeassistant-shop" `
  --mqtt-node-id "bench" `
  --mqtt-username "wtg" `
  --mqtt-password "test" `
  --mqtt-ha-discovery `
  --mqtt-retain-discovery `
  --force-config
```

Environment-variable auth variant:

```powershell
$env:WTG_MQTT_PASSWORD = "test"

.\wtg.exe --mqtt-save-config `
  --mqtt-host "homeassistant-shop" `
  --mqtt-node-id "bench" `
  --mqtt-username "wtg" `
  --mqtt-password-env "WTG_MQTT_PASSWORD" `
  --mqtt-ha-discovery `
  --mqtt-retain-discovery `
  --force-config
```

No-auth broker variant:

```powershell
.\wtg.exe --mqtt-save-config `
  --mqtt-host "broker.local" `
  --mqtt-node-id "bench"
```

`--mqtt-save-config` writes from explicit CLI flags only, validates auth combinations, sets `[mqtt].enabled = true`, and exits before MQTT or NVML initialization.

## Template

```toml
# WTG CLI configuration.
# WTG never auto-loads this file. Use --config <path> explicitly.
# Leave environment-specific values blank until you are ready to use them.

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

## Load config explicitly

```powershell
.\wtg.exe --watch --config .\wtg.toml
```

## Override config from CLI

```powershell
.\wtg.exe --watch --config .\wtg.toml --mqtt-host "homeassistant-shop"
```

## Use config for cleanup

```powershell
.\wtg.exe --sink mqtt --mqtt-ha-remove-discovery --config .\wtg.toml
```

Cleanup can use the same config file that published retained Home Assistant discovery. Cleanup still requires `--sink mqtt`.

## MQTT activation from config

`[mqtt].enabled = true` allows MQTT to activate from config without `--sink mqtt`, but only for `--watch`.

```toml
[mqtt]
enabled = true
```

Then:

```powershell
.\wtg.exe --watch --config .\wtg.toml
```

If `[mqtt].enabled = true` is used without `--watch`, WTG returns a usage error.

If `[mqtt].enabled = false` or absent, loading a config file does not activate MQTT by itself. In that case, MQTT still requires explicit `--sink mqtt`.

## Configuration precedence

```text
CLI flags
  override explicit config values
    override built-in defaults
```

Built-in defaults include:

```text
mqtt.port = 1883
mqtt.topic_prefix = "wtg"
mqtt.home_assistant.discovery_prefix = "homeassistant"
```

## Password security notes

- `--mqtt-password` is convenient for trusted local or home-lab use.
- `--mqtt-password` can be visible in the command line, shell history, process listings, logs, and terminal scrollback.
- Saved `wtg.toml` files written with `--mqtt-save-config` and a direct password store the password in plaintext.
- `--mqtt-password-env` keeps the password out of the WTG command line and `wtg.toml`, but setting the environment variable may still expose it depending on the environment.
- TLS and client certificates remain deferred.
