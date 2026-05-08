// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Adam Hooper

pub(crate) struct ProbeRecord {
    gpu_index: u32,
    gpu_name: String,
    gpu_uuid: String,
    temp_c: Option<u32>,
    util_gpu_pct: u32,
    util_mem_controller_pct: u32,
    vram_used_mib: u64,
    vram_total_mib: u64,
    power_w: Option<f32>,
    power_limit_w: Option<f32>,
}

impl ProbeRecord {
    pub(crate) fn from_snapshot(s: &wtg_core::nvml::GpuSnapshot) -> Self {
        Self {
            gpu_index: s.index,
            gpu_name: s.name.clone(),
            gpu_uuid: s.uuid.clone(),
            temp_c: s.temp_c,
            util_gpu_pct: s.gpu_util_pct,
            util_mem_controller_pct: s.mem_util_pct,
            vram_used_mib: crate::bytes_to_mib(s.mem_used_bytes),
            vram_total_mib: crate::bytes_to_mib(s.mem_total_bytes),
            power_w: crate::mw_to_w(s.power_mw),
            power_limit_w: crate::mw_to_w(s.power_limit_mw),
        }
    }
}

pub(crate) fn format_probe_record(record: &ProbeRecord) -> String {
    let temp_c = record
        .temp_c
        .map(|t| t.to_string())
        .unwrap_or_else(|| "N/A".to_string());
    let power_w = record
        .power_w
        .map(|w| format!("{w:.1}"))
        .unwrap_or_else(|| "N/A".to_string());
    let power_limit_w = record
        .power_limit_w
        .map(|w| format!("{w:.1}"))
        .unwrap_or_else(|| "N/A".to_string());

    format!(
        concat!(
            "[probe] gpu={}\n",
            "gpu.index: {}\n",
            "gpu.name: {}\n",
            "gpu.uuid: {}\n",
            "temp.c: {}\n",
            "util.gpu_pct: {}\n",
            "util.mem_controller_pct: {}\n",
            "vram.used_mib: {}\n",
            "vram.total_mib: {}\n",
            "power.w: {}\n",
            "power.limit_w: {}\n",
            "\n"
        ),
        record.gpu_index,
        record.gpu_index,
        record.gpu_name,
        record.gpu_uuid,
        temp_c,
        record.util_gpu_pct,
        record.util_mem_controller_pct,
        record.vram_used_mib,
        record.vram_total_mib,
        power_w,
        power_limit_w
    )
}

fn csv_escape_field(s: &str) -> String {
    if !s.chars().any(|c| matches!(c, ',' | '"' | '\n' | '\r')) {
        return s.to_string();
    }

    let mut escaped = String::with_capacity(s.len() + 2);
    escaped.push('"');
    for c in s.chars() {
        if c == '"' {
            escaped.push('"');
        }
        escaped.push(c);
    }
    escaped.push('"');
    escaped
}

pub(crate) fn format_probe_csv_header() -> &'static str {
    "gpu_index,gpu_name,gpu_uuid,temp_c,util_gpu_pct,util_mem_controller_pct,vram_used_mib,vram_total_mib,power_w,power_limit_w"
}

pub(crate) fn format_probe_csv_row(record: &ProbeRecord) -> String {
    let temp_c = record
        .temp_c
        .map(|t| t.to_string())
        .unwrap_or_else(|| "N/A".to_string());
    let power_w = record
        .power_w
        .map(|w| format!("{w:.1}"))
        .unwrap_or_else(|| "N/A".to_string());
    let power_limit_w = record
        .power_limit_w
        .map(|w| format!("{w:.1}"))
        .unwrap_or_else(|| "N/A".to_string());

    [
        record.gpu_index.to_string(),
        record.gpu_name.clone(),
        record.gpu_uuid.clone(),
        temp_c,
        record.util_gpu_pct.to_string(),
        record.util_mem_controller_pct.to_string(),
        record.vram_used_mib.to_string(),
        record.vram_total_mib.to_string(),
        power_w,
        power_limit_w,
    ]
    .iter()
    .map(|field| csv_escape_field(field))
    .collect::<Vec<_>>()
    .join(",")
}
