// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Adam Hooper

use std::env;

use wtg_core::nvml::{
    probe_context::{query_probe_context_for_gpu_with_ctx, GpuProbeContext},
    GpuSnapshot, NvmlContext,
};

use crate::mqtt::{
    format_availability_topic, format_ha_discovery_topic, MqttSink, DEFAULT_HA_DISCOVERY_PREFIX,
    DEFAULT_TOPIC_PREFIX,
};
use crate::CliArgs;

pub(crate) fn publish_snapshots(
    sink: &mut MqttSink,
    args: &CliArgs,
    ctx: &NvmlContext,
    snapshots: &[GpuSnapshot],
    tick_seq: u64,
    tick_ts: &str,
) -> Result<(), String> {
    let topic_prefix = topic_prefix(args);
    let node_id = node_id(args);
    let host_name = local_hostname();

    for snapshot in snapshots {
        let topic = format_state_topic(topic_prefix, node_id, snapshot.index);
        let context = query_probe_context_for_gpu_with_ctx(ctx, snapshot.index);
        let payload =
            format_state_payload(&host_name, node_id, snapshot, &context, tick_seq, tick_ts);
        sink.publish_raw(&topic, payload.as_bytes(), false)?;
    }

    Ok(())
}

pub(crate) fn publish_ha_discovery_for_snapshots(
    sink: &mut MqttSink,
    args: &CliArgs,
    snapshots: &[GpuSnapshot],
) -> Result<(), String> {
    if !args.mqtt_ha_discovery {
        return Ok(());
    }

    let topic_prefix = topic_prefix(args);
    let node_id = node_id(args);
    let ha_prefix = ha_prefix(args);
    let availability_topic = format_availability_topic(topic_prefix, node_id);
    let retain_discovery = args.mqtt_retain_discovery;

    for snapshot in snapshots {
        for metric in HA_SENSOR_METRICS {
            let topic = format_ha_discovery_topic(ha_prefix, node_id, snapshot.index, metric.key);
            let state_topic = format_state_topic(topic_prefix, node_id, snapshot.index);
            let payload = format_ha_discovery_payload(
                node_id,
                snapshot,
                metric,
                &state_topic,
                &availability_topic,
            );
            sink.publish_raw(&topic, payload.as_bytes(), retain_discovery)?;
        }
    }

    let spec = ha_availability_online_spec(topic_prefix, node_id);
    sink.publish_raw(&spec.topic, &spec.payload, spec.retain)
}

fn topic_prefix(args: &CliArgs) -> &str {
    args.mqtt_topic_prefix
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_TOPIC_PREFIX)
}

fn node_id(args: &CliArgs) -> &str {
    args.mqtt_node_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_default()
}

fn ha_prefix(args: &CliArgs) -> &str {
    args.mqtt_ha_prefix
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_HA_DISCOVERY_PREFIX)
}

fn format_state_topic(topic_prefix: &str, node_id: &str, gpu_index: u32) -> String {
    format!("{topic_prefix}/{node_id}/gpu{gpu_index}/state")
}

struct MqttPublishSpec {
    topic: String,
    payload: Vec<u8>,
    retain: bool,
}

fn ha_availability_online_spec(topic_prefix: &str, node_id: &str) -> MqttPublishSpec {
    MqttPublishSpec {
        topic: format_availability_topic(topic_prefix, node_id),
        payload: b"online".to_vec(),
        retain: true,
    }
}

fn local_hostname() -> String {
    env::var("COMPUTERNAME")
        .or_else(|_| env::var("HOSTNAME"))
        .map(|value| value.trim().to_string())
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

struct HaSensorMetric {
    key: &'static str,
    name: &'static str,
    unit: Option<&'static str>,
    device_class: Option<&'static str>,
    state_class: Option<&'static str>,
}

const HA_SENSOR_METRICS: &[HaSensorMetric] = &[
    HaSensorMetric {
        key: "util_gpu_pct",
        name: "GPU utilization",
        unit: Some("%"),
        device_class: None,
        state_class: Some("measurement"),
    },
    HaSensorMetric {
        key: "util_mem_controller_pct",
        name: "Memory controller utilization",
        unit: Some("%"),
        device_class: None,
        state_class: Some("measurement"),
    },
    HaSensorMetric {
        key: "vram_used_mib",
        name: "VRAM used",
        unit: Some("MiB"),
        device_class: None,
        state_class: Some("measurement"),
    },
    HaSensorMetric {
        key: "vram_total_mib",
        name: "VRAM total",
        unit: Some("MiB"),
        device_class: None,
        state_class: Some("measurement"),
    },
    HaSensorMetric {
        key: "power_w",
        name: "Power",
        unit: Some("W"),
        device_class: Some("power"),
        state_class: Some("measurement"),
    },
    HaSensorMetric {
        key: "power_limit_w",
        name: "Power limit",
        unit: Some("W"),
        device_class: Some("power"),
        state_class: Some("measurement"),
    },
    HaSensorMetric {
        key: "temp_c",
        name: "Temperature",
        unit: Some("\u{00b0}C"),
        device_class: Some("temperature"),
        state_class: Some("measurement"),
    },
    HaSensorMetric {
        key: "driver_version",
        name: "Driver version",
        unit: None,
        device_class: None,
        state_class: None,
    },
    HaSensorMetric {
        key: "cuda_driver_version",
        name: "CUDA driver version",
        unit: None,
        device_class: None,
        state_class: None,
    },
    HaSensorMetric {
        key: "compute_mode",
        name: "Compute mode",
        unit: None,
        device_class: None,
        state_class: None,
    },
    HaSensorMetric {
        key: "perf_state",
        name: "Performance state",
        unit: None,
        device_class: None,
        state_class: None,
    },
    HaSensorMetric {
        key: "pci_bus_id",
        name: "PCI bus ID",
        unit: None,
        device_class: None,
        state_class: None,
    },
    HaSensorMetric {
        key: "wtg_version",
        name: "WTG version",
        unit: None,
        device_class: None,
        state_class: None,
    },
];

fn format_ha_discovery_payload(
    node_id: &str,
    snapshot: &GpuSnapshot,
    metric: &HaSensorMetric,
    state_topic: &str,
    availability_topic: &str,
) -> String {
    let unique_id = format!("wtg_{node_id}_gpu{}_{}", snapshot.index, metric.key);
    let device_id = format!("wtg_{node_id}_gpu{}", snapshot.index);
    let device_name = format!("WTG {node_id} GPU {}", snapshot.index);

    let mut payload = format!(
        concat!(
            "{{",
            "\"name\":{},",
            "\"unique_id\":{},",
            "\"state_topic\":{},",
            "\"value_template\":{},",
            "\"availability_topic\":{},",
            "\"payload_available\":\"online\",",
            "\"device\":{{",
            "\"identifiers\":[{}],",
            "\"name\":{},",
            "\"manufacturer\":\"WTG\",",
            "\"model\":{},",
            "\"sw_version\":{}",
            "}}"
        ),
        json_string(&format!("GPU {} {}", snapshot.index, metric.name)),
        json_string(&unique_id),
        json_string(state_topic),
        json_string(&format!("{{{{ value_json.{} }}}}", metric.key)),
        json_string(availability_topic),
        json_string(&device_id),
        json_string(&device_name),
        json_string(&snapshot.name),
        json_string(env!("CARGO_PKG_VERSION"))
    );

    if let Some(unit) = metric.unit {
        payload.push_str(",\"unit_of_measurement\":");
        payload.push_str(&json_string(unit));
    }
    if let Some(device_class) = metric.device_class {
        payload.push_str(",\"device_class\":");
        payload.push_str(&json_string(device_class));
    }
    if let Some(state_class) = metric.state_class {
        payload.push_str(",\"state_class\":");
        payload.push_str(&json_string(state_class));
    }

    payload.push('}');
    payload
}

fn format_state_payload(
    host_name: &str,
    node_id: &str,
    snapshot: &GpuSnapshot,
    context: &GpuProbeContext,
    tick_seq: u64,
    tick_ts: &str,
) -> String {
    format!(
        concat!(
            "{{",
            "\"wtg_version\":{},",
            "\"payload_schema\":1,",
            "\"tick_seq\":{},",
            "\"tick_ts\":{},",
            "\"host\":{},",
            "\"node_id\":{},",
            "\"gpu_index\":{},",
            "\"gpu_name\":{},",
            "\"gpu_uuid\":{},",
            "\"driver_version\":{},",
            "\"cuda_driver_version\":{},",
            "\"compute_mode\":{},",
            "\"perf_state\":{},",
            "\"pci_bus_id\":{},",
            "\"temp_c\":{},",
            "\"util_gpu_pct\":{},",
            "\"util_mem_controller_pct\":{},",
            "\"vram_used_mib\":{},",
            "\"vram_total_mib\":{},",
            "\"power_w\":{},",
            "\"power_limit_w\":{}",
            "}}"
        ),
        json_string(env!("CARGO_PKG_VERSION")),
        tick_seq,
        json_string(tick_ts),
        json_string(host_name),
        json_string(node_id),
        snapshot.index,
        json_string(&snapshot.name),
        json_string(&snapshot.uuid),
        json_string(&context.driver_version),
        json_string(&context.cuda_driver_version),
        json_string(&context.compute_mode),
        json_string(&context.perf_state),
        json_string(&context.pci_bus_id),
        optional_u32_json(snapshot.temp_c),
        snapshot.gpu_util_pct,
        snapshot.mem_util_pct,
        bytes_to_mib(snapshot.mem_used_bytes),
        bytes_to_mib(snapshot.mem_total_bytes),
        optional_w_json(mw_to_w(snapshot.power_mw)),
        optional_w_json(mw_to_w(snapshot.power_limit_mw))
    )
}

fn bytes_to_mib(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
}

fn mw_to_w(mw: Option<u32>) -> Option<f32> {
    mw.map(|mw| mw as f32 / 1000.0)
}

fn optional_u32_json(value: Option<u32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "null".to_string())
}

fn optional_w_json(value: Option<f32>) -> String {
    value
        .map(|value| format!("{value:.1}"))
        .unwrap_or_else(|| "null".to_string())
}

fn json_string(value: &str) -> String {
    format!("\"{}\"", json_escape(value))
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());

    for c in value.chars() {
        match c {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            c if c.is_control() => escaped.push_str(&format!("\\u{:04x}", c as u32)),
            c => escaped.push(c),
        }
    }

    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_topic_uses_predictable_shape() {
        assert_eq!(
            format_state_topic("wtg", "node-a", 2),
            "wtg/node-a/gpu2/state"
        );
    }

    #[test]
    fn ha_availability_online_publish_is_retained() {
        let spec = ha_availability_online_spec("wtg", "node-a");

        assert_eq!(spec.topic, "wtg/node-a/status");
        assert_eq!(spec.payload, b"online".to_vec());
        assert!(spec.retain);
    }

    #[test]
    fn state_payload_has_expected_fields_and_json_escaping() {
        let snapshot = GpuSnapshot {
            index: 0,
            name: "GPU \"A\"".to_string(),
            uuid: "uuid".to_string(),
            mem_used_bytes: 512 * 1024 * 1024,
            mem_total_bytes: 1024 * 1024 * 1024,
            gpu_util_pct: 42,
            mem_util_pct: 7,
            temp_c: Some(55),
            power_mw: Some(12_300),
            power_limit_mw: Some(45_600),
        };
        let context = GpuProbeContext {
            driver_version: "580.88".to_string(),
            cuda_driver_version: "13000".to_string(),
            compute_mode: "Default".to_string(),
            perf_state: "P8".to_string(),
            pci_bus_id: "00000000:01:00.0".to_string(),
        };

        let payload =
            format_state_payload("host", "node-a", &snapshot, &context, 123, "1780420000.123");

        assert!(payload.contains(&format!(
            "\"wtg_version\":\"{}\"",
            env!("CARGO_PKG_VERSION")
        )));
        assert!(payload.contains("\"payload_schema\":1"));
        assert!(payload.contains("\"tick_seq\":123"));
        assert!(payload.contains("\"tick_ts\":\"1780420000.123\""));
        assert!(payload.contains("\"host\":\"host\""));
        assert!(payload.contains("\"node_id\":\"node-a\""));
        assert!(payload.contains("\"gpu_index\":0"));
        assert!(payload.contains("\"gpu_name\":\"GPU \\\"A\\\"\""));
        assert!(payload.contains("\"gpu_uuid\":\"uuid\""));
        assert!(payload.contains("\"driver_version\":\"580.88\""));
        assert!(payload.contains("\"cuda_driver_version\":\"13000\""));
        assert!(payload.contains("\"compute_mode\":\"Default\""));
        assert!(payload.contains("\"perf_state\":\"P8\""));
        assert!(payload.contains("\"pci_bus_id\":\"00000000:01:00.0\""));
        assert!(payload.contains("\"temp_c\":55"));
        assert!(payload.contains("\"util_gpu_pct\":42"));
        assert!(payload.contains("\"util_mem_controller_pct\":7"));
        assert!(payload.contains("\"vram_used_mib\":512"));
        assert!(payload.contains("\"vram_total_mib\":1024"));
        assert!(payload.contains("\"power_w\":12.3"));
        assert!(payload.contains("\"power_limit_w\":45.6"));
    }

    #[test]
    fn ha_discovery_payload_has_expected_numeric_sensor_fields() {
        let snapshot = GpuSnapshot {
            index: 0,
            name: "GPU \"A\"".to_string(),
            uuid: "uuid".to_string(),
            mem_used_bytes: 512 * 1024 * 1024,
            mem_total_bytes: 1024 * 1024 * 1024,
            gpu_util_pct: 42,
            mem_util_pct: 7,
            temp_c: Some(55),
            power_mw: Some(12_300),
            power_limit_mw: Some(45_600),
        };
        let metric = HA_SENSOR_METRICS
            .iter()
            .find(|metric| metric.key == "power_w")
            .unwrap();

        let payload = format_ha_discovery_payload(
            "node-a",
            &snapshot,
            metric,
            "wtg/node-a/gpu0/state",
            "wtg/node-a/status",
        );

        assert!(payload.contains("\"name\":\"GPU 0 Power\""));
        assert!(payload.contains("\"unique_id\":\"wtg_node-a_gpu0_power_w\""));
        assert!(payload.contains("\"state_topic\":\"wtg/node-a/gpu0/state\""));
        assert!(payload.contains("\"value_template\":\"{{ value_json.power_w }}\""));
        assert!(payload.contains("\"availability_topic\":\"wtg/node-a/status\""));
        assert!(payload.contains("\"payload_available\":\"online\""));
        assert!(payload.contains(
            "\"device\":{\"identifiers\":[\"wtg_node-a_gpu0\"],\"name\":\"WTG node-a GPU 0\""
        ));
        assert!(payload.contains("\"manufacturer\":\"WTG\""));
        assert!(payload.contains("\"model\":\"GPU \\\"A\\\"\""));
        assert!(payload.contains(&format!("\"sw_version\":\"{}\"", env!("CARGO_PKG_VERSION"))));
        assert!(payload.contains("\"unit_of_measurement\":\"W\""));
        assert!(payload.contains("\"device_class\":\"power\""));
        assert!(payload.contains("\"state_class\":\"measurement\""));
    }

    #[test]
    fn ha_discovery_payload_omits_numeric_metadata_for_string_sensor() {
        let snapshot = GpuSnapshot {
            index: 1,
            name: "GPU B".to_string(),
            uuid: "uuid".to_string(),
            mem_used_bytes: 512 * 1024 * 1024,
            mem_total_bytes: 1024 * 1024 * 1024,
            gpu_util_pct: 42,
            mem_util_pct: 7,
            temp_c: Some(55),
            power_mw: Some(12_300),
            power_limit_mw: Some(45_600),
        };
        let metric = HA_SENSOR_METRICS
            .iter()
            .find(|metric| metric.key == "perf_state")
            .unwrap();

        let payload = format_ha_discovery_payload(
            "node-a",
            &snapshot,
            metric,
            "wtg/node-a/gpu1/state",
            "wtg/node-a/status",
        );

        assert!(payload.contains("\"unique_id\":\"wtg_node-a_gpu1_perf_state\""));
        assert!(payload.contains("\"value_template\":\"{{ value_json.perf_state }}\""));
        assert!(!payload.contains("unit_of_measurement"));
        assert!(!payload.contains("device_class"));
        assert!(!payload.contains("state_class"));
    }
}
