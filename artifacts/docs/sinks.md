# WTG Sinks

WTG supports file sinks and an optional MQTT sink.

## JSONL

```powershell
.\wtg.exe --once --sink jsonl
.\wtg.exe --probe --sink jsonl
.\wtg.exe --once --stats --sink jsonl
```

JSONL sinks create timestamped files named like:

```text
wtg_sink_<timestamp>_<suffix>.jsonl
```

Most modes write line records.

`--once --stats --sink jsonl` is the exception: it writes one compact canonical NVML provenance JSON object using schema `wtg.nvml.stats.v1`.

## CSV

```powershell
.\wtg.exe --once --sink csv
.\wtg.exe --probe --sink csv
.\wtg.exe --once --stats --sink csv
```

CSV sinks create timestamped files named like:

```text
wtg_sink_<timestamp>_<suffix>.csv
```

CSV remains the legacy flat structured sink for this release, including `--once --stats --sink csv`.

## MQTT

```powershell
.\wtg.exe --watch --sink mqtt --mqtt-host 127.0.0.1 --mqtt-port 1884 --mqtt-node-id testnode
```

MQTT publishes live `--watch` snapshot payloads to an existing broker.

MQTT is a delivery surface, not the telemetry source of truth.

## Support matrix

| Mode | JSONL | CSV | MQTT | Notes |
| --- | --- | --- | --- | --- |
| `--once` | yes | yes | no | concise snapshot |
| `--once --stats` | yes | yes | no | JSONL writes compact provenance JSON; CSV remains legacy flat stats |
| `--watch` | yes | yes | yes | MQTT publishes live snapshot payloads |
| `--watch --stats` | yes | yes | yes | legacy stats/watch behavior for this release |
| `--probe` | yes | yes | no | validation output |
| `--probe-fields` | yes | yes | no | field-ID diagnostics |
