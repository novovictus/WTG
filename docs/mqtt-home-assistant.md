# MQTT and Home Assistant

WTG can publish live GPU telemetry to an existing MQTT broker.

WTG remains the telemetry collector and publisher. It is not an MQTT broker, does not configure the broker, does not open firewall rules, and does not manage subscriber access.

## Basic MQTT watch publishing

```powershell
.\wtg.exe --watch --sink mqtt --mqtt-host 127.0.0.1 --mqtt-port 1884 --mqtt-node-id testnode
```

Topic shape:

```text
wtg/<node_id>/gpu<index>/state
```

Example:

```text
wtg/testnode/gpu0/state
```

State messages are QoS 0 and not retained.

## Authentication

Environment-variable password:

```powershell
$env:WTG_MQTT_PASSWORD = "your-password"

.\target\release\wtg.exe --watch `
  --sink mqtt `
  --mqtt-host homeassistant.local `
  --mqtt-node-id bench1 `
  --mqtt-username wtg `
  --mqtt-password-env WTG_MQTT_PASSWORD
```

Direct-password variant:

```powershell
.\target\release\wtg.exe --watch `
  --sink mqtt `
  --mqtt-host homeassistant.local `
  --mqtt-node-id bench1 `
  --mqtt-username wtg `
  --mqtt-password "your-password"
```

`--mqtt-password-env` is preferred when practical because it keeps the password out of the WTG command line and saved config file.

## Home Assistant discovery

Home Assistant discovery is opt-in.

```powershell
.\target\release\wtg.exe --watch `
  --sink mqtt `
  --mqtt-host homeassistant.local `
  --mqtt-node-id bench1 `
  --mqtt-username wtg `
  --mqtt-password-env WTG_MQTT_PASSWORD `
  --mqtt-ha-discovery `
  --mqtt-retain-discovery
```

Discovery topic shape:

```text
homeassistant/sensor/wtg_<node_id>_gpu<index>_<metric>/config
```

Availability topic:

```text
wtg/<node_id>/status
```

When Home Assistant discovery is enabled, WTG publishes discovery configs, publishes retained `online` availability, and then publishes state messages. WTG also configures an MQTT Last Will and Testament that publishes retained `offline` availability on unexpected disconnect.

State messages remain non-retained.

## Discovery cleanup

```powershell
.\target\release\wtg.exe `
  --sink mqtt `
  --mqtt-host homeassistant.local `
  --mqtt-node-id bench1 `
  --mqtt-username wtg `
  --mqtt-password-env WTG_MQTT_PASSWORD `
  --mqtt-ha-remove-discovery
```

If discovery was published from a saved config, cleanup can use the same config file:

```powershell
.\wtg.exe --sink mqtt --mqtt-ha-remove-discovery --config .\wtg.toml
```

Cleanup clears retained WTG discovery config topics and retained availability from the broker. It does not delete normal state topics.

Home Assistant may still require stale device/entity registry entries to be deleted manually after retained discovery cleanup.

## Behavior notes

- `--sink mqtt` is supported only with `--watch`, except for the explicit discovery cleanup command.
- WTG opens an outbound connection to the configured broker.
- WTG does not expose a listening network service.
- One JSON state payload is published per GPU per watch tick.
- Payloads include watch tick metadata, `GpuSnapshot` values, and probe-context fields.
- Topic prefix defaults to `wtg`.
- Anonymous MQTT remains supported when no auth flags are provided.
- Discovery is emitted only when `--mqtt-ha-discovery` is set.
- Discovery configs are retained only when `--mqtt-retain-discovery` is set.
- Graceful shutdown offline publishing remains deferred.
