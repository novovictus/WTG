// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Adam Hooper

use std::fmt;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::process;
use std::time::Duration;

use wtg_core::nvml::GpuSnapshot;

pub(crate) const DEFAULT_MQTT_PORT: u16 = 1883;
pub(crate) const DEFAULT_TOPIC_PREFIX: &str = "wtg";
pub(crate) const DEFAULT_HA_DISCOVERY_PREFIX: &str = "homeassistant";

#[derive(Clone)]
pub(crate) struct MqttAuthOptions {
    username: String,
    password: String,
}

impl MqttAuthOptions {
    pub(crate) fn new(username: String, password: String) -> Result<Self, String> {
        let username = username.trim().to_string();
        if username.is_empty() {
            return Err("--mqtt-username must not be empty.".to_string());
        }
        if password.is_empty() {
            return Err("MQTT password environment variable must not be empty.".to_string());
        }

        Ok(Self { username, password })
    }
}

impl fmt::Debug for MqttAuthOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MqttAuthOptions")
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MqttHaDiscoveryOptions {
    prefix: String,
}

impl MqttHaDiscoveryOptions {
    pub(crate) fn new(prefix: String, _retain: bool) -> Result<Self, String> {
        Ok(Self {
            prefix: validate_topic_prefix("--mqtt-ha-prefix", &prefix)?,
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MqttOptions {
    host: String,
    port: u16,
    topic_prefix: String,
    node_id: String,
    auth: Option<MqttAuthOptions>,
    ha_availability: bool,
    ha_discovery: Option<MqttHaDiscoveryOptions>,
}

impl MqttOptions {
    pub(crate) fn new(
        host: String,
        port: u16,
        topic_prefix: String,
        node_id: String,
        auth: Option<MqttAuthOptions>,
        ha_availability: bool,
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
            auth,
            ha_availability,
            ha_discovery,
        })
    }

    fn availability_topic(&self) -> String {
        format_availability_topic(&self.topic_prefix, &self.node_id)
    }

    fn connect_will(&self) -> Option<MqttWill> {
        if !self.ha_availability {
            return None;
        }

        Some(MqttWill {
            topic: self.availability_topic(),
            payload: "offline".to_string(),
            retain: true,
        })
    }
}

#[derive(Debug, Clone)]
struct MqttWill {
    topic: String,
    payload: String,
    retain: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MqttPublishSpec {
    topic: String,
    payload: Vec<u8>,
    retain: bool,
}

pub(crate) struct MqttSink {
    options: MqttOptions,
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
        let will = options.connect_will();
        send_connect_packet(
            &mut stream,
            &client_id,
            options.auth.as_ref(),
            will.as_ref(),
        )?;
        read_connack(&mut stream)?;

        Ok(Self { options, stream })
    }

    pub(crate) fn publish_ha_discovery_cleanup_for_snapshots(
        &mut self,
        snapshots: &[GpuSnapshot],
    ) -> Result<(), String> {
        let Some(discovery) = self.options.ha_discovery.clone() else {
            return Ok(());
        };

        let gpu_indices = snapshots
            .iter()
            .map(|snapshot| snapshot.index)
            .collect::<Vec<_>>();
        for spec in ha_discovery_cleanup_specs(
            &discovery.prefix,
            &self.options.topic_prefix,
            &self.options.node_id,
            &gpu_indices,
        ) {
            self.publish_raw(&spec.topic, &spec.payload, spec.retain)?;
        }

        Ok(())
    }

    pub(crate) fn publish_raw(
        &mut self,
        topic: &str,
        payload: &[u8],
        retain: bool,
    ) -> Result<(), String> {
        let mut body = Vec::with_capacity(2 + topic.len() + payload.len());
        push_mqtt_string(&mut body, topic)?;
        body.extend_from_slice(payload);
        write_packet(&mut self.stream, publish_packet_type(retain), &body)
    }
}

fn send_connect_packet(
    stream: &mut TcpStream,
    client_id: &str,
    auth: Option<&MqttAuthOptions>,
    will: Option<&MqttWill>,
) -> Result<(), String> {
    let body = build_connect_body(client_id, auth, will)?;
    write_packet(stream, 0x10, &body)
}

fn build_connect_body(
    client_id: &str,
    auth: Option<&MqttAuthOptions>,
    will: Option<&MqttWill>,
) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    push_mqtt_string(&mut body, "MQTT")?;
    body.push(4);
    body.push(connect_flags(auth, will));
    body.extend_from_slice(&0u16.to_be_bytes());
    push_mqtt_string(&mut body, client_id)?;

    if let Some(will) = will {
        push_mqtt_string(&mut body, &will.topic)?;
        push_mqtt_string(&mut body, &will.payload)?;
    }

    if let Some(auth) = auth {
        push_mqtt_string(&mut body, &auth.username)?;
        push_mqtt_string(&mut body, &auth.password)?;
    }

    Ok(body)
}

fn connect_flags(auth: Option<&MqttAuthOptions>, will: Option<&MqttWill>) -> u8 {
    let mut flags = 0x02;
    if let Some(will) = will {
        flags |= 0x04;
        if will.retain {
            flags |= 0x20;
        }
    }
    if auth.is_some() {
        flags |= 0x80 | 0x40;
    }

    flags
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

pub(crate) fn format_availability_topic(topic_prefix: &str, node_id: &str) -> String {
    format!("{topic_prefix}/{node_id}/status")
}

pub(crate) fn format_ha_discovery_topic(
    ha_prefix: &str,
    node_id: &str,
    gpu_index: u32,
    metric: &str,
) -> String {
    format!("{ha_prefix}/sensor/wtg_{node_id}_gpu{gpu_index}_{metric}/config")
}

const CLEANUP_METRIC_KEYS: &[&str] = &[
    "util_gpu_pct",
    "util_mem_controller_pct",
    "vram_used_mib",
    "vram_total_mib",
    "power_w",
    "power_limit_w",
    "temp_c",
    "driver_version",
    "cuda_driver_version",
    "compute_mode",
    "perf_state",
    "pci_bus_id",
    "wtg_version",
];

fn ha_discovery_cleanup_specs(
    ha_prefix: &str,
    topic_prefix: &str,
    node_id: &str,
    gpu_indices: &[u32],
) -> Vec<MqttPublishSpec> {
    let mut specs = Vec::with_capacity(gpu_indices.len() * CLEANUP_METRIC_KEYS.len() + 1);
    for gpu_index in gpu_indices {
        for metric in CLEANUP_METRIC_KEYS {
            specs.push(MqttPublishSpec {
                topic: format_ha_discovery_topic(ha_prefix, node_id, *gpu_index, metric),
                payload: Vec::new(),
                retain: true,
            });
        }
    }

    specs.push(MqttPublishSpec {
        topic: format_availability_topic(topic_prefix, node_id),
        payload: Vec::new(),
        retain: true,
    });

    specs
}

fn publish_packet_type(retain: bool) -> u8 {
    if retain {
        0x31
    } else {
        0x30
    }
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
            false,
            None,
        )
        .unwrap();

        assert_eq!(options.host, "broker");
        assert_eq!(options.port, 1883);
        assert_eq!(options.topic_prefix, "wtg");
        assert_eq!(options.node_id, "node-a");
        assert!(options.auth.is_none());
        assert!(!options.ha_availability);
        assert!(options.ha_discovery.is_none());
    }

    #[test]
    fn options_support_username_password_auth_with_redacted_debug() {
        let password = String::from_utf8(vec![115, 101, 99, 114, 101, 116]).unwrap();
        let auth = MqttAuthOptions::new(" user ".to_string(), password.clone()).unwrap();
        let options = MqttOptions::new(
            "broker".to_string(),
            DEFAULT_MQTT_PORT,
            "wtg".to_string(),
            "node-a".to_string(),
            Some(auth),
            false,
            None,
        )
        .unwrap();

        assert_eq!(options.auth.as_ref().unwrap().username, "user");
        let debug = format!("{:?}", options.auth.as_ref().unwrap());
        assert!(debug.contains("user"));
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains(&password));
        let options_debug = format!("{options:?}");
        assert!(options_debug.contains("<redacted>"));
        assert!(!options_debug.contains(&password));
    }

    #[test]
    fn ha_discovery_options_apply_defaults_and_normalization() {
        assert_eq!(DEFAULT_HA_DISCOVERY_PREFIX, "homeassistant");

        let discovery = MqttHaDiscoveryOptions::new("/homeassistant/".to_string(), true).unwrap();

        assert_eq!(discovery.prefix, "homeassistant");
    }

    #[test]
    fn options_enable_ha_retained_availability_lwt() {
        let discovery = MqttHaDiscoveryOptions::new("homeassistant".to_string(), true).unwrap();
        let options = MqttOptions::new(
            "broker".to_string(),
            DEFAULT_MQTT_PORT,
            "wtg".to_string(),
            "node-a".to_string(),
            None,
            true,
            Some(discovery),
        )
        .unwrap();

        let will = options.connect_will().unwrap();

        assert_eq!(will.topic, "wtg/node-a/status");
        assert_eq!(will.payload, "offline");
        assert!(will.retain);
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
    fn anonymous_connect_uses_clean_session_flags() {
        let body = build_connect_body("client-a", None, None).unwrap();

        assert_eq!(body[7], 0x02);
        let strings = connect_payload_strings(&body);
        assert_eq!(strings.len(), 1);
        assert_eq!(strings[0], "client-a");
    }

    #[test]
    fn authenticated_connect_uses_username_password_flags_and_payload_order() {
        let password = String::from_utf8(vec![115, 101, 99, 114, 101, 116]).unwrap();
        let auth = MqttAuthOptions::new("user".to_string(), password.clone()).unwrap();

        let body = build_connect_body("client-a", Some(&auth), None).unwrap();

        assert_eq!(body[7], 0xc2);
        let strings = connect_payload_strings(&body);
        assert_eq!(strings.len(), 3);
        assert_eq!(strings[0], "client-a");
        assert_eq!(strings[1], "user");
        assert!(strings[2] == password);
    }

    #[test]
    fn ha_availability_connect_uses_retained_lwt_flags_and_payload_order() {
        let will = MqttWill {
            topic: "wtg/node-a/status".to_string(),
            payload: "offline".to_string(),
            retain: true,
        };

        let body = build_connect_body("client-a", None, Some(&will)).unwrap();

        assert_eq!(body[7], 0x26);
        let strings = connect_payload_strings(&body);
        assert_eq!(
            strings,
            vec![
                "client-a".to_string(),
                "wtg/node-a/status".to_string(),
                "offline".to_string()
            ]
        );
    }

    #[test]
    fn authenticated_ha_availability_connect_keeps_mqtt_payload_order() {
        let password = String::from_utf8(vec![115, 101, 99, 114, 101, 116]).unwrap();
        let auth = MqttAuthOptions::new("user".to_string(), password.clone()).unwrap();
        let will = MqttWill {
            topic: "wtg/node-a/status".to_string(),
            payload: "offline".to_string(),
            retain: true,
        };

        let body = build_connect_body("client-a", Some(&auth), Some(&will)).unwrap();

        assert_eq!(body[7], 0xe6);
        let strings = connect_payload_strings(&body);
        assert_eq!(strings.len(), 5);
        assert_eq!(strings[0], "client-a");
        assert_eq!(strings[1], "wtg/node-a/status");
        assert_eq!(strings[2], "offline");
        assert_eq!(strings[3], "user");
        assert!(strings[4] == password);
    }

    #[test]
    fn topic_helpers_use_predictable_shape() {
        assert_eq!(
            format_availability_topic("wtg", "node-a"),
            "wtg/node-a/status"
        );
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
    fn ha_discovery_cleanup_specs_clear_discovery_and_availability_retained() {
        let specs = ha_discovery_cleanup_specs("homeassistant", "wtg", "node-a", &[0]);

        assert_eq!(specs.len(), CLEANUP_METRIC_KEYS.len() + 1);
        let power = specs
            .iter()
            .find(|spec| spec.topic == "homeassistant/sensor/wtg_node-a_gpu0_power_w/config")
            .unwrap();
        assert!(power.payload.is_empty());
        assert!(power.retain);

        let availability = specs.last().unwrap();
        assert_eq!(availability.topic, "wtg/node-a/status");
        assert!(availability.payload.is_empty());
        assert!(availability.retain);
    }

    fn connect_payload_strings(body: &[u8]) -> Vec<String> {
        let mut offset = 10;
        let mut strings = Vec::new();

        while offset < body.len() {
            let len = ((body[offset] as usize) << 8) | body[offset + 1] as usize;
            offset += 2;
            strings.push(String::from_utf8(body[offset..offset + len].to_vec()).unwrap());
            offset += len;
        }

        strings
    }
}
