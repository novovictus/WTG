# WTG Redline Home Assistant Templates

This directory contains optional Home Assistant package templates for deriving Redline display entities from WTG MQTT discovery sensors.

WTG already publishes MQTT discovery entities when started with MQTT and Home Assistant discovery enabled. The Redline package does not create duplicate MQTT sensors. It consumes the existing WTG MQTT entities and creates local Home Assistant template entities for dashboard display.

## Files

- `wtg_YOUR_HOSTNAME_redline.yaml` - hostname-scoped Home Assistant package template.

`YOUR_HOSTNAME` is intentional. Copy and rename the file for each WTG node you want to monitor.

Example:

```text
redline/wtg_YOUR_HOSTNAME_redline.yaml
```

becomes:

```text
/config/packages/wtg_golden_redline.yaml
```

Then replace every occurrence of `YOUR_HOSTNAME` inside the copied file with the hostname slug used in the discovered WTG entity IDs.

Example discovered entity:

```text
sensor.wtg_golden_gpu_0_gpu_0_power
```

Hostname slug:

```text
golden
```

Replacement:

```text
YOUR_HOSTNAME -> golden
```

## Home Assistant package setup

Enable packages in `configuration.yaml` if not already enabled:

```yaml
homeassistant:
  packages: !include_dir_named packages
```

Create the packages directory if needed:

```text
/config/packages
```

Copy the template into that directory using a hostname-specific filename:

```text
/config/packages/wtg_golden_redline.yaml
```

For multiple WTG nodes, install one copy per node:

```text
/config/packages/wtg_bench_redline.yaml
/config/packages/wtg_rog_redline.yaml
/config/packages/wtg_surface_redline.yaml
```

Each copied file must have `YOUR_HOSTNAME` replaced with that node's actual Home Assistant entity slug.

## Required base entities

The package expects WTG Home Assistant discovery entities with this shape:

```text
sensor.wtg_<hostname>_gpu_0_gpu_0_gpu_utilization
sensor.wtg_<hostname>_gpu_0_gpu_0_memory_controller_utilization
sensor.wtg_<hostname>_gpu_0_gpu_0_power
sensor.wtg_<hostname>_gpu_0_gpu_0_power_limit
sensor.wtg_<hostname>_gpu_0_gpu_0_temperature
sensor.wtg_<hostname>_gpu_0_gpu_0_vram_used
sensor.wtg_<hostname>_gpu_0_gpu_0_vram_total
```

If those base entities do not exist, start WTG with MQTT and Home Assistant discovery enabled first.

Example:

```powershell
$env:WTG_MQTT_PASSWORD = "your-password"

.\wtg.exe --watch `
  --sink mqtt `
  --mqtt-host homeassistant.local `
  --mqtt-node-id golden `
  --mqtt-username wtg `
  --mqtt-password-env WTG_MQTT_PASSWORD `
  --mqtt-ha-discovery `
  --mqtt-retain-discovery
```

## Reloading Home Assistant

After installing or editing the package:

1. Open Home Assistant.
2. Go to Developer Tools -> YAML.
3. Run Check configuration.
4. Run Reload template entities.

If the entities do not appear, restart Home Assistant.

Then search for:

```text
redline
```

Expected derived entities:

```text
sensor.wtg_<hostname>_redline_score
sensor.wtg_<hostname>_redline_state
sensor.wtg_<hostname>_redline_summary
binary_sensor.wtg_<hostname>_redline_sus
```

Home Assistant may preserve older entity IDs in the entity registry. Verify the actual entity IDs before wiring dashboard cards.

## Redline state model

Display sequence:

```text
IDLE -> LOAD -> MAX -> LIMIT -> SUS
```

`SUS` is an override for suspicious telemetry shape, not a higher load state. It is intended to catch cases like memory-controller utilization pinned high while GPU utilization, power, and VRAM allocation remain low.

`LIMIT` is reserved for constraint evidence. Near-power-cap operation by itself is treated as `MAX`, not `LIMIT`.

## Notes

The template currently targets GPU 0. Multi-GPU systems can be handled by copying the package and adjusting both the filename and the GPU index/entity references.

The template uses Fahrenheit temperature thresholds because Home Assistant may convert WTG's temperature entity to the user's configured unit system.
