// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Adam Hooper

use std::env;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::process;
use std::time::Duration;

use wtg_core::nvml::{
    probe_context::{query_probe_context_for_gpu_with_ctx, GpuProbeContext},
    GpuSnapshot, NvmlContext,
};

pub(crate) const DEFAULT_MQTT_PORT: u16 = 1883;
pub(crate) const DEFAULT_TOPIC_PREFIX: &str = "wtg";
pub(crate) const DEFAULT_HA_DISCOVERY_PREFIX: &str = "homeassistant";

#[derive(Debug, Clone)]
pub(crate) struct MqttHaDiscoveryOptions {
    prefix: String,
    retain: bool,
}

impl MqttHaDiscoveryOptions {
    pub(crate) fn new(prefix: String, retain: bool) -> Result<Self, String> {
        Ok(Self {
            prefix: validate_topic_prefix("--mqtt-ha-prefix", &prefix)?,
            retain,
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MqttOptions {
    host: String,
    port: u16,
    topic_prefix: String,
    node_id: String,
    ha_discovery: Option<MqttHaDiscoveryOptions>,
}

impl MqttOptions {
    pub(crate) fn new(
        host: String,
        port: u16,
        topic_prefix: String,
        node_id: String,
        ha_discovery: Option<MqttHaDiscoveryOptions>,
    ) -> Result<Self, String> {
        if port == 0 {
            return Err("--mqtt-port must be between 1 and 65535.".to_string());
        }

        let host = host.trim().to_string();
        if host.is_empty() {
            return Err("--sink mqtt requires --mqtt-host <host>.".to_string());
        }

        let topic_prefix = validate_topic_prefix("--mqtt-topic-prefix", &topic_prefix)?;
        let node_id = validate_node_id(&node_id)?;

        Ok(Self {
            host,
            port,
            topic_prefix,
            node_id,
            ha_discovery,
        })
    }
}

pub(crate) struct MqttSink {
    options: MqttOptions,
    host_name: String,
    stream: TcpStream,
}

impl MqttSink {
    pub(crate) fn connect(options: MqttOptions) -> Result<Self, String> {
        let mut stream =
            TcpStream::connect((options.host.as_str(), options.port)).map_err(|e| {
                format!(
                    "failed to connect to MQTT broker {}:{}: {e}",
                    options.host, options.port
                )
            })?;
        stream
            .set_nodelay(true)
            .map_err(|e| format!("failed to configure MQTT TCP stream: {e}"))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| format!("failed to configure MQTT read timeout: {e}"))?;
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| format!("failed to configure MQTT write timeout: {e}"))?;

        let client_id = format!("wtg-{}-{}", options.node_id, process::id());
        send_connect_packet(&mut stream, &client_id)?;
        read_connack(&mut stream)?;

        Ok(Self {
            options,
            host_name: local_hostname(),
            stream,
        })
    }

    pub(crate) fn publish_snapshots(
        &mut self,
        ctx: &NvmlContext,
        snapshots: &[GpuSnapshot],
        tick_seq: u64,
        tick_ts: &str,
    ) -> Result<(), String> {
        for snapshot in snapshots {
            let topic = self.topic_for_gpu(snapshot.index);
            let context = query_probe_context_for_gpu_with_ctx(ctx, snapshot.index);
            let payload = format_state_payload(
                &self.host_name,
                &self.options.node_id,
                snapshot,
                &context,
                tick_seq,
                tick_ts,
            );
            self.publish(&topic, payload.as_bytes(), false)?;
        }

        Ok(())
    }

    pub(crate) fn publish_ha_discovery_for_snapshots(
        &mut self,
        snapshots: &[GpuSnapshot],
    ) -> Result<(), String> {
        let Some(discovery) = self.options.ha_discovery.clone() else {
            return Ok(());
        };

        let availability_topic = self.availability_topic();

        for snapshot in snapshots {
            for metric in HA_SENSOR_METRICS {
                let topic = format_ha_discovery_topic(
                    &discovery.prefix,
                    &self.options.node_id,
                    snapshot.index,
                    metric.key,
                );
                let state_topic = self.topic_for_gpu(snapshot.index);
                let payload = format_ha_discovery_payload(
                    &self.options.node_id,
                    snapshot,
                    metric,
                    &state_topic,
                    &availability_topic,
                );
                self.publish(&topic, payload.as_bytes(), discovery.retain)?;
            }
        }

        Ok(())
    }

    pub(crate) fn publish_ha_availability_online(&mut self) -> Result<(), String> {
        if self.options.ha_discovery.is_none() {
            return Ok(());
        }

        let availability_topic = self.availability_topic();
        self.publish(&availability_topic, b"online", false)
    }

    fn topic_for_gpu(&self, gpu_index: u32) -> String {
        format_state_topic(&self.options.topic_prefix, &self.options.node_id, gpu_index)
    }

    fn availability_topic(&self) -> String {
        format_availability_topic(&self.options.topic_prefix, &self.options.node_id)
    }

    fn publish(&mut self, topic: &str, payload: &[u8], retain: bool) -> Result<(), String> {
        let mut body = Vec::with_capacity(2 + topic.len() + payload.len());
        push_mqtt_string(&mut body, topic)?;
        body.extend_from_slice(payload);
        write_packet(&mut self.stream, publish_packet_type(retain), &body)
    }
}

fn send_connect_packet(stream: &mut TcpStream, client_id: &str) -> Result<(), String> {
    let mut body = Vec::new();
    push_mqtt_string(&mut body, "MQTT")?;
    body.push(4);
    body.push(0x02);
    body.extend_from_slice(&0u16.to_be_bytes());
    push_mqtt_string(&mut body, client_id)?;

    write_packet(stream, 0x10, &body)
}

fn write_packet(stream: &mut TcpStream, packet_type: u8, body: &[u8]) -> Result<(), String> {
    let mut packet = Vec::with_capacity(1 + 4 + body.len());
    packet.push(packet_type);
    packet.extend_from_slice(&encode_remaining_length(body.len())?);
    packet.extend_from_slice(body);

    stream
        .write_all(&packet)
        .map_err(|e| format!("failed to write MQTT packet: {e}"))
}

fn read_connack(stream: &mut TcpStream) -> Result<(), String> {
    let mut packet_type = [0u8; 1];
    stream
        .read_exact(&mut packet_type)
        .map_err(|e| format!("failed to read MQTT CONNACK: {e}"))?;
    if packet_type[0] != 0x20 {
        return Err(format!(
            "unexpected MQTT packet while waiting for CONNACK: 0x{:02x}",
            packet_type[0]
        ));
    }

    let remaining_len = read_remaining_length(stream)?;
    let mut body = vec![0u8; remaining_len];
    stream
        .read_exact(&mut body)
        .map_err(|e| format!("failed to read MQTT CONNACK body: {e}"))?;

    if body.len() != 2 {
        return Err(format!(
            "invalid MQTT CONNACK length: expected 2, got {}",
            body.len()
        ));
    }

    if body[1] != 0 {
        return Err(format!(
            "MQTT broker rejected connection: {}",
            connack_reason(body[1])
        ));
    }

    Ok(())
}

fn push_mqtt_string(out: &mut Vec<u8>, value: &str) -> Result<(), String> {
    let bytes = value.as_bytes();
    if bytes.len() > u16::MAX as usize {
        return Err("MQTT string exceeds 65535 bytes.".to_string());
    }

    out.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

fn encode_remaining_length(mut len: usize) -> Result<Vec<u8>, String> {
    if len > 268_435_455 {
        return Err("MQTT packet exceeds maximum remaining length.".to_string());
    }

    let mut out = Vec::new();
    loop {
        let mut byte = (len % 128) as u8;
        len /= 128;
        if len > 0 {
            byte |= 0x80;
        }
        out.push(byte);

        if len == 0 {
            break;
        }
    }

    Ok(out)
}

fn read_remaining_length(stream: &mut TcpStream) -> Result<usize, String> {
    let mut multiplier = 1usize;
    let mut value = 0usize;

    for _ in 0..4 {
        let mut encoded_byte = [0u8; 1];
        stream
            .read_exact(&mut encoded_byte)
            .map_err(|e| format!("failed to read MQTT remaining length: {e}"))?;

        value += ((encoded_byte[0] & 0x7f) as usize) * multiplier;
        if encoded_byte[0] & 0x80 == 0 {
            return Ok(value);
        }

        multiplier *= 128;
    }

    Err("malformed MQTT remaining length.".to_string())
}

fn connack_reason(code: u8) -> &'static str {
    match code {
        1 => "unacceptable protocol version",
        2 => "client identifier rejected",
        3 => "server unavailable",
        4 => "bad username or password",
        5 => "not authorized",
        _ => "unknown return code",
    }
}

fn validate_topic_prefix(label: &str, prefix: &str) -> Result<String, String> {
    let trimmed = prefix.trim().trim_matches('/').to_string();
    if trimmed.is_empty() {
        return Err(format!("{label} must not be empty."));
    }
    validate_topic_text(label, &trimmed)?;
    Ok(trimmed)
}

fn validate_node_id(node_id: &str) -> Result<String, String> {
    let trimmed = node_id.trim().to_string();
    if trimmed.is_empty() {
        return Err("--sink mqtt requires --mqtt-node-id <id>.".to_string());
    }
    if trimmed.contains('/') {
        return Err("--mqtt-node-id must be a single topic segment without '/'.".to_string());
    }
    validate_topic_text("--mqtt-node-id", &trimmed)?;
    Ok(trimmed)
}

fn validate_topic_text(label: &str, value: &str) -> Result<(), String> {
    if value.contains('\0') {
        return Err(format!("{label} must not contain NUL bytes."));
    }
    if value.contains('#') || value.contains('+') {
        return Err(format!(
            "{label} must not contain MQTT wildcard characters."
        ));
    }
    Ok(())
}

fn format_state_topic(topic_prefix: &str, node_id: &str, gpu_index: u32) -> String {
    format!("{topic_prefix}/{node_id}/gpu{gpu_index}/state")
}

fn format_availability_topic(topic_prefix: &str, node_id: &str) -> String {
    format!("{topic_prefix}/{node_id}/status")
}

fn format_ha_discovery_topic(
    ha_prefix: &str,
    node_id: &str,
    gpu_index: u32,
    metric: &str,
) -> String {
    format!("{ha_prefix}/sensor/wtg_{node_id}_gpu{gpu_index}_{metric}/config")
}

fn publish_packet_type(retain: bool) -> u8 {
    if retain {
        0x31
    } else {
        0x30
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
        crate::bytes_to_mib(snapshot.mem_used_bytes),
        crate::bytes_to_mib(snapshot.mem_total_bytes),
        optional_w_json(crate::mw_to_w(snapshot.power_mw)),
        optional_w_json(crate::mw_to_w(snapshot.power_limit_mw))
    )
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
    fn options_apply_topic_defaults_and_normalization() {
        let options = MqttOptions::new(
            " broker ".to_string(),
            DEFAULT_MQTT_PORT,
            "/wtg/".to_string(),
            "node-a".to_string(),
            None,
        )
        .unwrap();

        assert_eq!(options.host, "broker");
        assert_eq!(options.port, 1883);
        assert_eq!(options.topic_prefix, "wtg");
        assert_eq!(options.node_id, "node-a");
        assert!(options.ha_discovery.is_none());
    }

    #[test]
    fn ha_discovery_options_apply_defaults_and_normalization() {
        assert_eq!(DEFAULT_HA_DISCOVERY_PREFIX, "homeassistant");

        let discovery = MqttHaDiscoveryOptions::new("/homeassistant/".to_string(), true).unwrap();

        assert_eq!(discovery.prefix, "homeassistant");
        assert!(discovery.retain);
    }

    #[test]
    fn remaining_length_encoding_matches_mqtt_examples() {
        assert_eq!(encode_remaining_length(0).unwrap(), vec![0x00]);
        assert_eq!(encode_remaining_length(127).unwrap(), vec![0x7f]);
        assert_eq!(encode_remaining_length(128).unwrap(), vec![0x80, 0x01]);
        assert_eq!(
            encode_remaining_length(16_384).unwrap(),
            vec![0x80, 0x80, 0x01]
        );
    }

    #[test]
    fn state_topic_uses_predictable_shape() {
        assert_eq!(
            format_state_topic("wtg", "node-a", 2),
            "wtg/node-a/gpu2/state"
        );
        assert_eq!(
            format_availability_topic("wtg", "node-a"),
            "wtg/node-a/status"
        );
    }

    #[test]
    fn ha_discovery_topic_uses_predictable_shape() {
        assert_eq!(
            format_ha_discovery_topic("homeassistant", "node-a", 2, "power_w"),
            "homeassistant/sensor/wtg_node-a_gpu2_power_w/config"
        );
    }

    #[test]
    fn publish_packet_type_sets_retain_only_when_requested() {
        assert_eq!(publish_packet_type(false), 0x30);
        assert_eq!(publish_packet_type(true), 0x31);
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
