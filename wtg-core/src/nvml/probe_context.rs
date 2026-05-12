// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Adam Hooper

use super::{init_context, NvmlContext};
use nvml_wrapper::enum_wrappers::device::PerformanceState;

#[derive(Debug, Clone)]
pub struct GpuProbeContext {
    pub driver_version: String,
    pub cuda_driver_version: String,
    pub compute_mode: String,
    pub perf_state: String,
    pub pci_bus_id: String,
}

impl GpuProbeContext {
    pub fn unavailable() -> Self {
        Self {
            driver_version: unavailable(),
            cuda_driver_version: unavailable(),
            compute_mode: unavailable(),
            perf_state: unavailable(),
            pci_bus_id: unavailable(),
        }
    }
}

/// Convenience helper for callers that do not already own an NVML context.
///
/// Active probe paths should prefer `query_probe_context_for_gpu_with_ctx`
/// so the caller can reuse an existing NVML context.
#[allow(dead_code)]
pub fn query_probe_context_for_gpu(gpu_index: u32) -> GpuProbeContext {
    match init_context() {
        Ok(ctx) => query_probe_context_for_gpu_with_ctx(&ctx, gpu_index),
        Err(_) => GpuProbeContext::unavailable(),
    }
}

pub fn query_probe_context_for_gpu_with_ctx(ctx: &NvmlContext, gpu_index: u32) -> GpuProbeContext {
    let driver_version = ctx
        .nvml
        .sys_driver_version()
        .unwrap_or_else(|_| unavailable());
    let cuda_driver_version = ctx
        .nvml
        .sys_cuda_driver_version()
        .map(|version| version.to_string())
        .unwrap_or_else(|_| unavailable());

    let (compute_mode, perf_state, pci_bus_id) = match ctx.nvml.device_by_index(gpu_index) {
        Ok(dev) => {
            let compute_mode = dev
                .compute_mode()
                .map(|mode| format!("{mode:?}"))
                .unwrap_or_else(|_| unavailable());
            let perf_state = dev
                .performance_state()
                .map(|state| format_perf_state(state).to_string())
                .unwrap_or_else(|_| unavailable());
            let pci_bus_id = dev
                .pci_info()
                .map(|pci| pci.bus_id)
                .unwrap_or_else(|_| unavailable());

            (compute_mode, perf_state, pci_bus_id)
        }
        Err(_) => (unavailable(), unavailable(), unavailable()),
    };

    GpuProbeContext {
        driver_version,
        cuda_driver_version,
        compute_mode,
        perf_state,
        pci_bus_id,
    }
}

fn format_perf_state(state: PerformanceState) -> &'static str {
    match state {
        PerformanceState::Zero => "P0",
        PerformanceState::One => "P1",
        PerformanceState::Two => "P2",
        PerformanceState::Three => "P3",
        PerformanceState::Four => "P4",
        PerformanceState::Five => "P5",
        PerformanceState::Six => "P6",
        PerformanceState::Seven => "P7",
        PerformanceState::Eight => "P8",
        PerformanceState::Nine => "P9",
        PerformanceState::Ten => "P10",
        PerformanceState::Eleven => "P11",
        PerformanceState::Twelve => "P12",
        PerformanceState::Thirteen => "P13",
        PerformanceState::Fourteen => "P14",
        PerformanceState::Fifteen => "P15",
        PerformanceState::Unknown => "Unknown",
    }
}

fn unavailable() -> String {
    "N/A".to_string()
}
