// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Adam Hooper

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};
use wtg_core::nvml::{
    probe_context::GpuProbeContext,
    provenance::{NvmlFact, NvmlFactValue, NvmlNode},
    GpuSnapshot, NvmlContext,
};

pub(crate) fn format_nvml_provenance_stats_pretty(
    snapshots: &[GpuSnapshot],
    _contexts: &[GpuProbeContext],
    ctx: &NvmlContext,
    tick_seq: u64,
    tick_ts: &str,
) -> String {
    let payload = build_nvml_provenance_stats(snapshots, ctx, tick_seq, tick_ts);
    serde_json::to_string_pretty(&payload)
        .expect("NVML provenance stats JSON serialization should succeed")
}

pub(crate) fn format_nvml_provenance_stats_jsonl(
    snapshots: &[GpuSnapshot],
    _contexts: &[GpuProbeContext],
    ctx: &NvmlContext,
    tick_seq: u64,
    tick_ts: &str,
) -> String {
    let payload = build_nvml_provenance_stats(snapshots, ctx, tick_seq, tick_ts);
    serde_json::to_string(&payload)
        .expect("NVML provenance stats JSONL serialization should succeed")
}

fn build_nvml_provenance_stats(
    snapshots: &[GpuSnapshot],
    ctx: &NvmlContext,
    tick_seq: u64,
    tick_ts: &str,
) -> Value {
    let stats = wtg_core::nvml::provenance::collect_provenance_stats(ctx, snapshots);

    json!({
        "schema": "wtg.nvml.stats.v1",
        "provider": "nvidia",
        "provider_source": "nvidia.nvml",
        "provider_authority": "NVIDIA NVML",
        "telemetry_class": "provider_truth",
        "timestamp_unix_ms": now_unix_ms(),
        "tick_seq": tick_seq,
        "tick_ts": tick_ts,
        "wtg_version": env!("CARGO_PKG_VERSION"),
        "driver": fact_map_to_value(&stats.driver),
        "devices": stats.devices.iter().map(device_to_value).collect::<Vec<_>>(),
    })
}

fn device_to_value(device: &std::collections::BTreeMap<String, NvmlNode>) -> Value {
    let mut object = Map::new();
    for (key, node) in device {
        object.insert(key.clone(), node_to_value(node));
    }
    Value::Object(object)
}

fn node_to_value(node: &NvmlNode) -> Value {
    match node {
        NvmlNode::Fact(fact) => fact_to_value(fact),
        NvmlNode::Group(group) => {
            let mut object = Map::new();
            for (key, child) in group {
                object.insert(key.clone(), node_to_value(child));
            }
            Value::Object(object)
        }
    }
}

fn fact_map_to_value(facts: &std::collections::BTreeMap<String, NvmlFact>) -> Value {
    let mut object = Map::new();
    for (key, fact) in facts {
        object.insert(key.clone(), fact_to_value(fact));
    }
    Value::Object(object)
}

fn fact_to_value(fact: &NvmlFact) -> Value {
    let mut object = Map::new();
    object.insert("source_api".to_string(), json!(fact.source_api));
    object.insert("state".to_string(), json!(fact.state.as_str()));
    object.insert(
        "raw".to_string(),
        fact.raw
            .as_ref()
            .map(fact_value_to_value)
            .unwrap_or(Value::Null),
    );
    if let Some(unit) = fact.unit {
        object.insert("unit".to_string(), json!(unit));
    }
    if let Some(error_message) = fact.error_message.as_ref() {
        object.insert("error_message".to_string(), json!(error_message));
    }
    Value::Object(object)
}

fn fact_value_to_value(value: &NvmlFactValue) -> Value {
    match value {
        NvmlFactValue::String(value) => json!(value),
        NvmlFactValue::U32(value) => json!(value),
        NvmlFactValue::U64(value) => json!(value),
        NvmlFactValue::Bool(value) => json!(value),
        NvmlFactValue::Object(entries) => {
            let mut object = Map::new();
            for (key, value) in entries {
                object.insert(key.clone(), fact_value_to_value(value));
            }
            Value::Object(object)
        }
    }
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}
