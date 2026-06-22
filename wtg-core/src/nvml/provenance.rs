// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Adam Hooper

use std::collections::BTreeMap;

use nvml_wrapper::{
    enum_wrappers::device::{Clock, PcieUtilCounter, PerformanceState, TemperatureSensor},
    error::NvmlError,
};

use super::{GpuSnapshot, NvmlContext};

#[derive(Debug, Clone)]
pub enum NvmlFactValue {
    String(String),
    U32(u32),
    U64(u64),
    Bool(bool),
    Object(BTreeMap<String, NvmlFactValue>),
}

#[derive(Debug, Clone)]
pub enum NvmlFactState {
    Ok,
    Unsupported,
    NotAvailable,
    PermissionDenied,
    Error,
}

impl NvmlFactState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Unsupported => "unsupported",
            Self::NotAvailable => "not_available",
            Self::PermissionDenied => "permission_denied",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone)]
pub struct NvmlFact {
    pub source_api: &'static str,
    pub state: NvmlFactState,
    pub raw: Option<NvmlFactValue>,
    pub unit: Option<&'static str>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone)]
pub enum NvmlNode {
    Fact(NvmlFact),
    Group(BTreeMap<String, NvmlNode>),
}

#[derive(Debug, Clone)]
pub struct NvmlProvenanceStats {
    pub driver: BTreeMap<String, NvmlFact>,
    pub devices: Vec<BTreeMap<String, NvmlNode>>,
}

pub fn collect_provenance_stats(
    ctx: &NvmlContext,
    snapshots: &[GpuSnapshot],
) -> NvmlProvenanceStats {
    let driver = build_driver_facts(ctx);
    let devices = snapshots
        .iter()
        .map(|snapshot| build_device_facts(ctx, snapshot.index))
        .collect();

    NvmlProvenanceStats { driver, devices }
}

fn build_driver_facts(ctx: &NvmlContext) -> BTreeMap<String, NvmlFact> {
    let mut driver = BTreeMap::new();
    driver.insert(
        "nvml.driver.version".to_string(),
        match ctx.nvml.sys_driver_version() {
            Ok(value) => ok_string_fact("nvmlSystemGetDriverVersion", value),
            Err(error) => error_fact("nvmlSystemGetDriverVersion", None, error),
        },
    );
    driver.insert(
        "nvml.cuda.driver_version".to_string(),
        match ctx.nvml.sys_cuda_driver_version() {
            Ok(value) => ok_string_fact("nvmlSystemGetCudaDriverVersion", value.to_string()),
            Err(error) => error_fact("nvmlSystemGetCudaDriverVersion", None, error),
        },
    );
    driver
}

fn build_device_facts(ctx: &NvmlContext, gpu_index: u32) -> BTreeMap<String, NvmlNode> {
    let mut device = BTreeMap::new();
    device.insert(
        "nvml.device.index".to_string(),
        NvmlNode::Fact(ok_u32_fact("nvmlDeviceGetHandleByIndex", gpu_index, None)),
    );

    match ctx.nvml.device_by_index(gpu_index) {
        Ok(dev) => {
            device.insert(
                "nvml.device.name".to_string(),
                NvmlNode::Fact(match dev.name() {
                    Ok(value) => ok_string_fact("nvmlDeviceGetName", value),
                    Err(error) => error_fact("nvmlDeviceGetName", None, error),
                }),
            );
            device.insert(
                "nvml.device.uuid".to_string(),
                NvmlNode::Fact(match dev.uuid() {
                    Ok(value) => ok_string_fact("nvmlDeviceGetUUID", value),
                    Err(error) => error_fact("nvmlDeviceGetUUID", None, error),
                }),
            );

            let (pci_bus_id_fact, pcie_group) = build_pcie_group(&dev);
            device.insert(
                "nvml.device.pci.bus_id".to_string(),
                NvmlNode::Fact(pci_bus_id_fact),
            );
            device.insert(
                "nvml.device.compute_mode".to_string(),
                NvmlNode::Fact(match dev.compute_mode() {
                    Ok(value) => ok_string_fact("nvmlDeviceGetComputeMode", format!("{value:?}")),
                    Err(error) => error_fact("nvmlDeviceGetComputeMode", None, error),
                }),
            );
            device.insert(
                "nvml.device.performance_state".to_string(),
                NvmlNode::Fact(match dev.performance_state() {
                    Ok(value) => ok_string_fact(
                        "nvmlDeviceGetPerformanceState",
                        format_perf_state(value).to_string(),
                    ),
                    Err(error) => error_fact("nvmlDeviceGetPerformanceState", None, error),
                }),
            );

            let (used_fact, free_fact, total_fact) = build_memory_facts(&dev);
            device.insert(
                "nvml.memory.used_bytes".to_string(),
                NvmlNode::Fact(used_fact),
            );
            device.insert(
                "nvml.memory.free_bytes".to_string(),
                NvmlNode::Fact(free_fact),
            );
            device.insert(
                "nvml.memory.total_bytes".to_string(),
                NvmlNode::Fact(total_fact),
            );

            let (gpu_util_fact, mem_util_fact) = build_utilization_facts(&dev);
            device.insert(
                "nvml.utilization.gpu_pct".to_string(),
                NvmlNode::Fact(gpu_util_fact),
            );
            device.insert(
                "nvml.utilization.memory_controller_pct".to_string(),
                NvmlNode::Fact(mem_util_fact),
            );

            device.insert(
                "nvml.temperature.gpu_c".to_string(),
                NvmlNode::Fact(match dev.temperature(TemperatureSensor::Gpu) {
                    Ok(value) => ok_u32_fact("nvmlDeviceGetTemperature", value, Some("celsius")),
                    Err(error) => error_fact("nvmlDeviceGetTemperature", Some("celsius"), error),
                }),
            );
            device.insert(
                "nvml.power.draw_mw".to_string(),
                NvmlNode::Fact(match dev.power_usage() {
                    Ok(value) => ok_u32_fact("nvmlDeviceGetPowerUsage", value, Some("milliwatts")),
                    Err(error) => error_fact("nvmlDeviceGetPowerUsage", Some("milliwatts"), error),
                }),
            );
            device.insert(
                "nvml.power.enforced_limit_mw".to_string(),
                NvmlNode::Fact(match dev.enforced_power_limit() {
                    Ok(value) => {
                        ok_u32_fact("nvmlDeviceGetEnforcedPowerLimit", value, Some("milliwatts"))
                    }
                    Err(error) => {
                        error_fact("nvmlDeviceGetEnforcedPowerLimit", Some("milliwatts"), error)
                    }
                }),
            );

            device.insert(
                "identity".to_string(),
                NvmlNode::Group(build_identity_group(&dev)),
            );
            device.insert("pcie".to_string(), NvmlNode::Group(pcie_group));
            device.insert(
                "clocks".to_string(),
                NvmlNode::Group(build_clocks_group(&dev)),
            );
            device.insert("bar1".to_string(), NvmlNode::Group(build_bar1_group(&dev)));
            device.insert(
                "power_management".to_string(),
                NvmlNode::Group(build_power_management_group(&dev)),
            );
            device.insert(
                "media".to_string(),
                NvmlNode::Group(build_media_group(&dev)),
            );
            device.insert(
                "cooling".to_string(),
                NvmlNode::Group(build_cooling_group(&dev)),
            );
            device.insert(
                "processes".to_string(),
                NvmlNode::Group(build_processes_group(&dev)),
            );
        }
        Err(error) => {
            let handle_error = error_fact("nvmlDeviceGetHandleByIndex", None, error);
            device.insert(
                "nvml.device.name".to_string(),
                NvmlNode::Fact(handle_error.clone()),
            );
            device.insert(
                "nvml.device.uuid".to_string(),
                NvmlNode::Fact(handle_error.clone()),
            );
            device.insert(
                "nvml.device.pci.bus_id".to_string(),
                NvmlNode::Fact(handle_error.clone()),
            );
            device.insert(
                "nvml.device.compute_mode".to_string(),
                NvmlNode::Fact(handle_error.clone()),
            );
            device.insert(
                "nvml.device.performance_state".to_string(),
                NvmlNode::Fact(handle_error.clone()),
            );
            device.insert(
                "nvml.memory.used_bytes".to_string(),
                NvmlNode::Fact(handle_error.clone()),
            );
            device.insert(
                "nvml.memory.free_bytes".to_string(),
                NvmlNode::Fact(handle_error.clone()),
            );
            device.insert(
                "nvml.memory.total_bytes".to_string(),
                NvmlNode::Fact(handle_error.clone()),
            );
            device.insert(
                "nvml.utilization.gpu_pct".to_string(),
                NvmlNode::Fact(handle_error.clone()),
            );
            device.insert(
                "nvml.utilization.memory_controller_pct".to_string(),
                NvmlNode::Fact(handle_error.clone()),
            );
            device.insert(
                "nvml.temperature.gpu_c".to_string(),
                NvmlNode::Fact(handle_error.clone()),
            );
            device.insert(
                "nvml.power.draw_mw".to_string(),
                NvmlNode::Fact(handle_error.clone()),
            );
            device.insert(
                "nvml.power.enforced_limit_mw".to_string(),
                NvmlNode::Fact(handle_error.clone()),
            );
            device.insert(
                "identity".to_string(),
                NvmlNode::Group(single_error_group(
                    "nvmlDeviceGetHandleByIndex",
                    handle_error.clone(),
                )),
            );
            device.insert(
                "pcie".to_string(),
                NvmlNode::Group(single_error_group(
                    "nvmlDeviceGetHandleByIndex",
                    handle_error.clone(),
                )),
            );
            device.insert(
                "clocks".to_string(),
                NvmlNode::Group(single_error_group(
                    "nvmlDeviceGetHandleByIndex",
                    handle_error.clone(),
                )),
            );
            device.insert(
                "bar1".to_string(),
                NvmlNode::Group(single_error_group(
                    "nvmlDeviceGetHandleByIndex",
                    handle_error.clone(),
                )),
            );
            device.insert(
                "power_management".to_string(),
                NvmlNode::Group(single_error_group(
                    "nvmlDeviceGetHandleByIndex",
                    handle_error.clone(),
                )),
            );
            device.insert(
                "media".to_string(),
                NvmlNode::Group(single_error_group(
                    "nvmlDeviceGetHandleByIndex",
                    handle_error.clone(),
                )),
            );
            device.insert(
                "cooling".to_string(),
                NvmlNode::Group(single_error_group(
                    "nvmlDeviceGetHandleByIndex",
                    handle_error.clone(),
                )),
            );
            device.insert(
                "processes".to_string(),
                NvmlNode::Group(single_error_group(
                    "nvmlDeviceGetHandleByIndex",
                    handle_error,
                )),
            );
        }
    }

    device
}

fn build_identity_group(device: &nvml_wrapper::Device<'_>) -> BTreeMap<String, NvmlNode> {
    let mut group = BTreeMap::new();
    group.insert(
        "brand".to_string(),
        NvmlNode::Fact(match device.brand() {
            Ok(value) => ok_string_fact("nvmlDeviceGetBrand", format!("{value:?}")),
            Err(error) => error_fact("nvmlDeviceGetBrand", None, error),
        }),
    );
    group.insert(
        "board_id".to_string(),
        NvmlNode::Fact(match device.board_id() {
            Ok(value) => ok_u32_fact("nvmlDeviceGetBoardId", value, None),
            Err(error) => error_fact("nvmlDeviceGetBoardId", None, error),
        }),
    );
    group.insert(
        "vbios_version".to_string(),
        NvmlNode::Fact(match device.vbios_version() {
            Ok(value) => ok_string_fact("nvmlDeviceGetVbiosVersion", value),
            Err(error) => error_fact("nvmlDeviceGetVbiosVersion", None, error),
        }),
    );
    group.insert(
        "cuda_compute_capability".to_string(),
        NvmlNode::Fact(match device.cuda_compute_capability() {
            Ok(value) => {
                let mut raw = BTreeMap::new();
                raw.insert(
                    "major".to_string(),
                    NvmlFactValue::String(value.major.to_string()),
                );
                raw.insert(
                    "minor".to_string(),
                    NvmlFactValue::String(value.minor.to_string()),
                );
                ok_object_fact("nvmlDeviceGetCudaComputeCapability", raw)
            }
            Err(error) => error_fact("nvmlDeviceGetCudaComputeCapability", None, error),
        }),
    );
    group.insert(
        "cuda_cores".to_string(),
        NvmlNode::Fact(match device.num_cores() {
            Ok(value) => ok_u32_fact("nvmlDeviceGetNumGpuCores", value, None),
            Err(error) => error_fact("nvmlDeviceGetNumGpuCores", None, error),
        }),
    );
    group
}

fn build_pcie_group(device: &nvml_wrapper::Device<'_>) -> (NvmlFact, BTreeMap<String, NvmlNode>) {
    let pci_info = device.pci_info();
    let pci_bus_id_fact = match &pci_info {
        Ok(value) => ok_string_fact("nvmlDeviceGetPciInfo", value.bus_id.clone()),
        Err(error) => error_fact_ref("nvmlDeviceGetPciInfo", None, error),
    };

    let mut group = BTreeMap::new();
    match pci_info {
        Ok(value) => {
            group.insert(
                "domain".to_string(),
                NvmlNode::Fact(ok_u32_fact("nvmlDeviceGetPciInfo", value.domain, None)),
            );
            group.insert(
                "bus".to_string(),
                NvmlNode::Fact(ok_u32_fact("nvmlDeviceGetPciInfo", value.bus, None)),
            );
            group.insert(
                "device".to_string(),
                NvmlNode::Fact(ok_u32_fact("nvmlDeviceGetPciInfo", value.device, None)),
            );
            group.insert(
                "pci_device_id".to_string(),
                NvmlNode::Fact(ok_u32_fact(
                    "nvmlDeviceGetPciInfo",
                    value.pci_device_id,
                    None,
                )),
            );
            group.insert(
                "pci_sub_system_id".to_string(),
                NvmlNode::Fact(match value.pci_sub_system_id {
                    Some(id) => ok_u32_fact("nvmlDeviceGetPciInfo", id, None),
                    None => not_available_fact("nvmlDeviceGetPciInfo", None),
                }),
            );
        }
        Err(error) => {
            group.insert(
                "domain".to_string(),
                NvmlNode::Fact(error_fact_ref("nvmlDeviceGetPciInfo", None, &error)),
            );
            group.insert(
                "bus".to_string(),
                NvmlNode::Fact(error_fact_ref("nvmlDeviceGetPciInfo", None, &error)),
            );
            group.insert(
                "device".to_string(),
                NvmlNode::Fact(error_fact_ref("nvmlDeviceGetPciInfo", None, &error)),
            );
            group.insert(
                "pci_device_id".to_string(),
                NvmlNode::Fact(error_fact_ref("nvmlDeviceGetPciInfo", None, &error)),
            );
            group.insert(
                "pci_sub_system_id".to_string(),
                NvmlNode::Fact(error_fact("nvmlDeviceGetPciInfo", None, error)),
            );
        }
    }

    group.insert(
        "current_link_gen".to_string(),
        NvmlNode::Fact(match device.current_pcie_link_gen() {
            Ok(value) => ok_u32_fact("nvmlDeviceGetCurrPcieLinkGeneration", value, None),
            Err(error) => error_fact("nvmlDeviceGetCurrPcieLinkGeneration", None, error),
        }),
    );
    group.insert(
        "current_link_width".to_string(),
        NvmlNode::Fact(match device.current_pcie_link_width() {
            Ok(value) => ok_u32_fact("nvmlDeviceGetCurrPcieLinkWidth", value, None),
            Err(error) => error_fact("nvmlDeviceGetCurrPcieLinkWidth", None, error),
        }),
    );
    group.insert(
        "max_link_gen".to_string(),
        NvmlNode::Fact(match device.max_pcie_link_gen() {
            Ok(value) => ok_u32_fact("nvmlDeviceGetMaxPcieLinkGeneration", value, None),
            Err(error) => error_fact("nvmlDeviceGetMaxPcieLinkGeneration", None, error),
        }),
    );
    group.insert(
        "max_link_width".to_string(),
        NvmlNode::Fact(match device.max_pcie_link_width() {
            Ok(value) => ok_u32_fact("nvmlDeviceGetMaxPcieLinkWidth", value, None),
            Err(error) => error_fact("nvmlDeviceGetMaxPcieLinkWidth", None, error),
        }),
    );
    group.insert(
        "throughput_tx_kb_per_s".to_string(),
        NvmlNode::Fact(match device.pcie_throughput(PcieUtilCounter::Send) {
            Ok(value) => ok_u32_fact("nvmlDeviceGetPcieThroughput", value, Some("kb_per_s")),
            Err(error) => error_fact("nvmlDeviceGetPcieThroughput", Some("kb_per_s"), error),
        }),
    );
    group.insert(
        "throughput_rx_kb_per_s".to_string(),
        NvmlNode::Fact(match device.pcie_throughput(PcieUtilCounter::Receive) {
            Ok(value) => ok_u32_fact("nvmlDeviceGetPcieThroughput", value, Some("kb_per_s")),
            Err(error) => error_fact("nvmlDeviceGetPcieThroughput", Some("kb_per_s"), error),
        }),
    );

    (pci_bus_id_fact, group)
}

fn build_clocks_group(device: &nvml_wrapper::Device<'_>) -> BTreeMap<String, NvmlNode> {
    let mut group = BTreeMap::new();
    group.insert(
        "graphics".to_string(),
        NvmlNode::Group(build_clock_surface(device, Clock::Graphics)),
    );
    group.insert(
        "sm".to_string(),
        NvmlNode::Group(build_clock_surface(device, Clock::SM)),
    );
    group.insert(
        "memory".to_string(),
        NvmlNode::Group(build_clock_surface(device, Clock::Memory)),
    );
    group.insert(
        "video".to_string(),
        NvmlNode::Group(build_clock_surface(device, Clock::Video)),
    );
    group.insert(
        "auto_boosted_clocks".to_string(),
        NvmlNode::Fact(match device.auto_boosted_clocks_enabled() {
            Ok(value) => {
                let mut raw = BTreeMap::new();
                raw.insert("enabled".to_string(), NvmlFactValue::Bool(value.is_enabled));
                raw.insert(
                    "default_enabled".to_string(),
                    NvmlFactValue::Bool(value.is_enabled_default),
                );
                ok_object_fact("nvmlDeviceGetAutoBoostedClocksEnabled", raw)
            }
            Err(error) => error_fact("nvmlDeviceGetAutoBoostedClocksEnabled", None, error),
        }),
    );
    group
}

fn build_clock_surface(
    device: &nvml_wrapper::Device<'_>,
    clock: Clock,
) -> BTreeMap<String, NvmlNode> {
    let mut surface = BTreeMap::new();
    surface.insert(
        "current_mhz".to_string(),
        NvmlNode::Fact(match device.clock_info(clock.clone()) {
            Ok(value) => ok_u32_fact("nvmlDeviceGetClockInfo", value, Some("mhz")),
            Err(error) => error_fact("nvmlDeviceGetClockInfo", Some("mhz"), error),
        }),
    );
    surface.insert(
        "max_mhz".to_string(),
        NvmlNode::Fact(match device.max_clock_info(clock.clone()) {
            Ok(value) => ok_u32_fact("nvmlDeviceGetMaxClockInfo", value, Some("mhz")),
            Err(error) => error_fact("nvmlDeviceGetMaxClockInfo", Some("mhz"), error),
        }),
    );
    surface.insert(
        "applications_mhz".to_string(),
        NvmlNode::Fact(match device.applications_clock(clock) {
            Ok(value) => ok_u32_fact("nvmlDeviceGetApplicationsClock", value, Some("mhz")),
            Err(error) => error_fact("nvmlDeviceGetApplicationsClock", Some("mhz"), error),
        }),
    );
    surface
}

fn build_bar1_group(device: &nvml_wrapper::Device<'_>) -> BTreeMap<String, NvmlNode> {
    let mut group = BTreeMap::new();
    match device.bar1_memory_info() {
        Ok(value) => {
            group.insert(
                "used_bytes".to_string(),
                NvmlNode::Fact(ok_u64_fact(
                    "nvmlDeviceGetBAR1MemoryInfo",
                    value.used,
                    Some("bytes"),
                )),
            );
            group.insert(
                "free_bytes".to_string(),
                NvmlNode::Fact(ok_u64_fact(
                    "nvmlDeviceGetBAR1MemoryInfo",
                    value.free,
                    Some("bytes"),
                )),
            );
            group.insert(
                "total_bytes".to_string(),
                NvmlNode::Fact(ok_u64_fact(
                    "nvmlDeviceGetBAR1MemoryInfo",
                    value.total,
                    Some("bytes"),
                )),
            );
        }
        Err(error) => {
            group.insert(
                "used_bytes".to_string(),
                NvmlNode::Fact(error_fact_ref(
                    "nvmlDeviceGetBAR1MemoryInfo",
                    Some("bytes"),
                    &error,
                )),
            );
            group.insert(
                "free_bytes".to_string(),
                NvmlNode::Fact(error_fact_ref(
                    "nvmlDeviceGetBAR1MemoryInfo",
                    Some("bytes"),
                    &error,
                )),
            );
            group.insert(
                "total_bytes".to_string(),
                NvmlNode::Fact(error_fact(
                    "nvmlDeviceGetBAR1MemoryInfo",
                    Some("bytes"),
                    error,
                )),
            );
        }
    }
    group
}

fn build_power_management_group(device: &nvml_wrapper::Device<'_>) -> BTreeMap<String, NvmlNode> {
    let mut group = BTreeMap::new();
    group.insert(
        "limit_mw".to_string(),
        NvmlNode::Fact(match device.power_management_limit() {
            Ok(value) => ok_u32_fact(
                "nvmlDeviceGetPowerManagementLimit",
                value,
                Some("milliwatts"),
            ),
            Err(error) => error_fact(
                "nvmlDeviceGetPowerManagementLimit",
                Some("milliwatts"),
                error,
            ),
        }),
    );
    group.insert(
        "default_limit_mw".to_string(),
        NvmlNode::Fact(match device.power_management_limit_default() {
            Ok(value) => ok_u32_fact(
                "nvmlDeviceGetPowerManagementDefaultLimit",
                value,
                Some("milliwatts"),
            ),
            Err(error) => error_fact(
                "nvmlDeviceGetPowerManagementDefaultLimit",
                Some("milliwatts"),
                error,
            ),
        }),
    );
    match device.power_management_limit_constraints() {
        Ok(value) => {
            group.insert(
                "limit_constraints_min_mw".to_string(),
                NvmlNode::Fact(ok_u32_fact(
                    "nvmlDeviceGetPowerManagementLimitConstraints",
                    value.min_limit,
                    Some("milliwatts"),
                )),
            );
            group.insert(
                "limit_constraints_max_mw".to_string(),
                NvmlNode::Fact(ok_u32_fact(
                    "nvmlDeviceGetPowerManagementLimitConstraints",
                    value.max_limit,
                    Some("milliwatts"),
                )),
            );
        }
        Err(error) => {
            group.insert(
                "limit_constraints_min_mw".to_string(),
                NvmlNode::Fact(error_fact_ref(
                    "nvmlDeviceGetPowerManagementLimitConstraints",
                    Some("milliwatts"),
                    &error,
                )),
            );
            group.insert(
                "limit_constraints_max_mw".to_string(),
                NvmlNode::Fact(error_fact(
                    "nvmlDeviceGetPowerManagementLimitConstraints",
                    Some("milliwatts"),
                    error,
                )),
            );
        }
    }
    group.insert(
        "algorithm_active".to_string(),
        NvmlNode::Fact(match device.is_power_management_algo_active() {
            Ok(value) => ok_bool_fact("nvmlDeviceGetPowerManagementMode", value),
            Err(error) => error_fact("nvmlDeviceGetPowerManagementMode", None, error),
        }),
    );
    group.insert(
        "power_source".to_string(),
        NvmlNode::Fact(match device.power_source() {
            Ok(value) => ok_string_fact("nvmlDeviceGetPowerSource", format!("{value:?}")),
            Err(error) => error_fact("nvmlDeviceGetPowerSource", None, error),
        }),
    );
    group
}

fn build_media_group(device: &nvml_wrapper::Device<'_>) -> BTreeMap<String, NvmlNode> {
    let mut group = BTreeMap::new();
    group.insert(
        "encoder".to_string(),
        NvmlNode::Fact(match device.encoder_utilization() {
            Ok(value) => ok_u32_fact(
                "nvmlDeviceGetEncoderUtilization",
                value.utilization,
                Some("percent"),
            ),
            Err(error) => error_fact("nvmlDeviceGetEncoderUtilization", Some("percent"), error),
        }),
    );
    group.insert(
        "decoder".to_string(),
        NvmlNode::Fact(match device.decoder_utilization() {
            Ok(value) => ok_u32_fact(
                "nvmlDeviceGetDecoderUtilization",
                value.utilization,
                Some("percent"),
            ),
            Err(error) => error_fact("nvmlDeviceGetDecoderUtilization", Some("percent"), error),
        }),
    );
    group
}

fn build_cooling_group(device: &nvml_wrapper::Device<'_>) -> BTreeMap<String, NvmlNode> {
    let mut group = BTreeMap::new();
    match device.num_fans() {
        Ok(num_fans) => {
            group.insert(
                "num_fans".to_string(),
                NvmlNode::Fact(ok_u32_fact("nvmlDeviceGetNumFans", num_fans, None)),
            );
            let mut fan_speeds = BTreeMap::new();
            for fan_idx in 0..num_fans {
                fan_speeds.insert(
                    format!("fan_{fan_idx}_speed_pct"),
                    NvmlNode::Fact(match device.fan_speed(fan_idx) {
                        Ok(value) => {
                            ok_u32_fact("nvmlDeviceGetFanSpeed_v2", value, Some("percent"))
                        }
                        Err(error) => {
                            error_fact("nvmlDeviceGetFanSpeed_v2", Some("percent"), error)
                        }
                    }),
                );
            }
            group.insert("fan_speeds".to_string(), NvmlNode::Group(fan_speeds));
        }
        Err(error) => {
            group.insert(
                "num_fans".to_string(),
                NvmlNode::Fact(error_fact_ref("nvmlDeviceGetNumFans", None, &error)),
            );
            let mut fan_speeds = BTreeMap::new();
            fan_speeds.insert(
                "fan_0_speed_pct".to_string(),
                NvmlNode::Fact(error_fact(
                    "nvmlDeviceGetFanSpeed_v2",
                    Some("percent"),
                    error,
                )),
            );
            group.insert("fan_speeds".to_string(), NvmlNode::Group(fan_speeds));
        }
    }
    group
}

fn build_processes_group(device: &nvml_wrapper::Device<'_>) -> BTreeMap<String, NvmlNode> {
    let mut group = BTreeMap::new();
    group.insert(
        "running_compute_processes_count".to_string(),
        NvmlNode::Fact(match device.running_compute_processes_count() {
            Ok(value) => ok_u32_fact("nvmlDeviceGetComputeRunningProcesses_v3", value, None),
            Err(error) => error_fact("nvmlDeviceGetComputeRunningProcesses_v3", None, error),
        }),
    );
    group.insert(
        "running_graphics_processes_count".to_string(),
        NvmlNode::Fact(match device.running_graphics_processes_count() {
            Ok(value) => ok_u32_fact("nvmlDeviceGetGraphicsRunningProcesses_v3", value, None),
            Err(error) => error_fact("nvmlDeviceGetGraphicsRunningProcesses_v3", None, error),
        }),
    );
    group
}

fn build_memory_facts(device: &nvml_wrapper::Device<'_>) -> (NvmlFact, NvmlFact, NvmlFact) {
    match device.memory_info() {
        Ok(value) => (
            ok_u64_fact("nvmlDeviceGetMemoryInfo", value.used, Some("bytes")),
            ok_u64_fact("nvmlDeviceGetMemoryInfo", value.free, Some("bytes")),
            ok_u64_fact("nvmlDeviceGetMemoryInfo", value.total, Some("bytes")),
        ),
        Err(error) => (
            error_fact_ref("nvmlDeviceGetMemoryInfo", Some("bytes"), &error),
            error_fact_ref("nvmlDeviceGetMemoryInfo", Some("bytes"), &error),
            error_fact("nvmlDeviceGetMemoryInfo", Some("bytes"), error),
        ),
    }
}

fn build_utilization_facts(device: &nvml_wrapper::Device<'_>) -> (NvmlFact, NvmlFact) {
    match device.utilization_rates() {
        Ok(value) => (
            ok_u32_fact("nvmlDeviceGetUtilizationRates", value.gpu, Some("percent")),
            ok_u32_fact(
                "nvmlDeviceGetUtilizationRates",
                value.memory,
                Some("percent"),
            ),
        ),
        Err(error) => (
            error_fact_ref("nvmlDeviceGetUtilizationRates", Some("percent"), &error),
            error_fact("nvmlDeviceGetUtilizationRates", Some("percent"), error),
        ),
    }
}

fn single_error_group(source_api: &'static str, fact: NvmlFact) -> BTreeMap<String, NvmlNode> {
    let mut group = BTreeMap::new();
    group.insert(
        "_category".to_string(),
        NvmlNode::Fact(NvmlFact {
            source_api,
            state: fact.state,
            raw: None,
            unit: None,
            error_message: fact.error_message,
        }),
    );
    group
}

fn ok_string_fact(source_api: &'static str, raw: String) -> NvmlFact {
    NvmlFact {
        source_api,
        state: NvmlFactState::Ok,
        raw: Some(NvmlFactValue::String(raw)),
        unit: None,
        error_message: None,
    }
}

fn ok_bool_fact(source_api: &'static str, raw: bool) -> NvmlFact {
    NvmlFact {
        source_api,
        state: NvmlFactState::Ok,
        raw: Some(NvmlFactValue::Bool(raw)),
        unit: None,
        error_message: None,
    }
}

fn ok_u32_fact(source_api: &'static str, raw: u32, unit: Option<&'static str>) -> NvmlFact {
    NvmlFact {
        source_api,
        state: NvmlFactState::Ok,
        raw: Some(NvmlFactValue::U32(raw)),
        unit,
        error_message: None,
    }
}

fn ok_u64_fact(source_api: &'static str, raw: u64, unit: Option<&'static str>) -> NvmlFact {
    NvmlFact {
        source_api,
        state: NvmlFactState::Ok,
        raw: Some(NvmlFactValue::U64(raw)),
        unit,
        error_message: None,
    }
}

fn ok_object_fact(source_api: &'static str, raw: BTreeMap<String, NvmlFactValue>) -> NvmlFact {
    NvmlFact {
        source_api,
        state: NvmlFactState::Ok,
        raw: Some(NvmlFactValue::Object(raw)),
        unit: None,
        error_message: None,
    }
}

fn not_available_fact(source_api: &'static str, unit: Option<&'static str>) -> NvmlFact {
    NvmlFact {
        source_api,
        state: NvmlFactState::NotAvailable,
        raw: None,
        unit,
        error_message: None,
    }
}

fn error_fact(source_api: &'static str, unit: Option<&'static str>, error: NvmlError) -> NvmlFact {
    error_fact_ref(source_api, unit, &error)
}

fn error_fact_ref(
    source_api: &'static str,
    unit: Option<&'static str>,
    error: &NvmlError,
) -> NvmlFact {
    NvmlFact {
        source_api,
        state: map_error_state(error),
        raw: None,
        unit,
        error_message: Some(error.to_string()),
    }
}

fn map_error_state(error: &NvmlError) -> NvmlFactState {
    match error {
        NvmlError::NotSupported => NvmlFactState::Unsupported,
        NvmlError::NoPermission => NvmlFactState::PermissionDenied,
        NvmlError::NotFound
        | NvmlError::NoData
        | NvmlError::DriverNotLoaded
        | NvmlError::FunctionNotFound
        | NvmlError::LibraryNotFound => NvmlFactState::NotAvailable,
        _ => NvmlFactState::Error,
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
