// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Adam Hooper

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use wtg_core::nvml::{probe_context::GpuProbeContext, GpuSnapshot};

pub(crate) fn format_nvml_provenance_stats_pretty(
    snapshots: &[GpuSnapshot],
    contexts: &[GpuProbeContext],
    tick_seq: u64,
    tick_ts: &str,
) -> String {
    let payload = build_nvml_provenance_stats(snapshots, contexts, tick_seq, tick_ts);
    serde_json::to_string_pretty(&payload)
        .expect("NVML provenance stats JSON serialization should succeed")
}

pub(crate) fn format_nvml_provenance_stats_jsonl(
    snapshots: &[GpuSnapshot],
    contexts: &[GpuProbeContext],
    tick_seq: u64,
    tick_ts: &str,
) -> String {
    let payload = build_nvml_provenance_stats(snapshots, contexts, tick_seq, tick_ts);
    serde_json::to_string(&payload)
        .expect("NVML provenance stats JSONL serialization should succeed")
}

fn build_nvml_provenance_stats(
    snapshots: &[GpuSnapshot],
    contexts: &[GpuProbeContext],
    tick_seq: u64,
    tick_ts: &str,
) -> Value {
    let driver_version = contexts
        .first()
        .map(|context| context.driver_version.as_str())
        .unwrap_or("N/A");
    let cuda_driver_version = contexts
        .first()
        .map(|context| context.cuda_driver_version.as_str())
        .unwrap_or("N/A");
    let devices = snapshots
        .iter()
        .zip(contexts.iter())
        .map(|(snapshot, context)| format_device(snapshot, context))
        .collect::<Vec<_>>();

    json!({
        "schema": "wtg.nvml.stats.v1",
        "provider": "nvidia.nvml",
        "provider_authority": "NVIDIA NVML",
        "telemetry_class": "provider_truth",
        "timestamp_unix_ms": now_unix_ms(),
        "tick_seq": tick_seq,
        "tick_ts": tick_ts,
        "wtg_version": env!("CARGO_PKG_VERSION"),
        "driver": {
            "nvml.driver.version": string_fact(
                "nvmlSystemGetDriverVersion",
                driver_version,
            ),
            "nvml.cuda.driver_version": string_fact(
                "nvmlSystemGetCudaDriverVersion",
                cuda_driver_version,
            ),
        },
        "devices": devices,
    })
}

fn format_device(snapshot: &GpuSnapshot, context: &GpuProbeContext) -> Value {
    json!({
        "nvml.device.index": number_fact("nvmlDeviceGetHandleByIndex", snapshot.index),
        "nvml.device.name": string_fact("nvmlDeviceGetName", &snapshot.name),
        "nvml.device.uuid": string_fact("nvmlDeviceGetUUID", &snapshot.uuid),
        "nvml.device.pci.bus_id": string_fact("nvmlDeviceGetPciInfo", &context.pci_bus_id),
        "nvml.device.compute_mode": string_fact("nvmlDeviceGetComputeMode", &context.compute_mode),
        "nvml.device.performance_state": string_fact(
            "nvmlDeviceGetPerformanceState",
            &context.perf_state,
        ),
        "nvml.memory.used_bytes": bytes_fact("nvmlDeviceGetMemoryInfo", snapshot.mem_used_bytes),
        "nvml.memory.free_bytes": bytes_fact(
            "nvmlDeviceGetMemoryInfo",
            snapshot.mem_total_bytes.saturating_sub(snapshot.mem_used_bytes),
        ),
        "nvml.memory.total_bytes": bytes_fact("nvmlDeviceGetMemoryInfo", snapshot.mem_total_bytes),
        "nvml.utilization.gpu_pct": unit_number_fact(
            "nvmlDeviceGetUtilizationRates",
            "percent",
            snapshot.gpu_util_pct,
        ),
        "nvml.utilization.memory_controller_pct": unit_number_fact(
            "nvmlDeviceGetUtilizationRates",
            "percent",
            snapshot.mem_util_pct,
        ),
        "nvml.temperature.gpu_c": optional_number_fact(
            "nvmlDeviceGetTemperature",
            "celsius",
            snapshot.temp_c.map(|value| value as u64),
            None,
        ),
        "nvml.power.draw_mw": optional_number_fact(
            "nvmlDeviceGetPowerUsage",
            "milliwatts",
            snapshot.power_mw.map(|value| value as u64),
            snapshot.power_mw.map(|value| json!({ "watts": mw_to_w(value) })),
        ),
        "nvml.power.enforced_limit_mw": optional_number_fact(
            "nvmlDeviceGetEnforcedPowerLimit",
            "milliwatts",
            snapshot.power_limit_mw.map(|value| value as u64),
            snapshot
                .power_limit_mw
                .map(|value| json!({ "watts": mw_to_w(value) })),
        ),
    })
}

fn number_fact(source_api: &str, raw: u32) -> Value {
    json!({
        "source_api": source_api,
        "state": "ok",
        "raw": raw,
    })
}

fn unit_number_fact(source_api: &str, unit: &str, raw: u32) -> Value {
    json!({
        "source_api": source_api,
        "state": "ok",
        "unit": unit,
        "raw": raw,
    })
}

fn bytes_fact(source_api: &str, raw: u64) -> Value {
    json!({
        "source_api": source_api,
        "state": "ok",
        "unit": "bytes",
        "raw": raw,
        "normalized": {
            "mib": bytes_to_mib(raw),
        },
    })
}

fn optional_number_fact(
    source_api: &str,
    unit: &str,
    raw: Option<u64>,
    normalized: Option<Value>,
) -> Value {
    json!({
        "source_api": source_api,
        "state": if raw.is_some() { "ok" } else { "unsupported" },
        "unit": unit,
        "raw": raw,
        "normalized": normalized,
    })
}

fn string_fact(source_api: &str, raw: &str) -> Value {
    json!({
        "source_api": source_api,
        "state": if raw == "N/A" { "unsupported" } else { "ok" },
        "raw": raw,
    })
}

fn bytes_to_mib(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
}

fn mw_to_w(mw: u32) -> f64 {
    mw as f64 / 1000.0
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}
