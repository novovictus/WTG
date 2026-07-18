# MQTT and Home Assistant

WTG can publish live NVIDIA/NVML watch telemetry to an existing MQTT broker and optionally publish retained Home Assistant discovery configuration.

WTG is the collector and publisher. It does not run or configure the broker, open firewall rules, manage subscriber access, install dashboards, or create downstream template packages.

## Publish live telemetry

```powershell
.\wtg.exe --watch --sink mqtt --mqtt-host 127.0.0.1 --mqtt-port 1884 --mqtt-node-id testnode
```

State topic:

```text
wtg/<node_id>/gpu<index>/state
```

One JSON state payload is published per NVIDIA GPU per watch tick. State messages use QoS 0 and are not retained.

## Authentication

```powershell
$env:WTG_MQTT_PASSWORD = "your-password"

.\wtg.exe --watch `
  --sink mqtt `
  --mqtt-host homeassistant.local `
  --mqtt-node-id bench `
  --mqtt-username wtg `
  --mqtt-password-env WTG_MQTT_PASSWORD
```

Anonymous MQTT remains supported when no authentication flags are supplied. See [Configuration](configuration.md) for config-file and password-handling details.

## Home Assistant discovery

```powershell
.\wtg.exe --watch `
  --sink mqtt `
  --mqtt-host homeassistant.local `
  --mqtt-node-id bench `
  --mqtt-username wtg `
  --mqtt-password-env WTG_MQTT_PASSWORD `
  --mqtt-ha-discovery `
  --mqtt-retain-discovery
```

Discovery topics follow:

```text
homeassistant/sensor/wtg_<node_id>_gpu<index>_<metric>/config
```

Availability topic:

```text
wtg/<node_id>/status
```

With discovery enabled, WTG publishes discovery configuration, retained `online` availability, and live state messages. The MQTT Last Will publishes retained `offline` availability after an unexpected disconnect. State messages remain non-retained.

Home Assistant creates deterministic WTG devices and metric entities. Entity IDs include the advertised hostname, GPU index, and metric, for example:

```text
sensor.wtg_<hostname>_gpu_0_gpu_0_power
sensor.wtg_<hostname>_gpu_0_gpu_0_gpu_utilization
sensor.wtg_<hostname>_gpu_0_gpu_0_temperature
```

## Discovery cleanup

```powershell
.\wtg.exe --sink mqtt --mqtt-ha-remove-discovery --config .\wtg.toml
```

Cleanup clears retained WTG discovery configuration and availability topics. It does not delete normal state topics. Home Assistant may still require stale registry entries to be removed manually.

## Architecture boundary

```text
NVIDIA NVML
  -> WTG collection and MQTT publication
  -> Home Assistant discovery and WTG entities
  -> optional downstream templates and dashboards
```

MQTT captures validate publication and transport. Home Assistant device/entity creation validates discovery. Neither becomes an alternate telemetry authority.

[WTG HA Redline](https://github.com/novovictus/wtg-ha-redline) is an optional downstream presentation project that consumes WTG-discovered entities. Its template sensors, scores, warning states, gauges, and dashboard cards are not WTG provider truth.

AMD ADL and Intel Level Zero provider output are not currently published through MQTT or Home Assistant discovery.
