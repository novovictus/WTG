use std::ffi::{c_char, c_void, CStr, OsStr};
use std::iter;
use std::os::windows::ffi::OsStrExt;
use std::ptr::{self, NonNull};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{json, Value};

const SOURCE: &str = "wtg.provider.amd.adlx";
const TELEMETRY_CLASS: &str = "provider_telemetry";
const PROVIDER: &str = "amd.adlx";
const PROVIDER_AUTHORITY: &str = "AMD ADLX";
const ADLX_DLL_NAME: &str = "amdadlx64.dll";
const ADLX_OK: i32 = 0;
const ADLX_ALREADY_INITIALIZED: i32 = 2;
const ADLX_FAIL: i32 = 3;
const ADLX_ADL_INIT_ERROR: i32 = 8;
const ADLX_NOT_FOUND: i32 = 9;
const ADLX_NOT_SUPPORTED: i32 = 12;
const ADLX_GPU_INACTIVE: i32 = 14;
const ADLX_NOT_ACTIVE: i32 = 17;
const ADLX_VER_MAJOR: u64 = 1;
const ADLX_VER_MINOR: u64 = 5;
const ADLX_VER_RELEASE: u64 = 0;
const ADLX_VER_BUILD_NUM: u64 = 124;
const ADLX_FULL_VERSION: u64 =
    (ADLX_VER_MAJOR << 48) | (ADLX_VER_MINOR << 32) | (ADLX_VER_RELEASE << 16) | ADLX_VER_BUILD_NUM;

type AdlxResult = i32;
type AdlxBool = u8;
type AdlxInt = i32;
type AdlxUInt = u32;
type AdlxInt64 = i64;
type AdlxDouble = f64;

type AdlxInitializeFn = unsafe extern "C" fn(u64, *mut *mut IADLXSystem) -> AdlxResult;
type AdlxInitialize2Fn =
    unsafe extern "C" fn(u64, *mut *mut IADLXSystem, *mut *mut IADLXInterface) -> AdlxResult;
type AdlxTerminateFn = unsafe extern "C" fn() -> AdlxResult;
type AdlxQueryVersionFn = unsafe extern "C" fn(*mut *const c_char) -> AdlxResult;

#[derive(Debug, Serialize)]
pub struct ProviderSample {
    wtg_version: &'static str,
    source: &'static str,
    telemetry_class: &'static str,
    provider: &'static str,
    provider_authority: &'static str,
    status: &'static str,
    sample_seq: u64,
    timestamp_unix_ms: u128,
    probe_attempted: bool,
    dll_name: Option<String>,
    dll_path: Option<String>,
    runtime_dll_state: &'static str,
    runtime_version: Option<String>,
    init_state: &'static str,
    adlx_initialized: bool,
    devices_returned: Option<usize>,
    gpus: Vec<AdlxGpuRecord>,
    errors: Vec<String>,
}

#[derive(Debug, Serialize)]
struct AdlxGpuRecord {
    gpu_index: usize,
    adapter_name: Option<String>,
    vendor_id: Option<String>,
    device_id: Option<String>,
    unique_id: Option<i32>,
    total_vram_mb: Option<u32>,
    driver_path: Option<String>,
    pnp_string: Option<String>,
    metrics: Vec<MetricFact>,
    errors: Vec<String>,
}

#[derive(Debug, Serialize)]
struct MetricFact {
    key: &'static str,
    source_api: &'static str,
    state: &'static str,
    raw: Value,
    unit: Option<&'static str>,
    error_message: Option<String>,
}

struct AdlxLibrary {
    module: NonNull<c_void>,
    dll_name: &'static str,
    dll_path: String,
    initialize: AdlxInitializeFn,
    initialize2: Option<AdlxInitialize2Fn>,
    terminate: AdlxTerminateFn,
    query_version: Option<AdlxQueryVersionFn>,
}

impl Drop for AdlxLibrary {
    fn drop(&mut self) {
        unsafe {
            FreeLibrary(self.module.as_ptr());
        }
    }
}

struct AdlxSession<'a> {
    _library: &'a AdlxLibrary,
    initialized: bool,
    adl_mapping: *mut IADLXInterface,
}

impl<'a> Drop for AdlxSession<'a> {
    fn drop(&mut self) {
        unsafe {
            release_interface(self.adl_mapping);
        }
        if self.initialized {
            unsafe {
                (self._library.terminate)();
            }
        }
    }
}

#[repr(C)]
struct IADLXInterface {
    p_vtbl: *const IADLXInterfaceVtbl,
}

#[repr(C)]
struct IADLXInterfaceVtbl {
    acquire: unsafe extern "system" fn(*mut IADLXInterface) -> i32,
    release: unsafe extern "system" fn(*mut IADLXInterface) -> i32,
    query_interface:
        unsafe extern "system" fn(*mut IADLXInterface, *const u16, *mut *mut c_void) -> AdlxResult,
}

#[repr(C)]
struct IADLXSystem {
    p_vtbl: *const IADLXSystemVtbl,
}

#[repr(C)]
struct IADLXSystemVtbl {
    get_hybrid_graphics_type: unsafe extern "system" fn(*mut IADLXSystem, *mut i32) -> AdlxResult,
    get_gpus: unsafe extern "system" fn(*mut IADLXSystem, *mut *mut IADLXGPUList) -> AdlxResult,
    query_interface:
        unsafe extern "system" fn(*mut IADLXSystem, *const u16, *mut *mut c_void) -> AdlxResult,
    get_displays_services:
        unsafe extern "system" fn(*mut IADLXSystem, *mut *mut c_void) -> AdlxResult,
    get_desktops_services:
        unsafe extern "system" fn(*mut IADLXSystem, *mut *mut c_void) -> AdlxResult,
    get_gpus_changed_handling:
        unsafe extern "system" fn(*mut IADLXSystem, *mut *mut c_void) -> AdlxResult,
    enable_log: unsafe extern "system" fn(
        *mut IADLXSystem,
        i32,
        i32,
        *mut c_void,
        *const u16,
    ) -> AdlxResult,
    get_3d_settings_services:
        unsafe extern "system" fn(*mut IADLXSystem, *mut *mut c_void) -> AdlxResult,
    get_gpu_tuning_services:
        unsafe extern "system" fn(*mut IADLXSystem, *mut *mut c_void) -> AdlxResult,
    get_performance_monitoring_services: unsafe extern "system" fn(
        *mut IADLXSystem,
        *mut *mut IADLXPerformanceMonitoringServices,
    ) -> AdlxResult,
    total_system_ram: unsafe extern "system" fn(*mut IADLXSystem, *mut AdlxUInt) -> AdlxResult,
    get_i2c:
        unsafe extern "system" fn(*mut IADLXSystem, *mut IADLXGPU, *mut *mut c_void) -> AdlxResult,
}

#[repr(C)]
struct IADLXGPUList {
    p_vtbl: *const IADLXGPUListVtbl,
}

#[repr(C)]
struct IADLXGPUListVtbl {
    acquire: unsafe extern "system" fn(*mut IADLXGPUList) -> i32,
    release: unsafe extern "system" fn(*mut IADLXGPUList) -> i32,
    query_interface:
        unsafe extern "system" fn(*mut IADLXGPUList, *const u16, *mut *mut c_void) -> AdlxResult,
    size: unsafe extern "system" fn(*mut IADLXGPUList) -> AdlxUInt,
    empty: unsafe extern "system" fn(*mut IADLXGPUList) -> AdlxBool,
    begin: unsafe extern "system" fn(*mut IADLXGPUList) -> AdlxUInt,
    end: unsafe extern "system" fn(*mut IADLXGPUList) -> AdlxUInt,
    at: unsafe extern "system" fn(
        *mut IADLXGPUList,
        AdlxUInt,
        *mut *mut IADLXInterface,
    ) -> AdlxResult,
    clear: unsafe extern "system" fn(*mut IADLXGPUList) -> AdlxResult,
    remove_back: unsafe extern "system" fn(*mut IADLXGPUList) -> AdlxResult,
    add_back: unsafe extern "system" fn(*mut IADLXGPUList, *mut IADLXInterface) -> AdlxResult,
    at_gpu_list:
        unsafe extern "system" fn(*mut IADLXGPUList, AdlxUInt, *mut *mut IADLXGPU) -> AdlxResult,
    add_back_gpu_list: unsafe extern "system" fn(*mut IADLXGPUList, *mut IADLXGPU) -> AdlxResult,
}

#[repr(C)]
struct IADLXGPU {
    p_vtbl: *const IADLXGPUVtbl,
}

#[repr(C)]
struct IADLXGPUVtbl {
    acquire: unsafe extern "system" fn(*mut IADLXGPU) -> i32,
    release: unsafe extern "system" fn(*mut IADLXGPU) -> i32,
    query_interface:
        unsafe extern "system" fn(*mut IADLXGPU, *const u16, *mut *mut c_void) -> AdlxResult,
    vendor_id: unsafe extern "system" fn(*mut IADLXGPU, *mut *const c_char) -> AdlxResult,
    asic_family_type: unsafe extern "system" fn(*mut IADLXGPU, *mut i32) -> AdlxResult,
    gpu_type: unsafe extern "system" fn(*mut IADLXGPU, *mut i32) -> AdlxResult,
    is_external: unsafe extern "system" fn(*mut IADLXGPU, *mut AdlxBool) -> AdlxResult,
    name: unsafe extern "system" fn(*mut IADLXGPU, *mut *const c_char) -> AdlxResult,
    driver_path: unsafe extern "system" fn(*mut IADLXGPU, *mut *const c_char) -> AdlxResult,
    pnp_string: unsafe extern "system" fn(*mut IADLXGPU, *mut *const c_char) -> AdlxResult,
    has_desktops: unsafe extern "system" fn(*mut IADLXGPU, *mut AdlxBool) -> AdlxResult,
    total_vram: unsafe extern "system" fn(*mut IADLXGPU, *mut AdlxUInt) -> AdlxResult,
    vram_type: unsafe extern "system" fn(*mut IADLXGPU, *mut *const c_char) -> AdlxResult,
    bios_info: unsafe extern "system" fn(
        *mut IADLXGPU,
        *mut *const c_char,
        *mut *const c_char,
        *mut *const c_char,
    ) -> AdlxResult,
    device_id: unsafe extern "system" fn(*mut IADLXGPU, *mut *const c_char) -> AdlxResult,
    revision_id: unsafe extern "system" fn(*mut IADLXGPU, *mut *const c_char) -> AdlxResult,
    subsystem_id: unsafe extern "system" fn(*mut IADLXGPU, *mut *const c_char) -> AdlxResult,
    subsystem_vendor_id: unsafe extern "system" fn(*mut IADLXGPU, *mut *const c_char) -> AdlxResult,
    unique_id: unsafe extern "system" fn(*mut IADLXGPU, *mut AdlxInt) -> AdlxResult,
}

#[repr(C)]
struct IADLXPerformanceMonitoringServices {
    p_vtbl: *const IADLXPerformanceMonitoringServicesVtbl,
}

#[repr(C)]
struct ADLXIntRange {
    min_value: AdlxInt,
    max_value: AdlxInt,
    step: AdlxInt,
}

#[repr(C)]
struct IADLXPerformanceMonitoringServicesVtbl {
    acquire: unsafe extern "system" fn(*mut IADLXPerformanceMonitoringServices) -> i32,
    release: unsafe extern "system" fn(*mut IADLXPerformanceMonitoringServices) -> i32,
    query_interface: unsafe extern "system" fn(
        *mut IADLXPerformanceMonitoringServices,
        *const u16,
        *mut *mut c_void,
    ) -> AdlxResult,
    get_sampling_interval_range: unsafe extern "system" fn(
        *mut IADLXPerformanceMonitoringServices,
        *mut ADLXIntRange,
    ) -> AdlxResult,
    set_sampling_interval:
        unsafe extern "system" fn(*mut IADLXPerformanceMonitoringServices, AdlxInt) -> AdlxResult,
    get_sampling_interval: unsafe extern "system" fn(
        *mut IADLXPerformanceMonitoringServices,
        *mut AdlxInt,
    ) -> AdlxResult,
    get_max_performance_metrics_history_size_range: unsafe extern "system" fn(
        *mut IADLXPerformanceMonitoringServices,
        *mut ADLXIntRange,
    ) -> AdlxResult,
    set_max_performance_metrics_history_size:
        unsafe extern "system" fn(*mut IADLXPerformanceMonitoringServices, AdlxInt) -> AdlxResult,
    get_max_performance_metrics_history_size: unsafe extern "system" fn(
        *mut IADLXPerformanceMonitoringServices,
        *mut AdlxInt,
    ) -> AdlxResult,
    clear_performance_metrics_history:
        unsafe extern "system" fn(*mut IADLXPerformanceMonitoringServices) -> AdlxResult,
    get_current_performance_metrics_history_size: unsafe extern "system" fn(
        *mut IADLXPerformanceMonitoringServices,
        *mut AdlxInt,
    ) -> AdlxResult,
    start_performance_metrics_tracking:
        unsafe extern "system" fn(*mut IADLXPerformanceMonitoringServices) -> AdlxResult,
    stop_performance_metrics_tracking:
        unsafe extern "system" fn(*mut IADLXPerformanceMonitoringServices) -> AdlxResult,
    get_all_metrics_history: unsafe extern "system" fn(
        *mut IADLXPerformanceMonitoringServices,
        AdlxInt,
        AdlxInt,
        *mut *mut c_void,
    ) -> AdlxResult,
    get_gpu_metrics_history: unsafe extern "system" fn(
        *mut IADLXPerformanceMonitoringServices,
        *mut IADLXGPU,
        AdlxInt,
        AdlxInt,
        *mut *mut c_void,
    ) -> AdlxResult,
    get_system_metrics_history: unsafe extern "system" fn(
        *mut IADLXPerformanceMonitoringServices,
        AdlxInt,
        AdlxInt,
        *mut *mut c_void,
    ) -> AdlxResult,
    get_fps_history: unsafe extern "system" fn(
        *mut IADLXPerformanceMonitoringServices,
        AdlxInt,
        AdlxInt,
        *mut *mut c_void,
    ) -> AdlxResult,
    get_current_all_metrics: unsafe extern "system" fn(
        *mut IADLXPerformanceMonitoringServices,
        *mut *mut c_void,
    ) -> AdlxResult,
    get_current_gpu_metrics: unsafe extern "system" fn(
        *mut IADLXPerformanceMonitoringServices,
        *mut IADLXGPU,
        *mut *mut IADLXGPUMetrics,
    ) -> AdlxResult,
    get_current_system_metrics: unsafe extern "system" fn(
        *mut IADLXPerformanceMonitoringServices,
        *mut *mut c_void,
    ) -> AdlxResult,
    get_current_fps: unsafe extern "system" fn(
        *mut IADLXPerformanceMonitoringServices,
        *mut *mut c_void,
    ) -> AdlxResult,
    get_supported_gpu_metrics: unsafe extern "system" fn(
        *mut IADLXPerformanceMonitoringServices,
        *mut IADLXGPU,
        *mut *mut IADLXGPUMetricsSupport,
    ) -> AdlxResult,
    get_supported_system_metrics: unsafe extern "system" fn(
        *mut IADLXPerformanceMonitoringServices,
        *mut *mut c_void,
    ) -> AdlxResult,
}

#[repr(C)]
struct IADLXGPUMetricsSupport {
    p_vtbl: *const IADLXGPUMetricsSupportVtbl,
}

#[repr(C)]
struct IADLXGPUMetricsSupportVtbl {
    acquire: unsafe extern "system" fn(*mut IADLXGPUMetricsSupport) -> i32,
    release: unsafe extern "system" fn(*mut IADLXGPUMetricsSupport) -> i32,
    query_interface: unsafe extern "system" fn(
        *mut IADLXGPUMetricsSupport,
        *const u16,
        *mut *mut c_void,
    ) -> AdlxResult,
    is_supported_gpu_usage:
        unsafe extern "system" fn(*mut IADLXGPUMetricsSupport, *mut AdlxBool) -> AdlxResult,
    is_supported_gpu_clock_speed:
        unsafe extern "system" fn(*mut IADLXGPUMetricsSupport, *mut AdlxBool) -> AdlxResult,
    is_supported_gpu_vram_clock_speed:
        unsafe extern "system" fn(*mut IADLXGPUMetricsSupport, *mut AdlxBool) -> AdlxResult,
    is_supported_gpu_temperature:
        unsafe extern "system" fn(*mut IADLXGPUMetricsSupport, *mut AdlxBool) -> AdlxResult,
    is_supported_gpu_hotspot_temperature:
        unsafe extern "system" fn(*mut IADLXGPUMetricsSupport, *mut AdlxBool) -> AdlxResult,
    is_supported_gpu_power:
        unsafe extern "system" fn(*mut IADLXGPUMetricsSupport, *mut AdlxBool) -> AdlxResult,
    is_supported_gpu_total_board_power:
        unsafe extern "system" fn(*mut IADLXGPUMetricsSupport, *mut AdlxBool) -> AdlxResult,
    is_supported_gpu_fan_speed:
        unsafe extern "system" fn(*mut IADLXGPUMetricsSupport, *mut AdlxBool) -> AdlxResult,
    is_supported_gpu_vram:
        unsafe extern "system" fn(*mut IADLXGPUMetricsSupport, *mut AdlxBool) -> AdlxResult,
    is_supported_gpu_voltage:
        unsafe extern "system" fn(*mut IADLXGPUMetricsSupport, *mut AdlxBool) -> AdlxResult,
    get_gpu_usage_range: unsafe extern "system" fn(
        *mut IADLXGPUMetricsSupport,
        *mut AdlxInt,
        *mut AdlxInt,
    ) -> AdlxResult,
    get_gpu_clock_speed_range: unsafe extern "system" fn(
        *mut IADLXGPUMetricsSupport,
        *mut AdlxInt,
        *mut AdlxInt,
    ) -> AdlxResult,
    get_gpu_vram_clock_speed_range: unsafe extern "system" fn(
        *mut IADLXGPUMetricsSupport,
        *mut AdlxInt,
        *mut AdlxInt,
    ) -> AdlxResult,
    get_gpu_temperature_range: unsafe extern "system" fn(
        *mut IADLXGPUMetricsSupport,
        *mut AdlxInt,
        *mut AdlxInt,
    ) -> AdlxResult,
    get_gpu_hotspot_temperature_range: unsafe extern "system" fn(
        *mut IADLXGPUMetricsSupport,
        *mut AdlxInt,
        *mut AdlxInt,
    ) -> AdlxResult,
    get_gpu_power_range: unsafe extern "system" fn(
        *mut IADLXGPUMetricsSupport,
        *mut AdlxInt,
        *mut AdlxInt,
    ) -> AdlxResult,
    get_gpu_fan_speed_range: unsafe extern "system" fn(
        *mut IADLXGPUMetricsSupport,
        *mut AdlxInt,
        *mut AdlxInt,
    ) -> AdlxResult,
    get_gpu_vram_range: unsafe extern "system" fn(
        *mut IADLXGPUMetricsSupport,
        *mut AdlxInt,
        *mut AdlxInt,
    ) -> AdlxResult,
    get_gpu_voltage_range: unsafe extern "system" fn(
        *mut IADLXGPUMetricsSupport,
        *mut AdlxInt,
        *mut AdlxInt,
    ) -> AdlxResult,
    get_gpu_total_board_power_range: unsafe extern "system" fn(
        *mut IADLXGPUMetricsSupport,
        *mut AdlxInt,
        *mut AdlxInt,
    ) -> AdlxResult,
    get_gpu_intake_temperature_range: unsafe extern "system" fn(
        *mut IADLXGPUMetricsSupport,
        *mut AdlxInt,
        *mut AdlxInt,
    ) -> AdlxResult,
    is_supported_gpu_intake_temperature:
        unsafe extern "system" fn(*mut IADLXGPUMetricsSupport, *mut AdlxBool) -> AdlxResult,
}

#[repr(C)]
struct IADLXGPUMetrics {
    p_vtbl: *const IADLXGPUMetricsVtbl,
}

#[repr(C)]
struct IADLXGPUMetricsVtbl {
    acquire: unsafe extern "system" fn(*mut IADLXGPUMetrics) -> i32,
    release: unsafe extern "system" fn(*mut IADLXGPUMetrics) -> i32,
    query_interface:
        unsafe extern "system" fn(*mut IADLXGPUMetrics, *const u16, *mut *mut c_void) -> AdlxResult,
    timestamp: unsafe extern "system" fn(*mut IADLXGPUMetrics, *mut AdlxInt64) -> AdlxResult,
    gpu_usage: unsafe extern "system" fn(*mut IADLXGPUMetrics, *mut AdlxDouble) -> AdlxResult,
    gpu_clock_speed: unsafe extern "system" fn(*mut IADLXGPUMetrics, *mut AdlxInt) -> AdlxResult,
    gpu_vram_clock_speed:
        unsafe extern "system" fn(*mut IADLXGPUMetrics, *mut AdlxInt) -> AdlxResult,
    gpu_temperature: unsafe extern "system" fn(*mut IADLXGPUMetrics, *mut AdlxDouble) -> AdlxResult,
    gpu_hotspot_temperature:
        unsafe extern "system" fn(*mut IADLXGPUMetrics, *mut AdlxDouble) -> AdlxResult,
    gpu_power: unsafe extern "system" fn(*mut IADLXGPUMetrics, *mut AdlxDouble) -> AdlxResult,
    gpu_total_board_power:
        unsafe extern "system" fn(*mut IADLXGPUMetrics, *mut AdlxDouble) -> AdlxResult,
    gpu_fan_speed: unsafe extern "system" fn(*mut IADLXGPUMetrics, *mut AdlxInt) -> AdlxResult,
    gpu_vram: unsafe extern "system" fn(*mut IADLXGPUMetrics, *mut AdlxInt) -> AdlxResult,
    gpu_voltage: unsafe extern "system" fn(*mut IADLXGPUMetrics, *mut AdlxInt) -> AdlxResult,
    gpu_intake_temperature:
        unsafe extern "system" fn(*mut IADLXGPUMetrics, *mut AdlxDouble) -> AdlxResult,
}

pub fn provider_source() -> &'static str {
    SOURCE
}

pub fn telemetry_class() -> &'static str {
    TELEMETRY_CLASS
}

pub fn sample_status(sample: &ProviderSample) -> &'static str {
    sample.status
}

pub fn collect_once(sample_seq: u64) -> ProviderSample {
    let timestamp_unix_ms = now_unix_ms();
    match collect_once_inner(sample_seq, timestamp_unix_ms) {
        Ok(sample) => sample,
        Err(sample) => sample,
    }
}

pub fn format_snapshot(sample: &ProviderSample) -> String {
    let reason = primary_reason(sample);
    let mut lines = vec![
        "ADLX Provider:".to_string(),
        format!(
            "  Probe attempted: {}",
            if sample.probe_attempted { "yes" } else { "no" }
        ),
        format!("  Status: {}", snapshot_status_label(sample)),
        format!("  Runtime/DLL: {}", sample.runtime_dll_state),
        format!("  Init: {}", sample.init_state),
        format!(
            "  Devices returned: {}",
            sample
                .devices_returned
                .map(|count| count.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        ),
    ];

    if snapshot_status_label(sample) == "unavailable" {
        lines.push(format!("  Reason: {reason}"));
        return lines.join("\n");
    }

    for gpu in &sample.gpus {
        lines.push(String::new());
        push_gpu_snapshot_lines(&mut lines, sample, gpu);
    }

    if lines.len() == 6 {
        lines.push(format!("  Reason: {reason}"));
        lines.join("\n")
    } else {
        lines.join("\n")
    }
}

pub fn format_watch_sample(sample: &ProviderSample) -> String {
    let reason = primary_reason(sample);
    match sample.status {
        "ok" => {
            let mut lines = vec![format!("sample_seq: {}", sample.sample_seq)];
            for gpu in &sample.gpus {
                lines.push(String::new());
                push_gpu_snapshot_lines(&mut lines, sample, gpu);
            }
            lines.join("\n")
        }
        "unavailable" => format!(
            "sample_seq: {}\nProvider status: unavailable\nReason: {}",
            sample.sample_seq, reason
        ),
        "error" => format!(
            "sample_seq: {}\nProvider status: error\nReason: {}",
            sample.sample_seq, reason
        ),
        other => format!(
            "sample_seq: {}\nProvider status: {}\nReason: {}",
            sample.sample_seq, other, reason
        ),
    }
}

pub fn format_probe_snapshot(sample: &ProviderSample) -> String {
    let mut lines = Vec::new();
    lines.push("[probe] provider=amd_adlx".to_string());
    lines.push(format!("wtg.version: {}", sample.wtg_version));
    lines.push(format!("provider.authority: {}", sample.provider_authority));
    lines.push(format!("provider.source: {}", sample.source));
    lines.push(format!("telemetry.class: {}", sample.telemetry_class));
    lines.push(format!("provider.status: {}", sample.status));
    lines.push(format!("adlx.probe_attempted: {}", sample.probe_attempted));
    if let Some(dll_name) = &sample.dll_name {
        lines.push(format!("adlx.dll_name: {}", dll_name));
    }
    if let Some(dll_path) = &sample.dll_path {
        lines.push(format!("adlx.dll_path: {}", dll_path));
    }
    lines.push(format!("adlx.runtime_dll_state: {}", sample.runtime_dll_state));
    if let Some(version) = &sample.runtime_version {
        lines.push(format!("adlx.runtime_version: {}", version));
    }
    lines.push(format!("adlx.init_state: {}", sample.init_state));
    lines.push(format!("adlx.initialized: {}", sample.adlx_initialized));
    lines.push(format!(
        "adlx.gpu_count: {}",
        sample
            .devices_returned
            .map(|count| count.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    ));
    for gpu in &sample.gpus {
        lines.push(String::new());
        lines.push(format!("[probe] amd_adlx_gpu={}", gpu.gpu_index));
        if let Some(name) = &gpu.adapter_name {
            lines.push(format!("gpu.name: {}", name));
        }
        if let Some(vendor_id) = &gpu.vendor_id {
            lines.push(format!("gpu.vendor_id: {}", vendor_id));
        }
        if let Some(device_id) = &gpu.device_id {
            lines.push(format!("gpu.device_id: {}", device_id));
        }
        if let Some(unique_id) = gpu.unique_id {
            lines.push(format!("gpu.unique_id: {}", unique_id));
        }
        if let Some(total_vram_mb) = gpu.total_vram_mb {
            lines.push(format!("gpu.total_vram_mb: {}", total_vram_mb));
        }
        if let Some(driver_path) = &gpu.driver_path {
            lines.push(format!("gpu.driver_path: {}", driver_path));
        }
        if let Some(pnp_string) = &gpu.pnp_string {
            lines.push(format!("gpu.pnp_string: {}", pnp_string));
        }
        for metric in &gpu.metrics {
            lines.push(format!(
                "{}.state: {}",
                metric.key.replace('.', "_"),
                metric.state
            ));
            lines.push(format!(
                "{}.source_api: {}",
                metric.key.replace('.', "_"),
                metric.source_api
            ));
            if !metric.raw.is_null() {
                lines.push(format!(
                    "{}.raw: {}",
                    metric.key.replace('.', "_"),
                    metric.raw
                ));
            }
            if let Some(unit) = metric.unit {
                lines.push(format!("{}.unit: {}", metric.key.replace('.', "_"), unit));
            }
            if let Some(error_message) = &metric.error_message {
                lines.push(format!(
                    "{}.error: {}",
                    metric.key.replace('.', "_"),
                    error_message
                ));
            }
        }
        for error in &gpu.errors {
            lines.push(format!("gpu.error: {}", error));
        }
    }
    for error in &sample.errors {
        lines.push(format!("provider.error: {}", error));
    }
    lines.join("\n")
}

pub fn format_stats_snapshot_json(sample: &ProviderSample, tick_seq: u64, tick_ts: &str) -> String {
    let gpus = sample
        .gpus
        .iter()
        .map(|gpu| {
            json!({
                "gpu_index": gpu.gpu_index,
                "adapter_name": stats_string_field(gpu.adapter_name.as_deref(), "IADLXGPU::Name"),
                "vendor_id": stats_string_field(gpu.vendor_id.as_deref(), "IADLXGPU::VendorId"),
                "device_id": stats_string_field(gpu.device_id.as_deref(), "IADLXGPU::DeviceId"),
                "unique_id": stats_i32_field(gpu.unique_id, "IADLXGPU::UniqueId"),
                "total_vram_mb": stats_u32_field(gpu.total_vram_mb, "IADLXGPU::TotalVRAM", Some("MiB")),
                "driver_path": stats_string_field(gpu.driver_path.as_deref(), "IADLXGPU::DriverPath"),
                "pnp_string": stats_string_field(gpu.pnp_string.as_deref(), "IADLXGPU::PNPString"),
                "metrics": gpu.metrics,
                "errors": gpu.errors,
            })
        })
        .collect::<Vec<_>>();

    let payload = json!({
        "provider": "amd-adlx",
        "provider_authority": PROVIDER_AUTHORITY,
        "provider_source": SOURCE,
        "schema": "wtg.amd_adlx.stats.v1",
        "telemetry_class": TELEMETRY_CLASS,
        "tick_seq": tick_seq,
        "tick_ts": tick_ts,
        "timestamp_unix_ms": sample.timestamp_unix_ms,
        "wtg_version": sample.wtg_version,
        "status": sample.status,
        "probe_attempted": sample.probe_attempted,
        "dll_name": sample.dll_name,
        "dll_path": sample.dll_path,
        "runtime_dll_state": sample.runtime_dll_state,
        "runtime_version": sample.runtime_version,
        "init_state": sample.init_state,
        "adlx_initialized": sample.adlx_initialized,
        "devices_returned": sample.devices_returned,
        "gpus": gpus,
        "errors": sample.errors,
    });

    serde_json::to_string_pretty(&payload).expect("ADLX stats JSON serialization should succeed")
}

fn collect_once_inner(
    sample_seq: u64,
    timestamp_unix_ms: u128,
) -> Result<ProviderSample, ProviderSample> {
    let library = match load_adlx_library() {
        Ok(library) => library,
        Err(reason) => {
            return Err(unavailable_sample(
                sample_seq,
                timestamp_unix_ms,
                true,
                None,
                None,
                "not found",
                None,
                "not attempted",
                None,
                reason,
            ));
        }
    };

    let runtime_version = query_runtime_version(&library);
    let mut system = ptr::null_mut::<IADLXSystem>();
    let mut adl_mapping = ptr::null_mut::<IADLXInterface>();
    let (init_api, init_result) = unsafe {
        if let Some(initialize2) = library.initialize2 {
            (
                "ADLXInitialize2",
                initialize2(ADLX_FULL_VERSION, &mut system, &mut adl_mapping),
            )
        } else {
            ("ADLXInitialize", (library.initialize)(ADLX_FULL_VERSION, &mut system))
        }
    };
    if init_result != ADLX_OK && init_result != ADLX_ALREADY_INITIALIZED {
        let reason = format!("{init_api} failed: {}", adlx_result_name(init_result));
        return Err(unavailable_sample(
            sample_seq,
            timestamp_unix_ms,
            true,
            Some(library.dll_name.to_string()),
            Some(library.dll_path.clone()),
            "found",
            runtime_version,
            "failed",
            None,
            reason,
        ));
    }
    if system.is_null() {
        return Err(error_sample(
            sample_seq,
            timestamp_unix_ms,
            true,
            Some(library.dll_name.to_string()),
            Some(library.dll_path.clone()),
            "found",
            runtime_version,
            "failed",
            None,
            format!("{init_api} returned a null system interface."),
        ));
    }
    let _session = AdlxSession {
        _library: &library,
        initialized: true,
        adl_mapping,
    };

    let mut gpu_list = ptr::null_mut::<IADLXGPUList>();
    let get_gpus_result = unsafe { ((*(*system).p_vtbl).get_gpus)(system, &mut gpu_list) };
    if get_gpus_result != ADLX_OK {
        return Err(error_sample(
            sample_seq,
            timestamp_unix_ms,
            true,
            Some(library.dll_name.to_string()),
            Some(library.dll_path.clone()),
            "found",
            runtime_version,
            "succeeded",
            None,
            format!(
                "IADLXSystem::GetGPUs failed: {}",
                adlx_result_name(get_gpus_result)
            ),
        ));
    }
    if gpu_list.is_null() {
        return Err(unavailable_sample(
            sample_seq,
            timestamp_unix_ms,
            true,
            Some(library.dll_name.to_string()),
            Some(library.dll_path.clone()),
            "found",
            runtime_version,
            "succeeded",
            None,
            "ADLX returned a null GPU list.".to_string(),
        ));
    }

    let mut perf_services = ptr::null_mut::<IADLXPerformanceMonitoringServices>();
    let perf_result = unsafe {
        ((*(*system).p_vtbl).get_performance_monitoring_services)(system, &mut perf_services)
    };
    if perf_result != ADLX_OK || perf_services.is_null() {
        unsafe {
            release_interface(gpu_list.cast::<IADLXInterface>());
        }
        return Err(unavailable_sample(
            sample_seq,
            timestamp_unix_ms,
            true,
            Some(library.dll_name.to_string()),
            Some(library.dll_path.clone()),
            "found",
            runtime_version,
            "succeeded",
            None,
            format!(
                "IADLXSystem::GetPerformanceMonitoringServices failed: {}",
                adlx_result_name(perf_result)
            ),
        ));
    }

    let gpus = unsafe { collect_gpu_records(gpu_list, perf_services) };
    unsafe {
        release_interface(perf_services.cast::<IADLXInterface>());
        release_interface(gpu_list.cast::<IADLXInterface>());
    }

    if gpus.is_empty() {
        return Err(unavailable_sample(
            sample_seq,
            timestamp_unix_ms,
            true,
            Some(library.dll_name.to_string()),
            Some(library.dll_path.clone()),
            "found",
            runtime_version,
            "succeeded",
            Some(0),
            "ADLX returned zero AMD GPUs.".to_string(),
        ));
    }

    let status = classify_status(&gpus);
    let mut errors = Vec::new();
    if status != "ok" {
        errors.push(no_metrics_reason(&gpus));
    }

    let sample = ProviderSample {
        wtg_version: env!("CARGO_PKG_VERSION"),
        source: SOURCE,
        telemetry_class: TELEMETRY_CLASS,
        provider: PROVIDER,
        provider_authority: PROVIDER_AUTHORITY,
        status,
        sample_seq,
        timestamp_unix_ms,
        probe_attempted: true,
        dll_name: Some(library.dll_name.to_string()),
        dll_path: Some(library.dll_path.clone()),
        runtime_dll_state: "found",
        runtime_version,
        init_state: "succeeded",
        adlx_initialized: true,
        devices_returned: Some(gpus.len()),
        gpus,
        errors,
    };

    if status == "error" || status == "unavailable" {
        Err(sample)
    } else {
        Ok(sample)
    }
}

unsafe fn collect_gpu_records(
    gpu_list: *mut IADLXGPUList,
    perf_services: *mut IADLXPerformanceMonitoringServices,
) -> Vec<AdlxGpuRecord> {
    let begin = ((*(*gpu_list).p_vtbl).begin)(gpu_list);
    let end = ((*(*gpu_list).p_vtbl).end)(gpu_list);
    let mut records = Vec::new();

    for index in begin..end {
        let mut gpu = ptr::null_mut::<IADLXGPU>();
        let at_result = ((*(*gpu_list).p_vtbl).at_gpu_list)(gpu_list, index, &mut gpu);
        if at_result != ADLX_OK || gpu.is_null() {
            records.push(AdlxGpuRecord {
                gpu_index: index as usize,
                adapter_name: None,
                vendor_id: None,
                device_id: None,
                unique_id: None,
                total_vram_mb: None,
                driver_path: None,
                pnp_string: None,
                metrics: Vec::new(),
                errors: vec![format!(
                    "IADLXGPUList::At failed for index {index}: {}",
                    adlx_result_name(at_result)
                )],
            });
            continue;
        }

        let record = collect_single_gpu_record(index as usize, gpu, perf_services);
        release_interface(gpu.cast::<IADLXInterface>());
        records.push(record);
    }

    records
}

unsafe fn collect_single_gpu_record(
    gpu_index: usize,
    gpu: *mut IADLXGPU,
    perf_services: *mut IADLXPerformanceMonitoringServices,
) -> AdlxGpuRecord {
    let adapter_name = read_gpu_string(gpu, (*(*gpu).p_vtbl).name);
    let vendor_id = read_gpu_string(gpu, (*(*gpu).p_vtbl).vendor_id);
    let device_id = read_gpu_string(gpu, (*(*gpu).p_vtbl).device_id);
    let driver_path = read_gpu_string(gpu, (*(*gpu).p_vtbl).driver_path);
    let pnp_string = read_gpu_string(gpu, (*(*gpu).p_vtbl).pnp_string);
    let total_vram_mb = read_gpu_u32(gpu, (*(*gpu).p_vtbl).total_vram);
    let unique_id = read_gpu_i32(gpu, (*(*gpu).p_vtbl).unique_id);

    let mut errors = Vec::new();
    let mut metrics = Vec::new();

    let mut support = ptr::null_mut::<IADLXGPUMetricsSupport>();
    let support_result =
        ((*(*perf_services).p_vtbl).get_supported_gpu_metrics)(perf_services, gpu, &mut support);
    if support_result != ADLX_OK || support.is_null() {
        errors.push(format!(
            "GetSupportedGPUMetrics failed: {}",
            adlx_result_name(support_result)
        ));
        return AdlxGpuRecord {
            gpu_index,
            adapter_name,
            vendor_id,
            device_id,
            unique_id,
            total_vram_mb,
            driver_path,
            pnp_string,
            metrics,
            errors,
        };
    }

    let mut gpu_metrics = ptr::null_mut::<IADLXGPUMetrics>();
    let metrics_result =
        ((*(*perf_services).p_vtbl).get_current_gpu_metrics)(perf_services, gpu, &mut gpu_metrics);
    if metrics_result != ADLX_OK || gpu_metrics.is_null() {
        release_interface(support.cast::<IADLXInterface>());
        errors.push(format!(
            "GetCurrentGPUMetrics failed: {}",
            adlx_result_name(metrics_result)
        ));
        return AdlxGpuRecord {
            gpu_index,
            adapter_name,
            vendor_id,
            device_id,
            unique_id,
            total_vram_mb,
            driver_path,
            pnp_string,
            metrics,
            errors,
        };
    }

    metrics.push(collect_supported_double_metric(
        support,
        (*(*support).p_vtbl).is_supported_gpu_usage,
        gpu_metrics,
        (*(*gpu_metrics).p_vtbl).gpu_usage,
        "gpu_activity_pct",
        "IADLXGPUMetrics::GPUUsage",
        Some("percent"),
    ));
    metrics.push(collect_supported_int_metric(
        support,
        (*(*support).p_vtbl).is_supported_gpu_clock_speed,
        gpu_metrics,
        (*(*gpu_metrics).p_vtbl).gpu_clock_speed,
        "gpu_clock_mhz",
        "IADLXGPUMetrics::GPUClockSpeed",
        Some("MHz"),
    ));
    metrics.push(collect_supported_int_metric(
        support,
        (*(*support).p_vtbl).is_supported_gpu_vram_clock_speed,
        gpu_metrics,
        (*(*gpu_metrics).p_vtbl).gpu_vram_clock_speed,
        "memory_clock_mhz",
        "IADLXGPUMetrics::GPUVRAMClockSpeed",
        Some("MHz"),
    ));
    metrics.push(collect_supported_double_metric(
        support,
        (*(*support).p_vtbl).is_supported_gpu_temperature,
        gpu_metrics,
        (*(*gpu_metrics).p_vtbl).gpu_temperature,
        "gpu_temperature_c",
        "IADLXGPUMetrics::GPUTemperature",
        Some("C"),
    ));
    metrics.push(collect_supported_double_metric(
        support,
        (*(*support).p_vtbl).is_supported_gpu_power,
        gpu_metrics,
        (*(*gpu_metrics).p_vtbl).gpu_power,
        "gpu_power_w",
        "IADLXGPUMetrics::GPUPower",
        Some("W"),
    ));
    metrics.push(collect_supported_double_metric(
        support,
        (*(*support).p_vtbl).is_supported_gpu_total_board_power,
        gpu_metrics,
        (*(*gpu_metrics).p_vtbl).gpu_total_board_power,
        "gpu_total_board_power_w",
        "IADLXGPUMetrics::GPUTotalBoardPower",
        Some("W"),
    ));
    metrics.push(collect_supported_int_metric(
        support,
        (*(*support).p_vtbl).is_supported_gpu_fan_speed,
        gpu_metrics,
        (*(*gpu_metrics).p_vtbl).gpu_fan_speed,
        "gpu_fan_speed_rpm",
        "IADLXGPUMetrics::GPUFanSpeed",
        Some("RPM"),
    ));
    metrics.push(collect_supported_int_metric(
        support,
        (*(*support).p_vtbl).is_supported_gpu_vram,
        gpu_metrics,
        (*(*gpu_metrics).p_vtbl).gpu_vram,
        "gpu_vram_used_mb",
        "IADLXGPUMetrics::GPUVRAM",
        Some("MiB"),
    ));

    release_interface(gpu_metrics.cast::<IADLXInterface>());
    release_interface(support.cast::<IADLXInterface>());

    AdlxGpuRecord {
        gpu_index,
        adapter_name,
        vendor_id,
        device_id,
        unique_id,
        total_vram_mb,
        driver_path,
        pnp_string,
        metrics,
        errors,
    }
}

unsafe fn collect_supported_double_metric<S, M>(
    support: *mut S,
    is_supported: unsafe extern "system" fn(*mut S, *mut AdlxBool) -> AdlxResult,
    metrics: *mut M,
    getter: unsafe extern "system" fn(*mut M, *mut AdlxDouble) -> AdlxResult,
    key: &'static str,
    source_api: &'static str,
    unit: Option<&'static str>,
) -> MetricFact {
    let mut supported = 0u8;
    let support_result = is_supported(support, &mut supported);
    if support_result != ADLX_OK {
        return fact_error(
            key,
            source_api,
            unit,
            format!("support check failed: {}", adlx_result_name(support_result)),
        );
    }
    if supported == 0 {
        return fact_unsupported(key, source_api, unit);
    }

    let mut value = 0.0f64;
    let result = getter(metrics, &mut value);
    if result != ADLX_OK {
        return fact_not_available(
            key,
            source_api,
            unit,
            format!("metric read failed: {}", adlx_result_name(result)),
        );
    }

    fact_ok(key, source_api, json!(value), unit)
}

unsafe fn collect_supported_int_metric<S, M>(
    support: *mut S,
    is_supported: unsafe extern "system" fn(*mut S, *mut AdlxBool) -> AdlxResult,
    metrics: *mut M,
    getter: unsafe extern "system" fn(*mut M, *mut AdlxInt) -> AdlxResult,
    key: &'static str,
    source_api: &'static str,
    unit: Option<&'static str>,
) -> MetricFact {
    let mut supported = 0u8;
    let support_result = is_supported(support, &mut supported);
    if support_result != ADLX_OK {
        return fact_error(
            key,
            source_api,
            unit,
            format!("support check failed: {}", adlx_result_name(support_result)),
        );
    }
    if supported == 0 {
        return fact_unsupported(key, source_api, unit);
    }

    let mut value = 0i32;
    let result = getter(metrics, &mut value);
    if result != ADLX_OK {
        return fact_not_available(
            key,
            source_api,
            unit,
            format!("metric read failed: {}", adlx_result_name(result)),
        );
    }

    fact_ok(key, source_api, json!(value), unit)
}

unsafe fn read_gpu_string(
    gpu: *mut IADLXGPU,
    getter: unsafe extern "system" fn(*mut IADLXGPU, *mut *const c_char) -> AdlxResult,
) -> Option<String> {
    let mut ptr = ptr::null::<c_char>();
    let result = getter(gpu, &mut ptr);
    if result != ADLX_OK || ptr.is_null() {
        return None;
    }
    CStr::from_ptr(ptr).to_str().ok().map(str::to_string)
}

unsafe fn read_gpu_u32(
    gpu: *mut IADLXGPU,
    getter: unsafe extern "system" fn(*mut IADLXGPU, *mut AdlxUInt) -> AdlxResult,
) -> Option<u32> {
    let mut value = 0u32;
    let result = getter(gpu, &mut value);
    (result == ADLX_OK).then_some(value)
}

unsafe fn read_gpu_i32(
    gpu: *mut IADLXGPU,
    getter: unsafe extern "system" fn(*mut IADLXGPU, *mut AdlxInt) -> AdlxResult,
) -> Option<i32> {
    let mut value = 0i32;
    let result = getter(gpu, &mut value);
    (result == ADLX_OK).then_some(value)
}

fn classify_status(gpus: &[AdlxGpuRecord]) -> &'static str {
    if gpus
        .iter()
        .flat_map(|gpu| gpu.metrics.iter())
        .any(|metric| metric.state == "ok")
    {
        "ok"
    } else if gpus.iter().any(|gpu| !gpu.errors.is_empty()) {
        "error"
    } else {
        "unavailable"
    }
}

fn no_metrics_reason(gpus: &[AdlxGpuRecord]) -> String {
    for gpu in gpus {
        if let Some(error) = gpu.errors.first() {
            return error.clone();
        }
    }
    "ADLX did not expose any requested metrics for the detected AMD GPUs.".to_string()
}

fn push_gpu_snapshot_lines(lines: &mut Vec<String>, sample: &ProviderSample, gpu: &AdlxGpuRecord) {
    let name = gpu.adapter_name.as_deref().unwrap_or("(unnamed AMD GPU)");
    lines.push(format!("AMD ADLX device {}: {}", gpu.gpu_index, name));
    if let Some(unique_id) = gpu.unique_id {
        lines.push(format!("  Unique ID: {}", unique_id));
    }
    if let Some(vendor_id) = &gpu.vendor_id {
        if let Some(device_id) = &gpu.device_id {
            lines.push(format!(
                "  PCI IDs: vendor={}, device={}",
                vendor_id, device_id
            ));
        }
    }
    let runtime = sample
        .runtime_version
        .as_deref()
        .unwrap_or("version unavailable");
    lines.push(format!("  Runtime: initialized ({runtime})"));

    if let Some(total_vram_mb) = gpu.total_vram_mb {
        if let Some(vram_used_mb) = metric_i32(gpu, "gpu_vram_used_mb") {
            lines.push(format!(
                "  Memory: {} MiB / {} MiB",
                vram_used_mb, total_vram_mb
            ));
        } else {
            lines.push(format!("  Total VRAM: {} MiB", total_vram_mb));
        }
    }
    push_metric_line(
        lines,
        gpu,
        "gpu_activity_pct",
        "GPU activity",
        MetricDisplay::Float2("%"),
    );
    push_metric_line(
        lines,
        gpu,
        "gpu_clock_mhz",
        "GPU/core clock",
        MetricDisplay::Int("MHz"),
    );
    push_metric_line(
        lines,
        gpu,
        "memory_clock_mhz",
        "Memory clock",
        MetricDisplay::Int("MHz"),
    );
    push_metric_line(
        lines,
        gpu,
        "gpu_temperature_c",
        "GPU temperature",
        MetricDisplay::Float1(" C"),
    );
    push_metric_line(
        lines,
        gpu,
        "gpu_power_w",
        "GPU power",
        MetricDisplay::Float1(" W"),
    );
    push_metric_line(
        lines,
        gpu,
        "gpu_total_board_power_w",
        "Board power",
        MetricDisplay::Float1(" W"),
    );
    push_metric_line(
        lines,
        gpu,
        "gpu_fan_speed_rpm",
        "Fan speed",
        MetricDisplay::Int(" RPM"),
    );

    if let Some(error) = gpu.errors.first() {
        lines.push(format!("  Metrics: unavailable, {}", error));
    }
}

fn push_metric_line(
    lines: &mut Vec<String>,
    gpu: &AdlxGpuRecord,
    key: &'static str,
    label: &str,
    display: MetricDisplay,
) {
    if let Some(metric) = gpu.metrics.iter().find(|metric| metric.key == key) {
        match metric.state {
            "ok" => {
                if let Some(rendered) = render_metric(metric, display) {
                    lines.push(format!("  {}: {}", label, rendered));
                }
            }
            "not_available" => {
                let reason = metric
                    .error_message
                    .as_deref()
                    .unwrap_or("provider did not return a value");
                lines.push(format!("  {}: unavailable, {}", label, reason));
            }
            "error" => {
                let reason = metric
                    .error_message
                    .as_deref()
                    .unwrap_or("provider call failed");
                lines.push(format!("  {}: unavailable, {}", label, reason));
            }
            _ => {}
        }
    }
}

fn render_metric(metric: &MetricFact, display: MetricDisplay) -> Option<String> {
    match display {
        MetricDisplay::Float1(suffix) => metric
            .raw
            .as_f64()
            .map(|value| format!("{value:.1}{suffix}")),
        MetricDisplay::Float2(suffix) => metric
            .raw
            .as_f64()
            .map(|value| format!("{value:.2}{suffix}")),
        MetricDisplay::Int(suffix) => metric.raw.as_i64().map(|value| format!("{value}{suffix}")),
    }
}

fn metric_i32(gpu: &AdlxGpuRecord, key: &'static str) -> Option<i32> {
    gpu.metrics
        .iter()
        .find(|metric| metric.key == key && metric.state == "ok")
        .and_then(|metric| metric.raw.as_i64())
        .map(|value| value as i32)
}

fn primary_reason(sample: &ProviderSample) -> &str {
    sample
        .errors
        .first()
        .map(String::as_str)
        .or_else(|| {
            sample
                .gpus
                .iter()
                .flat_map(|gpu| gpu.errors.iter())
                .next()
                .map(String::as_str)
        })
        .unwrap_or("provider returned no additional details")
}

fn snapshot_status_label(sample: &ProviderSample) -> &'static str {
    if sample.status == "ok" {
        "available"
    } else {
        "unavailable"
    }
}

fn load_adlx_library() -> Result<AdlxLibrary, String> {
    let wide_name = to_wide(ADLX_DLL_NAME);
    let module = unsafe { LoadLibraryW(wide_name.as_ptr()) };
    let module = NonNull::new(module).ok_or_else(|| {
        format!(
            "AMD ADLX DLL not found via {}. Install Radeon Software with ADLX runtime support.",
            ADLX_DLL_NAME
        )
    })?;

    let dll_path = unsafe { module_path(module.as_ptr())? };
    let initialize = unsafe { required_symbol::<AdlxInitializeFn>(module, b"ADLXInitialize\0")? };
    let initialize2 = unsafe { optional_symbol::<AdlxInitialize2Fn>(module, b"ADLXInitialize2\0") };
    let terminate = unsafe { required_symbol::<AdlxTerminateFn>(module, b"ADLXTerminate\0")? };
    let query_version =
        unsafe { optional_symbol::<AdlxQueryVersionFn>(module, b"ADLXQueryVersion\0") };

    Ok(AdlxLibrary {
        module,
        dll_name: ADLX_DLL_NAME,
        dll_path,
        initialize,
        initialize2,
        terminate,
        query_version,
    })
}

fn query_runtime_version(library: &AdlxLibrary) -> Option<String> {
    let query_version = library.query_version?;
    let mut version_ptr = ptr::null::<c_char>();
    let result = unsafe { query_version(&mut version_ptr) };
    if result != ADLX_OK || version_ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(version_ptr) }
        .to_str()
        .ok()
        .map(str::to_string)
}

unsafe fn release_interface(ptr: *mut IADLXInterface) {
    if !ptr.is_null() {
        ((*(*ptr).p_vtbl).release)(ptr);
    }
}

unsafe fn required_symbol<T>(module: NonNull<c_void>, name: &[u8]) -> Result<T, String>
where
    T: Copy,
{
    let symbol = GetProcAddress(module.as_ptr(), name.as_ptr().cast());
    if symbol.is_null() {
        return Err(format!(
            "missing ADLX symbol {}",
            String::from_utf8_lossy(&name[..name.len().saturating_sub(1)])
        ));
    }
    Ok(std::mem::transmute_copy(&symbol))
}

unsafe fn optional_symbol<T>(module: NonNull<c_void>, name: &[u8]) -> Option<T>
where
    T: Copy,
{
    let symbol = GetProcAddress(module.as_ptr(), name.as_ptr().cast());
    if symbol.is_null() {
        None
    } else {
        Some(std::mem::transmute_copy(&symbol))
    }
}

unsafe fn module_path(module: *mut c_void) -> Result<String, String> {
    let mut buffer = [0u16; 260];
    let len = GetModuleFileNameW(module, buffer.as_mut_ptr(), buffer.len() as u32);
    if len == 0 {
        return Err("failed to query loaded ADLX module path".to_string());
    }
    Ok(String::from_utf16_lossy(&buffer[..len as usize]))
}

fn fact_ok(
    key: &'static str,
    source_api: &'static str,
    raw: Value,
    unit: Option<&'static str>,
) -> MetricFact {
    MetricFact {
        key,
        source_api,
        state: "ok",
        raw,
        unit,
        error_message: None,
    }
}

fn fact_unsupported(
    key: &'static str,
    source_api: &'static str,
    unit: Option<&'static str>,
) -> MetricFact {
    MetricFact {
        key,
        source_api,
        state: "unsupported",
        raw: Value::Null,
        unit,
        error_message: None,
    }
}

fn fact_not_available(
    key: &'static str,
    source_api: &'static str,
    unit: Option<&'static str>,
    error_message: String,
) -> MetricFact {
    MetricFact {
        key,
        source_api,
        state: "not_available",
        raw: Value::Null,
        unit,
        error_message: Some(error_message),
    }
}

fn fact_error(
    key: &'static str,
    source_api: &'static str,
    unit: Option<&'static str>,
    error_message: String,
) -> MetricFact {
    MetricFact {
        key,
        source_api,
        state: "error",
        raw: Value::Null,
        unit,
        error_message: Some(error_message),
    }
}

fn stats_string_field(raw: Option<&str>, source_api: &'static str) -> Value {
    raw.map(|value| json!({"raw": value, "source_api": source_api, "state": "ok"}))
        .unwrap_or_else(|| json!({"raw": null, "source_api": source_api, "state": "not_available"}))
}

fn stats_i32_field(raw: Option<i32>, source_api: &'static str) -> Value {
    raw.map(|value| json!({"raw": value, "source_api": source_api, "state": "ok"}))
        .unwrap_or_else(|| json!({"raw": null, "source_api": source_api, "state": "not_available"}))
}

fn stats_u32_field(
    raw: Option<u32>,
    source_api: &'static str,
    unit: Option<&'static str>,
) -> Value {
    raw.map(|value| json!({"raw": value, "source_api": source_api, "state": "ok", "unit": unit}))
        .unwrap_or_else(|| json!({"raw": null, "source_api": source_api, "state": "not_available", "unit": unit}))
}

fn unavailable_sample(
    sample_seq: u64,
    timestamp_unix_ms: u128,
    probe_attempted: bool,
    dll_name: Option<String>,
    dll_path: Option<String>,
    runtime_dll_state: &'static str,
    runtime_version: Option<String>,
    init_state: &'static str,
    devices_returned: Option<usize>,
    reason: String,
) -> ProviderSample {
    ProviderSample {
        wtg_version: env!("CARGO_PKG_VERSION"),
        source: SOURCE,
        telemetry_class: TELEMETRY_CLASS,
        provider: PROVIDER,
        provider_authority: PROVIDER_AUTHORITY,
        status: "unavailable",
        sample_seq,
        timestamp_unix_ms,
        probe_attempted,
        dll_name,
        dll_path,
        runtime_dll_state,
        runtime_version,
        init_state,
        adlx_initialized: false,
        devices_returned,
        gpus: Vec::new(),
        errors: vec![reason],
    }
}

fn error_sample(
    sample_seq: u64,
    timestamp_unix_ms: u128,
    probe_attempted: bool,
    dll_name: Option<String>,
    dll_path: Option<String>,
    runtime_dll_state: &'static str,
    runtime_version: Option<String>,
    init_state: &'static str,
    devices_returned: Option<usize>,
    reason: String,
) -> ProviderSample {
    ProviderSample {
        wtg_version: env!("CARGO_PKG_VERSION"),
        source: SOURCE,
        telemetry_class: TELEMETRY_CLASS,
        provider: PROVIDER,
        provider_authority: PROVIDER_AUTHORITY,
        status: "error",
        sample_seq,
        timestamp_unix_ms,
        probe_attempted,
        dll_name,
        dll_path,
        runtime_dll_state,
        runtime_version,
        init_state,
        adlx_initialized: false,
        devices_returned,
        gpus: Vec::new(),
        errors: vec![reason],
    }
}

fn adlx_result_name(code: i32) -> &'static str {
    match code {
        ADLX_OK => "ADLX_OK",
        ADLX_ALREADY_INITIALIZED => "ADLX_ALREADY_INITIALIZED",
        ADLX_FAIL => "ADLX_FAIL",
        ADLX_ADL_INIT_ERROR => "ADLX_ADL_INIT_ERROR",
        ADLX_NOT_FOUND => "ADLX_NOT_FOUND",
        ADLX_NOT_SUPPORTED => "ADLX_NOT_SUPPORTED",
        ADLX_GPU_INACTIVE => "ADLX_GPU_INACTIVE",
        ADLX_NOT_ACTIVE => "ADLX_NOT_ACTIVE",
        _ => "ADLX_UNKNOWN_RESULT",
    }
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn to_wide(value: &str) -> Vec<u16> {
    OsStr::new(value)
        .encode_wide()
        .chain(iter::once(0))
        .collect()
}

enum MetricDisplay {
    Float1(&'static str),
    Float2(&'static str),
    Int(&'static str),
}

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryW(lp_lib_file_name: *const u16) -> *mut c_void;
    fn FreeLibrary(h_lib_module: *mut c_void) -> i32;
    fn GetProcAddress(h_module: *mut c_void, lp_proc_name: *const c_char) -> *mut c_void;
    fn GetModuleFileNameW(h_module: *mut c_void, lp_filename: *mut u16, n_size: u32) -> u32;
}

#[cfg(test)]
mod tests {
    use super::{
        format_snapshot, sample_status, AdlxGpuRecord, MetricFact, ProviderSample, PROVIDER,
        PROVIDER_AUTHORITY, SOURCE, TELEMETRY_CLASS,
    };
    use serde_json::json;

    #[test]
    fn unavailable_snapshot_uses_provider_status_block() {
        let sample = ProviderSample {
            wtg_version: "0.3.0",
            source: SOURCE,
            telemetry_class: TELEMETRY_CLASS,
            provider: PROVIDER,
            provider_authority: PROVIDER_AUTHORITY,
            status: "unavailable",
            sample_seq: 0,
            timestamp_unix_ms: 0,
            probe_attempted: true,
            dll_name: None,
            dll_path: None,
            runtime_dll_state: "not found",
            runtime_version: None,
            init_state: "not attempted",
            adlx_initialized: false,
            devices_returned: None,
            gpus: Vec::new(),
            errors: vec!["ADLX runtime not present".to_string()],
        };

        let rendered = format_snapshot(&sample);
        assert!(rendered.contains("ADLX Provider:"));
        assert!(rendered.contains("Probe attempted: yes"));
        assert!(rendered.contains("Runtime/DLL: not found"));
        assert!(rendered.contains("Init: not attempted"));
        assert!(rendered.contains("Status: unavailable"));
        assert!(rendered.contains("ADLX runtime not present"));
        assert_eq!(sample_status(&sample), "unavailable");
    }

    #[test]
    fn ok_snapshot_renders_gpu_activity_without_fake_unavailable_lines() {
        let sample = ProviderSample {
            wtg_version: "0.3.0",
            source: SOURCE,
            telemetry_class: TELEMETRY_CLASS,
            provider: PROVIDER,
            provider_authority: PROVIDER_AUTHORITY,
            status: "ok",
            sample_seq: 0,
            timestamp_unix_ms: 0,
            probe_attempted: true,
            dll_name: Some("amdadlx64.dll".to_string()),
            dll_path: Some("C:\\Windows\\System32\\amdadlx64.dll".to_string()),
            runtime_dll_state: "found",
            runtime_version: Some("1.5.0".to_string()),
            init_state: "succeeded",
            adlx_initialized: true,
            devices_returned: Some(1),
            gpus: vec![AdlxGpuRecord {
                gpu_index: 0,
                adapter_name: Some("AMD Radeon(TM) Graphics".to_string()),
                vendor_id: Some("1002".to_string()),
                device_id: Some("1638".to_string()),
                unique_id: Some(7),
                total_vram_mb: Some(512),
                driver_path: None,
                pnp_string: None,
                metrics: vec![
                    MetricFact {
                        key: "gpu_activity_pct",
                        source_api: "IADLXGPUMetrics::GPUUsage",
                        state: "ok",
                        raw: json!(42.5),
                        unit: Some("percent"),
                        error_message: None,
                    },
                    MetricFact {
                        key: "gpu_vram_used_mb",
                        source_api: "IADLXGPUMetrics::GPUVRAM",
                        state: "ok",
                        raw: json!(128),
                        unit: Some("MiB"),
                        error_message: None,
                    },
                ],
                errors: Vec::new(),
            }],
            errors: Vec::new(),
        };

        let rendered = format_snapshot(&sample);
        assert!(rendered.contains("ADLX Provider:"));
        assert!(rendered.contains("Status: available"));
        assert!(rendered.contains("Runtime/DLL: found"));
        assert!(rendered.contains("Init: succeeded"));
        assert!(rendered.contains("Devices returned: 1"));
        assert!(rendered.contains("AMD ADLX device 0: AMD Radeon(TM) Graphics"));
        assert!(rendered.contains("GPU activity: 42.50%"));
        assert!(rendered.contains("Memory: 128 MiB / 512 MiB"));
    }
}
