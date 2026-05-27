// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Adam Hooper

use wtg_core::nvml::{
    field_values::{FieldQueryStatus, NvmlFieldValue},
    probe_context::GpuProbeContext,
    GpuSnapshot,
};

pub(crate) fn format_probe_fields_snapshot(s: &GpuSnapshot, context: &GpuProbeContext) -> String {
    format!(
        concat!(
            "[probe-fields] gpu={}\n",
            "wtg.version: {}\n",
            "gpu.index: {}\n",
            "gpu.name: {}\n",
            "gpu.uuid: {}\n",
            "driver.version: {}\n",
            "cuda.driver_version: {}\n",
            "gpu.compute_mode: {}\n",
            "gpu.perf_state: {}\n",
            "gpu.pci.bus_id: {}\n",
            "util.gpu_pct: {}\n",
            "util.mem_controller_pct: {}\n",
            "vram.used_mib: {}\n",
            "vram.total_mib: {}\n",
            "\n"
        ),
        s.index,
        env!("CARGO_PKG_VERSION"),
        s.index,
        s.name,
        s.uuid,
        context.driver_version,
        context.cuda_driver_version,
        context.compute_mode,
        context.perf_state,
        context.pci_bus_id,
        s.gpu_util_pct,
        s.mem_util_pct,
        crate::bytes_to_mib(s.mem_used_bytes),
        crate::bytes_to_mib(s.mem_total_bytes)
    )
}

pub(crate) fn format_field_value(gpu_index: u32, field: &NvmlFieldValue) -> String {
    let (query, status) = field_query_and_status(field);

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

pub(crate) fn format_probe_fields_csv_header() -> &'static str {
    "wtg_version,gpu_index,gpu_name,gpu_uuid,driver_version,cuda_driver_version,gpu_compute_mode,gpu_perf_state,gpu_pci_bus_id,util_gpu_pct,util_mem_controller_pct,vram_used_mib,vram_total_mib,field_id,field_query,field_status,field_type,field_value"
}

pub(crate) fn format_probe_fields_csv_row(
    s: &GpuSnapshot,
    context: &GpuProbeContext,
    field: &NvmlFieldValue,
) -> String {
    let (query, status) = field_query_and_status(field);

    crate::sink::format_csv_row(&[
        env!("CARGO_PKG_VERSION").to_string(),
        s.index.to_string(),
        s.name.clone(),
        s.uuid.clone(),
        context.driver_version.clone(),
        context.cuda_driver_version.clone(),
        context.compute_mode.clone(),
        context.perf_state.clone(),
        context.pci_bus_id.clone(),
        s.gpu_util_pct.to_string(),
        s.mem_util_pct.to_string(),
        crate::bytes_to_mib(s.mem_used_bytes).to_string(),
        crate::bytes_to_mib(s.mem_total_bytes).to_string(),
        field.field_id.to_string(),
        query.to_string(),
        status.to_string(),
        field.value_type.clone(),
        field.value.clone(),
    ])
}

fn field_query_and_status(field: &NvmlFieldValue) -> (&str, &str) {
    match &field.query {
        FieldQueryStatus::Ok => ("ok", "Ok"),
        FieldQueryStatus::CallError(e) => ("call_error", e.as_str()),
        FieldQueryStatus::FieldError(e) => ("field_error", e.as_str()),
    }
}
