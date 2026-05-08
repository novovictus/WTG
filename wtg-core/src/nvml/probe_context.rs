// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Adam Hooper

use super::{init_context, NvmlContext};

#[derive(Debug, Clone)]
pub struct GpuProbeContext {
    pub driver_version: String,
    pub cuda_driver_version: String,
    pub compute_mode: String,
    pub pci_bus_id: String,
}

impl GpuProbeContext {
    pub fn unavailable() -> Self {
        Self {
            driver_version: unavailable(),
            cuda_driver_version: unavailable(),
            compute_mode: unavailable(),
            pci_bus_id: unavailable(),
        }
    }
}

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

    let (compute_mode, pci_bus_id) = match ctx.nvml.device_by_index(gpu_index) {
        Ok(dev) => {
            let compute_mode = dev
                .compute_mode()
                .map(|mode| format!("{mode:?}"))
                .unwrap_or_else(|_| unavailable());
            let pci_bus_id = dev
                .pci_info()
                .map(|pci| pci.bus_id)
                .unwrap_or_else(|_| unavailable());

            (compute_mode, pci_bus_id)
        }
        Err(_) => (unavailable(), unavailable()),
    };

    GpuProbeContext {
        driver_version,
        cuda_driver_version,
        compute_mode,
        pci_bus_id,
    }
}

fn unavailable() -> String {
    "N/A".to_string()
}
