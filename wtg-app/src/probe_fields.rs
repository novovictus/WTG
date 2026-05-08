// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Adam Hooper

use wtg_core::nvml::{
    field_values::{FieldQueryStatus, NvmlFieldValue},
    GpuSnapshot,
};

pub(crate) fn format_probe_fields_snapshot(s: &GpuSnapshot) -> String {
    format!(
        concat!(
            "[probe-fields] gpu={}\n",
            "gpu.index: {}\n",
            "gpu.name: {}\n",
            "gpu.uuid: {}\n",
            "util.gpu_pct: {}\n",
            "util.mem_controller_pct: {}\n",
            "vram.used_mib: {}\n",
            "vram.total_mib: {}\n",
            "\n"
        ),
        s.index,
        s.index,
        s.name,
        s.uuid,
        s.gpu_util_pct,
        s.mem_util_pct,
        crate::bytes_to_mib(s.mem_used_bytes),
        crate::bytes_to_mib(s.mem_total_bytes)
    )
}

pub(crate) fn format_field_value(gpu_index: u32, field: &NvmlFieldValue) -> String {
    let (query, status) = match &field.query {
        FieldQueryStatus::Ok => ("ok", "Ok"),
        FieldQueryStatus::CallError(e) => ("call_error", e.as_str()),
        FieldQueryStatus::FieldError(e) => ("field_error", e.as_str()),
    };

    format!(
        concat!(
            "[field] gpu={} field.id={}\n",
            "field.query: {}\n",
            "field.status: {}\n",
            "field.type: {}\n",
            "field.value: {}\n",
            "\n"
        ),
        gpu_index, field.field_id, query, status, field.value_type, field.value
    )
}
