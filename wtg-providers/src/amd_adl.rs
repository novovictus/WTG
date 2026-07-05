use std::ffi::{c_char, c_int, c_void, CStr, OsStr};
use std::iter;
use std::mem::{size_of, MaybeUninit};
use std::os::windows::ffi::OsStrExt;
use std::ptr::{self, NonNull};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{json, Value};

const SOURCE: &str = "wtg.provider.amd.adl";
const TELEMETRY_CLASS: &str = "provider_telemetry";
const PROVIDER: &str = "amd.adl";
const PROVIDER_AUTHORITY: &str = "AMD ADL";
const DEFAULT_INTERVAL_MS: u64 = 1000;
const ADL_OK: i32 = 0;
const ADL_ERR_NOT_SUPPORTED: i32 = -8;
const ADL_MAX_PATH: usize = 256;
const ADL_FANCTRL_SPEED_TYPE_PERCENT: i32 = 1;
const ADL_FANCTRL_SPEED_TYPE_RPM: i32 = 2;

type AdlMainMemoryAlloc = unsafe extern "C" fn(c_int) -> *mut c_void;
type AdlMainControlCreate = unsafe extern "C" fn(AdlMainMemoryAlloc, c_int) -> c_int;
type AdlMainControlDestroy = unsafe extern "C" fn() -> c_int;
type AdlAdapterNumberOfAdaptersGet = unsafe extern "C" fn(*mut c_int) -> c_int;
type AdlAdapterAdapterInfoGet = unsafe extern "C" fn(*mut AdapterInfo, c_int) -> c_int;
type AdlAdapterActiveGet = unsafe extern "C" fn(c_int, *mut c_int) -> c_int;
type AdlOverdriveCaps = unsafe extern "C" fn(c_int, *mut c_int, *mut c_int, *mut c_int) -> c_int;
type AdlOverdrive5CurrentActivityGet = unsafe extern "C" fn(c_int, *mut AdlPmActivity) -> c_int;
type AdlOverdrive5TemperatureGet = unsafe extern "C" fn(c_int, c_int, *mut AdlTemperature) -> c_int;
type AdlOverdrive5FanSpeedInfoGet =
    unsafe extern "C" fn(c_int, c_int, *mut AdlFanSpeedInfo) -> c_int;
type AdlOverdrive5FanSpeedGet = unsafe extern "C" fn(c_int, c_int, *mut AdlFanSpeedValue) -> c_int;
type AdlOverdrive5OdParametersGet = unsafe extern "C" fn(c_int, *mut AdlOdParameters) -> c_int;
type AdlAdapterMemoryInfoGet = unsafe extern "C" fn(c_int, *mut AdlMemoryInfo) -> c_int;
type AdlAdapterVideoBiosInfoGet = unsafe extern "C" fn(c_int, *mut AdlBiosInfo) -> c_int;
type AdlAdapterAsicFamilyTypeGet = unsafe extern "C" fn(c_int, *mut c_int, *mut c_int) -> c_int;
type AdlAdapterObservedClockInfoGet = unsafe extern "C" fn(c_int, *mut c_int, *mut c_int) -> c_int;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Once,
    Watch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliArgs {
    mode: Mode,
    interval_ms: u64,
}

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
    dll_name: Option<String>,
    dll_path: Option<String>,
    adl_initialized: bool,
    adl_return_codes: SampleReturnCodes,
    telemetry_exports_matched: usize,
    physical_adapter_group_count: usize,
    amd_physical_adapter_count: usize,
    non_amd_physical_adapter_count: usize,
    extended_amd_discovery_run_count: usize,
    physical_adapter_groups: Vec<PhysicalAdapterGroup>,
    adapters: Vec<AdlAdapterRecord>,
    errors: Vec<String>,
}

#[derive(Debug, Serialize)]
struct AdlAdapterRecord {
    adl_raw_identity: AdlRawIdentity,
    physical_adapter_key: String,
    physical_adapter_role: &'static str,
    physical_adapter_primary_index: i32,
    logical_record_kind: &'static str,
    adl_calls: Vec<AdlApiCall>,
    provider_warnings: Vec<String>,
    provider_errors: Vec<String>,
}

#[derive(Debug, Serialize)]
struct PhysicalAdapterGroup {
    physical_adapter_key: String,
    physical_vendor_id: i32,
    physical_adapter_primary_index: i32,
    logical_record_kind: &'static str,
    logical_adapter_indices: Vec<i32>,
    logical_record_count: usize,
    extended_amd_discovery_attempted: bool,
}

#[derive(Debug, Serialize)]
struct AdlRawIdentity {
    adl_adapter_index: i32,
    adl_udid: String,
    adl_adapter_name: String,
    adl_display_name: String,
    adl_vendor_id: i32,
    adl_present: bool,
    adl_exists: bool,
    adl_bus_number: i32,
    adl_device_number: i32,
    adl_function_number: i32,
    adl_driver_path: String,
    adl_driver_path_ext: String,
    adl_pnp_string: String,
    adl_os_display_index: i32,
}

#[derive(Debug, Serialize)]
struct AdlApiCall {
    metric_key: &'static str,
    source_api: &'static str,
    state: &'static str,
    adl_return_code: Option<i32>,
    raw: Value,
    unit: Option<&'static str>,
    error_message: Option<String>,
}

#[derive(Debug, Serialize)]
struct SampleReturnCodes {
    adl_main_control_create: Option<i32>,
    adl_adapter_number_of_adapters_get: Option<i32>,
    adl_adapter_adapter_info_get: Option<i32>,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct AdapterInfo {
    i_size: c_int,
    i_adapter_index: c_int,
    str_udid: [c_char; ADL_MAX_PATH],
    i_bus_number: c_int,
    i_device_number: c_int,
    i_function_number: c_int,
    i_vendor_id: c_int,
    str_adapter_name: [c_char; ADL_MAX_PATH],
    str_display_name: [c_char; ADL_MAX_PATH],
    i_present: c_int,
    i_exist: c_int,
    str_driver_path: [c_char; ADL_MAX_PATH],
    str_driver_path_ext: [c_char; ADL_MAX_PATH],
    str_pnp_string: [c_char; ADL_MAX_PATH],
    i_os_display_index: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct AdlPmActivity {
    i_size: c_int,
    i_engine_clock: c_int,
    i_memory_clock: c_int,
    i_vddc: c_int,
    i_activity_percent: c_int,
    i_current_performance_level: c_int,
    i_current_bus_speed: c_int,
    i_current_bus_lanes: c_int,
    i_maximum_bus_lanes: c_int,
    i_reserved: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct AdlTemperature {
    i_size: c_int,
    i_temperature: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct AdlFanSpeedInfo {
    i_size: c_int,
    i_flags: c_int,
    i_min_percent: c_int,
    i_max_percent: c_int,
    i_min_rpm: c_int,
    i_max_rpm: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct AdlFanSpeedValue {
    i_size: c_int,
    i_speed_type: c_int,
    i_fan_speed: c_int,
    i_flags: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct AdlOdParameterRange {
    i_min: c_int,
    i_max: c_int,
    i_step: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct AdlOdParameters {
    i_size: c_int,
    i_number_of_performance_levels: c_int,
    i_activity_reporting_supported: c_int,
    i_discrete_performance_levels: c_int,
    i_reserved: c_int,
    s_engine_clock: AdlOdParameterRange,
    s_memory_clock: AdlOdParameterRange,
    s_vddc: AdlOdParameterRange,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct AdlMemoryInfo {
    str_memory_type: [c_char; ADL_MAX_PATH],
    i_memory_size: c_int,
    str_memory_bandwidth: [c_char; ADL_MAX_PATH],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct AdlBiosInfo {
    str_part_number: [c_char; ADL_MAX_PATH],
    str_version: [c_char; ADL_MAX_PATH],
    str_date: [c_char; ADL_MAX_PATH],
}

struct AdlLibrary {
    module: NonNull<c_void>,
    dll_name: String,
    dll_path: String,
    main_control_create: AdlMainControlCreate,
    main_control_destroy: AdlMainControlDestroy,
    adapter_number_of_adapters_get: AdlAdapterNumberOfAdaptersGet,
    adapter_info_get: AdlAdapterAdapterInfoGet,
    adapter_active_get: Option<AdlAdapterActiveGet>,
    overdrive_caps: Option<AdlOverdriveCaps>,
    overdrive5_current_activity_get: Option<AdlOverdrive5CurrentActivityGet>,
    overdrive5_temperature_get: Option<AdlOverdrive5TemperatureGet>,
    overdrive5_fan_speed_info_get: Option<AdlOverdrive5FanSpeedInfoGet>,
    overdrive5_fan_speed_get: Option<AdlOverdrive5FanSpeedGet>,
    overdrive5_od_parameters_get: Option<AdlOverdrive5OdParametersGet>,
    adapter_memory_info_get: Option<AdlAdapterMemoryInfoGet>,
    adapter_video_bios_info_get: Option<AdlAdapterVideoBiosInfoGet>,
    adapter_asic_family_type_get: Option<AdlAdapterAsicFamilyTypeGet>,
    adapter_observed_clock_info_get: Option<AdlAdapterObservedClockInfoGet>,
}

impl Drop for AdlLibrary {
    fn drop(&mut self) {
        unsafe {
            FreeLibrary(self.module.as_ptr());
        }
    }
}

struct AdlSession<'a> {
    library: &'a AdlLibrary,
    initialized: bool,
}

struct AdlInitError {
    adl_main_control_create: i32,
    message: String,
}

struct EnumerationError {
    adl_adapter_number_of_adapters_get: Option<i32>,
    adl_adapter_adapter_info_get: Option<i32>,
    message: String,
}

impl<'a> Drop for AdlSession<'a> {
    fn drop(&mut self) {
        if self.initialized {
            unsafe {
                (self.library.main_control_destroy)();
            }
        }
    }
}

pub fn run_cli<I>(args: I) -> i32
where
    I: IntoIterator<Item = String>,
{
    let args = match parse_args(args) {
        Ok(args) => args,
        Err(err) => return usage_error(&err),
    };

    match args.mode {
        Mode::Once => {
            emit_sample(0);
            0
        }
        Mode::Watch => {
            let mut sample_seq = 0u64;
            loop {
                emit_sample(sample_seq);
                sample_seq = sample_seq.saturating_add(1);
                thread::sleep(Duration::from_millis(args.interval_ms));
            }
        }
    }
}

fn emit_sample(sample_seq: u64) {
    let sample = collect_once(sample_seq);
    println!(
        "{}",
        serde_json::to_string(&sample).expect("provider JSON serialization should succeed")
    );
}

pub fn collect_once(sample_seq: u64) -> ProviderSample {
    // TODO: Follow-up: add a long-lived AMD ADL provider session for watch/eGUI to avoid
    // repeated load/init/free churn on each collect_once call.
    match AdlLibrary::load() {
        Ok(library) => match AdlSession::create(&library) {
            Ok(_session) => match enumerate_adapters(&library) {
                Ok(enumeration) if enumeration.adapters.is_empty() => ProviderSample {
                    wtg_version: env!("CARGO_PKG_VERSION"),
                    source: SOURCE,
                    telemetry_class: TELEMETRY_CLASS,
                    provider: PROVIDER,
                    provider_authority: PROVIDER_AUTHORITY,
                    status: "unavailable",
                    sample_seq,
                    timestamp_unix_ms: now_unix_ms(),
                    dll_name: Some(library.dll_name.clone()),
                    dll_path: Some(library.dll_path.clone()),
                    adl_initialized: true,
                    adl_return_codes: SampleReturnCodes {
                        adl_main_control_create: Some(ADL_OK),
                        adl_adapter_number_of_adapters_get: Some(
                            enumeration.adapter_number_of_adapters_get,
                        ),
                        adl_adapter_adapter_info_get: Some(enumeration.adapter_adapter_info_get),
                    },
                    telemetry_exports_matched: library.telemetry_exports_matched(),
                    physical_adapter_group_count: enumeration.physical_adapter_group_count,
                    amd_physical_adapter_count: enumeration.amd_physical_adapter_count,
                    non_amd_physical_adapter_count: enumeration.non_amd_physical_adapter_count,
                    extended_amd_discovery_run_count: enumeration.extended_amd_discovery_run_count,
                    physical_adapter_groups: enumeration.physical_adapter_groups,
                    adapters: enumeration.adapters,
                    errors: vec!["ADL initialized but returned zero adapters.".to_string()],
                },
                Ok(enumeration) => ProviderSample {
                    wtg_version: env!("CARGO_PKG_VERSION"),
                    source: SOURCE,
                    telemetry_class: TELEMETRY_CLASS,
                    provider: PROVIDER,
                    provider_authority: PROVIDER_AUTHORITY,
                    status: "ok",
                    sample_seq,
                    timestamp_unix_ms: now_unix_ms(),
                    dll_name: Some(library.dll_name.clone()),
                    dll_path: Some(library.dll_path.clone()),
                    adl_initialized: true,
                    adl_return_codes: SampleReturnCodes {
                        adl_main_control_create: Some(ADL_OK),
                        adl_adapter_number_of_adapters_get: Some(
                            enumeration.adapter_number_of_adapters_get,
                        ),
                        adl_adapter_adapter_info_get: Some(enumeration.adapter_adapter_info_get),
                    },
                    telemetry_exports_matched: library.telemetry_exports_matched(),
                    physical_adapter_group_count: enumeration.physical_adapter_group_count,
                    amd_physical_adapter_count: enumeration.amd_physical_adapter_count,
                    non_amd_physical_adapter_count: enumeration.non_amd_physical_adapter_count,
                    extended_amd_discovery_run_count: enumeration.extended_amd_discovery_run_count,
                    physical_adapter_groups: enumeration.physical_adapter_groups,
                    adapters: enumeration.adapters,
                    errors: Vec::new(),
                },
                Err(err) => ProviderSample {
                    wtg_version: env!("CARGO_PKG_VERSION"),
                    source: SOURCE,
                    telemetry_class: TELEMETRY_CLASS,
                    provider: PROVIDER,
                    provider_authority: PROVIDER_AUTHORITY,
                    status: "error",
                    sample_seq,
                    timestamp_unix_ms: now_unix_ms(),
                    dll_name: Some(library.dll_name.clone()),
                    dll_path: Some(library.dll_path.clone()),
                    adl_initialized: true,
                    adl_return_codes: SampleReturnCodes {
                        adl_main_control_create: Some(ADL_OK),
                        adl_adapter_number_of_adapters_get: err.adl_adapter_number_of_adapters_get,
                        adl_adapter_adapter_info_get: err.adl_adapter_adapter_info_get,
                    },
                    telemetry_exports_matched: library.telemetry_exports_matched(),
                    physical_adapter_group_count: 0,
                    amd_physical_adapter_count: 0,
                    non_amd_physical_adapter_count: 0,
                    extended_amd_discovery_run_count: 0,
                    physical_adapter_groups: Vec::new(),
                    adapters: Vec::new(),
                    errors: vec![err.message],
                },
            },
            Err(err) => ProviderSample {
                wtg_version: env!("CARGO_PKG_VERSION"),
                source: SOURCE,
                telemetry_class: TELEMETRY_CLASS,
                provider: PROVIDER,
                provider_authority: PROVIDER_AUTHORITY,
                status: "error",
                sample_seq,
                timestamp_unix_ms: now_unix_ms(),
                dll_name: Some(library.dll_name.clone()),
                dll_path: Some(library.dll_path.clone()),
                adl_initialized: false,
                adl_return_codes: SampleReturnCodes {
                    adl_main_control_create: Some(err.adl_main_control_create),
                    adl_adapter_number_of_adapters_get: None,
                    adl_adapter_adapter_info_get: None,
                },
                telemetry_exports_matched: library.telemetry_exports_matched(),
                physical_adapter_group_count: 0,
                amd_physical_adapter_count: 0,
                non_amd_physical_adapter_count: 0,
                extended_amd_discovery_run_count: 0,
                physical_adapter_groups: Vec::new(),
                adapters: Vec::new(),
                errors: vec![err.message],
            },
        },
        Err(err) => ProviderSample {
            wtg_version: env!("CARGO_PKG_VERSION"),
            source: SOURCE,
            telemetry_class: TELEMETRY_CLASS,
            provider: PROVIDER,
            provider_authority: PROVIDER_AUTHORITY,
            status: "unavailable",
            sample_seq,
            timestamp_unix_ms: now_unix_ms(),
            dll_name: None,
            dll_path: None,
            adl_initialized: false,
            adl_return_codes: SampleReturnCodes {
                adl_main_control_create: None,
                adl_adapter_number_of_adapters_get: None,
                adl_adapter_adapter_info_get: None,
            },
            telemetry_exports_matched: 0,
            physical_adapter_group_count: 0,
            amd_physical_adapter_count: 0,
            non_amd_physical_adapter_count: 0,
            extended_amd_discovery_run_count: 0,
            physical_adapter_groups: Vec::new(),
            adapters: Vec::new(),
            errors: vec![err],
        },
    }
}

pub fn provider_source() -> &'static str {
    SOURCE
}

pub fn telemetry_class() -> &'static str {
    TELEMETRY_CLASS
}

pub fn format_snapshot(sample: &ProviderSample) -> String {
    let reason = sample
        .errors
        .first()
        .map(String::as_str)
        .unwrap_or("provider returned no additional details");
    match sample.status {
        "ok" => {
            let mut lines = Vec::new();
            lines.push(format!(
                "ADL adapter records returned: {}",
                sample.adapters.len()
            ));
            lines.push(format!(
                "Physical adapter groups: {}",
                sample.physical_adapter_group_count
            ));
            lines.push(format!(
                "AMD physical adapters: {}",
                sample.amd_physical_adapter_count
            ));
            lines.push(format!(
                "Non-AMD physical adapters seen through ADL: {}",
                sample.non_amd_physical_adapter_count
            ));
            lines.push(format!(
                "Extended AMD discovery ran: {}",
                sample.extended_amd_discovery_run_count
            ));
            lines.push(String::new());

            if sample.physical_adapter_groups.is_empty() {
                lines.push("No physical adapter groups returned by ADL.".to_string());
            } else {
                for (index, group) in sample.physical_adapter_groups.iter().enumerate() {
                    push_snapshot_group_lines(&mut lines, sample, group, index);
                    if index + 1 != sample.physical_adapter_groups.len() {
                        lines.push(String::new());
                    }
                }
            }

            lines.join("\n")
        }
        "unavailable" => format!("Status: unavailable\nReason: {}", reason),
        "error" => format!("Status: error\nReason: {}", reason),
        other => format!("Status: {}\nReason: {}", other, reason),
    }
}

pub fn format_watch_sample(sample: &ProviderSample) -> String {
    let reason = sample
        .errors
        .first()
        .map(String::as_str)
        .unwrap_or("provider returned no additional details");

    match sample.status {
        "ok" => {
            let mut lines = Vec::new();
            lines.push(format!("sample_seq: {}", sample.sample_seq));
            lines.push(format!(
                "ADL records: {} | physical adapter groups: {} | AMD: {} | non-AMD: {} | extended AMD probes: {}",
                sample.adapters.len(),
                sample.physical_adapter_group_count,
                sample.amd_physical_adapter_count,
                sample.non_amd_physical_adapter_count,
                sample.extended_amd_discovery_run_count
            ));
            lines.push(String::new());

            if sample.physical_adapter_groups.is_empty() {
                lines.push("No physical adapter groups returned by ADL.".to_string());
            } else {
                for (index, group) in sample.physical_adapter_groups.iter().enumerate() {
                    if index > 0 {
                        lines.push(String::new());
                    }
                    push_watch_group_lines(&mut lines, sample, group);
                }
            }

            lines.join("\n")
        }
        "unavailable" => format!(
            "sample_seq: {}\nstatus: unavailable\nreason: {}",
            sample.sample_seq, reason
        ),
        "error" => format!(
            "sample_seq: {}\nstatus: error\nreason: {}",
            sample.sample_seq, reason
        ),
        other => format!(
            "sample_seq: {}\nstatus: {}\nreason: {}",
            sample.sample_seq, other, reason
        ),
    }
}

pub fn format_stats_snapshot_json(sample: &ProviderSample, tick_seq: u64, tick_ts: &str) -> String {
    let adapters = sample
        .physical_adapter_groups
        .iter()
        .map(|group| stats_adapter_json(sample, group))
        .collect::<Vec<_>>();
    let summary = optional_call_summary(sample);
    let payload = json!({
        "adapters": adapters,
        "adl": {
            "adapter_record_count": stats_number_field(
                sample.adapters.len(),
                "ADL_Adapter_AdapterInfo_Get",
                "ok",
                None,
                None,
            ),
            "physical_adapter_group_count": stats_number_field(
                sample.physical_adapter_group_count,
                "wtg.adl.physical_grouping",
                "ok",
                None,
                None,
            ),
            "amd_physical_adapter_count": stats_number_field(
                sample.amd_physical_adapter_count,
                "wtg.adl.physical_grouping",
                "ok",
                None,
                None,
            ),
            "non_amd_physical_adapter_count": stats_number_field(
                sample.non_amd_physical_adapter_count,
                "wtg.adl.physical_grouping",
                "ok",
                None,
                None,
            ),
            "extended_discovery_run_count": stats_number_field(
                sample.extended_amd_discovery_run_count,
                "wtg.adl.extended_discovery",
                "ok",
                None,
                None,
            ),
            "telemetry_exports_matched": stats_number_field(
                sample.telemetry_exports_matched,
                "wtg.adl.export_matching",
                "ok",
                None,
                None,
            ),
            "optional_calls_attempted": stats_number_field(
                summary.attempted,
                "wtg.adl.optional_calls",
                "ok",
                None,
                None,
            ),
            "optional_calls_ok": stats_number_field(
                summary.ok,
                "wtg.adl.optional_calls",
                "ok",
                None,
                None,
            ),
            "optional_calls_unsupported": stats_number_field(
                summary.unsupported,
                "wtg.adl.optional_calls",
                "ok",
                None,
                None,
            ),
            "optional_calls_not_available": stats_number_field(
                summary.not_available,
                "wtg.adl.optional_calls",
                "ok",
                None,
                None,
            ),
            "optional_calls_error": stats_number_field(
                summary.error,
                "wtg.adl.optional_calls",
                "ok",
                None,
                None,
            )
        },
        "provider": "amd",
        "provider_authority": PROVIDER_AUTHORITY,
        "provider_source": SOURCE,
        "schema": "wtg.amd_adl.stats.v1",
        "telemetry_class": TELEMETRY_CLASS,
        "tick_seq": tick_seq,
        "tick_ts": tick_ts,
        "timestamp_unix_ms": sample.timestamp_unix_ms,
        "wtg_version": sample.wtg_version
    });

    serde_json::to_string_pretty(&payload).expect("ADL stats JSON serialization should succeed")
}

pub fn format_probe_snapshot(sample: &ProviderSample) -> String {
    let reason = sample
        .errors
        .first()
        .map(String::as_str)
        .unwrap_or("provider returned no additional details");
    let mut lines = Vec::new();
    lines.push("[probe] provider=amd_adl".to_string());
    lines.push(format!("wtg.version: {}", sample.wtg_version));
    lines.push(format!("provider.authority: {}", sample.provider_authority));
    lines.push(format!("provider.source: {}", sample.source));
    lines.push(format!("telemetry.class: {}", sample.telemetry_class));
    lines.push(format!("adl.adapter_records: {}", sample.adapters.len()));
    lines.push(format!(
        "adl.physical_adapter_groups: {}",
        sample.physical_adapter_group_count
    ));
    lines.push(format!(
        "adl.amd_physical_adapters: {}",
        sample.amd_physical_adapter_count
    ));
    lines.push(format!(
        "adl.non_amd_physical_adapters: {}",
        sample.non_amd_physical_adapter_count
    ));
    lines.push(format!(
        "adl.telemetry_exports_matched: {}",
        sample.telemetry_exports_matched
    ));
    let summary = optional_call_summary(sample);
    lines.push(format!(
        "adl.optional_calls_attempted: {}",
        summary.attempted
    ));
    lines.push(format!("adl.optional_calls.ok: {}", summary.ok));
    lines.push(format!(
        "adl.optional_calls.unsupported: {}",
        summary.unsupported
    ));
    lines.push(format!(
        "adl.optional_calls.not_available: {}",
        summary.not_available
    ));
    lines.push(format!("adl.optional_calls.error: {}", summary.error));
    if sample.status != "ok" {
        lines.push(format!("provider.status: {}", sample.status));
        lines.push(format!("reason: {reason}"));
        return lines.join("\n");
    }

    for (index, group) in sample.physical_adapter_groups.iter().enumerate() {
        lines.push(String::new());
        lines.push(format!("[probe] adapter={index}"));
        push_probe_group_lines(&mut lines, sample, group);
    }

    lines.join("\n")
}

fn adapter_active_value(adapter: &AdlAdapterRecord) -> Option<bool> {
    adapter
        .adl_calls
        .iter()
        .find(|call| call.metric_key == "adapter_active")
        .and_then(|call| call.raw.get("value"))
        .and_then(Value::as_bool)
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn yes_no_or_unavailable(value: Option<bool>) -> &'static str {
    match value {
        Some(value) => yes_no(value),
        None => "unavailable",
    }
}

fn push_snapshot_group_lines(
    lines: &mut Vec<String>,
    sample: &ProviderSample,
    group: &PhysicalAdapterGroup,
    index: usize,
) {
    let Some(adapter) = primary_adapter_for_group(sample, group) else {
        lines.push(format!("ADL adapter group {index}: missing primary record"));
        return;
    };

    lines.push(format!(
        "ADL adapter group {index}: {}",
        adapter.adl_raw_identity.adl_adapter_name
    ));
    lines.push(format!("  Physical key: {}", group.physical_adapter_key));
    lines.push(format!(
        "  Logical ADL records: {}",
        group
            .logical_adapter_indices
            .iter()
            .map(i32::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    ));

    if group.physical_vendor_id != 1002 {
        lines.push("  Vendor scope: topology only".to_string());
        return;
    }

    lines.push(format!(
        "  Active: {}",
        yes_no_or_unavailable(adapter_active_value(adapter))
    ));
    if let Some(raw) = call_raw(adapter, "overdrive5_current_activity") {
        if let Some(activity_percent) = raw.get("activity_percent").and_then(Value::as_i64) {
            lines.push(format!("  Activity: {activity_percent}%"));
        }
        if let Some(engine_clock_10khz) = raw.get("engine_clock_10khz").and_then(Value::as_i64) {
            lines.push(format!(
                "  Engine clock: {:.1} MHz",
                (engine_clock_10khz as f64) / 100.0
            ));
        }
        if let Some(memory_clock_10khz) = raw.get("memory_clock_10khz").and_then(Value::as_i64) {
            lines.push(format!(
                "  Memory clock: {:.1} MHz",
                (memory_clock_10khz as f64) / 100.0
            ));
        }
        if let (Some(current_bus_speed), Some(current_bus_lanes), Some(maximum_bus_lanes)) = (
            raw.get("current_bus_speed").and_then(Value::as_i64),
            raw.get("current_bus_lanes").and_then(Value::as_i64),
            raw.get("maximum_bus_lanes").and_then(Value::as_i64),
        ) {
            lines.push(format!(
                "  Bus: {current_bus_speed} x{current_bus_lanes} / max x{maximum_bus_lanes}"
            ));
        }
    }
    if let Some(raw) = call_raw(adapter, "adapter_video_bios_info") {
        if let Some(version) = raw.get("version").and_then(Value::as_str) {
            if !version.is_empty() {
                lines.push(format!("  VBIOS: {version}"));
            }
        }
    }
    let unavailable = unavailable_metric_labels(adapter);
    if !unavailable.is_empty() {
        lines.push(format!("  Unavailable: {}", unavailable.join(", ")));
    }
}

fn push_probe_group_lines(
    lines: &mut Vec<String>,
    sample: &ProviderSample,
    group: &PhysicalAdapterGroup,
) {
    let Some(adapter) = primary_adapter_for_group(sample, group) else {
        lines.push("adapter.status: missing_primary_record".to_string());
        return;
    };

    lines.push(format!(
        "adapter.name: {}",
        adapter.adl_raw_identity.adl_adapter_name
    ));
    lines.push(format!(
        "adapter.physical_key: {}",
        group.physical_adapter_key
    ));
    lines.push(format!(
        "adapter.vendor_kind: {}",
        if group.physical_vendor_id == 1002 {
            "AMD"
        } else {
            "non-AMD"
        }
    ));
    lines.push(format!(
        "adapter.logical_adl_records: {}",
        group
            .logical_adapter_indices
            .iter()
            .map(i32::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    ));

    if group.physical_vendor_id != 1002 {
        lines.push("adapter.vendor_scope: topology_only".to_string());
        return;
    }

    lines.push(format!(
        "adapter.active: {}",
        yes_no_or_unavailable(adapter_active_value(adapter))
    ));
    if let Some(raw) = call_raw(adapter, "overdrive5_current_activity") {
        if let Some(activity_percent) = raw.get("activity_percent").and_then(Value::as_i64) {
            lines.push(format!("activity.pct: {activity_percent}"));
        }
        if let Some(engine_clock_10khz) = raw.get("engine_clock_10khz").and_then(Value::as_i64) {
            lines.push(format!(
                "clock.engine_mhz: {:.1}",
                (engine_clock_10khz as f64) / 100.0
            ));
        }
        if let Some(memory_clock_10khz) = raw.get("memory_clock_10khz").and_then(Value::as_i64) {
            lines.push(format!(
                "clock.memory_mhz: {:.1}",
                (memory_clock_10khz as f64) / 100.0
            ));
        }
        if let (Some(current_bus_speed), Some(current_bus_lanes)) = (
            raw.get("current_bus_speed").and_then(Value::as_i64),
            raw.get("current_bus_lanes").and_then(Value::as_i64),
        ) {
            lines.push(format!(
                "bus.current: {current_bus_speed} x{current_bus_lanes}"
            ));
        }
        if let Some(maximum_bus_lanes) = raw.get("maximum_bus_lanes").and_then(Value::as_i64) {
            lines.push(format!("bus.max: x{maximum_bus_lanes}"));
        }
    }
    if let Some(raw) = call_raw(adapter, "adapter_observed_clock_info") {
        if let Some(core_clock_10khz) = raw.get("core_clock_10khz").and_then(Value::as_i64) {
            lines.push(format!(
                "clock.observed_core_mhz: {:.1}",
                (core_clock_10khz as f64) / 100.0
            ));
        }
        if let Some(memory_clock_10khz) = raw.get("memory_clock_10khz").and_then(Value::as_i64) {
            lines.push(format!(
                "clock.observed_memory_mhz: {:.1}",
                (memory_clock_10khz as f64) / 100.0
            ));
        }
    }
    if let Some(raw) = call_raw(adapter, "adapter_video_bios_info") {
        if let Some(version) = raw.get("version").and_then(Value::as_str) {
            if !version.is_empty() {
                lines.push(format!("vbios.version: {version}"));
            }
        }
        if let Some(part_number) = raw.get("part_number").and_then(Value::as_str) {
            if !part_number.is_empty() {
                lines.push(format!("vbios.part_number: {part_number}"));
            }
        }
        if let Some(date) = raw.get("date").and_then(Value::as_str) {
            if !date.is_empty() {
                lines.push(format!("vbios.date: {date}"));
            }
        }
    }
    if let Some(raw) = call_raw(adapter, "adapter_asic_family_type") {
        if let Some(asic_family_type) = raw.get("asic_family_type").and_then(Value::as_i64) {
            lines.push(format!("asic.family_type: {asic_family_type}"));
        }
        if let Some(valids) = raw.get("valids").and_then(Value::as_i64) {
            lines.push(format!("asic.valids: {valids}"));
        }
    }
    let unavailable = unavailable_metric_labels(adapter);
    if !unavailable.is_empty() {
        lines.push(format!("unavailable: {}", unavailable.join(", ")));
    }
}

fn push_watch_group_lines(
    lines: &mut Vec<String>,
    sample: &ProviderSample,
    group: &PhysicalAdapterGroup,
) {
    let Some(primary_adapter) = sample.adapters.iter().find(|adapter| {
        adapter.adl_raw_identity.adl_adapter_index == group.physical_adapter_primary_index
    }) else {
        lines.push(format!("Unknown adapter [{}]", group.physical_adapter_key));
        lines.push("  provider formatting error: primary ADL record missing".to_string());
        return;
    };

    lines.push(format!(
        "{} [{}]",
        primary_adapter.adl_raw_identity.adl_adapter_name, group.physical_adapter_key
    ));

    if group.physical_vendor_id != 1002 {
        lines.push("  seen by ADL only; AMD extended discovery skipped".to_string());
        lines.push(format!(
            "  ADL logical records: {}",
            group
                .logical_adapter_indices
                .iter()
                .map(i32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ));
        return;
    }

    lines.push(format!(
        "  active: {}",
        yes_no_or_unavailable(adapter_active_value(primary_adapter))
    ));

    if let Some(raw) = call_raw(primary_adapter, "overdrive5_current_activity") {
        if let Some(activity_percent) = raw.get("activity_percent").and_then(Value::as_i64) {
            lines.push(format!("  activity: {activity_percent}%"));
        }
        if let Some(engine_clock_10khz) = raw.get("engine_clock_10khz").and_then(Value::as_i64) {
            lines.push(format!(
                "  engine clock: {:.1} MHz",
                (engine_clock_10khz as f64) / 100.0
            ));
        }
        if let Some(memory_clock_10khz) = raw.get("memory_clock_10khz").and_then(Value::as_i64) {
            lines.push(format!(
                "  memory clock: {:.1} MHz",
                (memory_clock_10khz as f64) / 100.0
            ));
        }
        if let Some(perf_level) = raw.get("current_performance_level").and_then(Value::as_i64) {
            lines.push(format!("  perf level raw: {perf_level}"));
        }
        if let (Some(current_bus_speed), Some(current_bus_lanes), Some(maximum_bus_lanes)) = (
            raw.get("current_bus_speed").and_then(Value::as_i64),
            raw.get("current_bus_lanes").and_then(Value::as_i64),
            raw.get("maximum_bus_lanes").and_then(Value::as_i64),
        ) {
            lines.push(format!(
                "  bus: {current_bus_speed} x{current_bus_lanes} / max x{maximum_bus_lanes}"
            ));
        }
    }

    if let Some(raw) = call_raw(primary_adapter, "adapter_observed_clock_info") {
        if let Some(core_clock_10khz) = raw.get("core_clock_10khz").and_then(Value::as_i64) {
            lines.push(format!(
                "  observed core clock: {:.1} MHz",
                (core_clock_10khz as f64) / 100.0
            ));
        }
    }

    if let Some(raw) = call_raw(primary_adapter, "adapter_video_bios_info") {
        let version = raw.get("version").and_then(Value::as_str).unwrap_or("");
        let part_number = raw.get("part_number").and_then(Value::as_str).unwrap_or("");
        let date = raw.get("date").and_then(Value::as_str).unwrap_or("");
        if !version.is_empty() || !part_number.is_empty() || !date.is_empty() {
            lines.push(format!("  vbios: {version} | {part_number} | {date}"));
        }
    }

    if let Some(raw) = call_raw(primary_adapter, "adapter_asic_family_type") {
        let asic_family_type = raw
            .get("asic_family_type")
            .and_then(Value::as_i64)
            .unwrap_or_default();
        let valids = raw
            .get("valids")
            .and_then(Value::as_i64)
            .unwrap_or_default();
        lines.push(format!("  asic family: {asic_family_type} valid={valids}"));
    }

    let unavailable = unavailable_metric_labels(primary_adapter);
    if !unavailable.is_empty() {
        lines.push(format!("  unavailable: {}", unavailable.join(", ")));
    }
}

fn call_raw<'a>(adapter: &'a AdlAdapterRecord, metric_key: &str) -> Option<&'a Value> {
    adapter
        .adl_calls
        .iter()
        .find(|call| call.metric_key == metric_key && call.state == "ok")
        .map(|call| &call.raw)
}

fn unavailable_metric_labels(adapter: &AdlAdapterRecord) -> Vec<&'static str> {
    let mut unavailable = Vec::new();
    for (metric_key, label) in [
        ("overdrive_caps", "overdrive caps"),
        ("overdrive5_temperature", "temp"),
        ("overdrive5_fan_speed_info", "fan"),
        ("adapter_memory_info", "memory info"),
    ] {
        if metric_state(adapter, metric_key).is_some_and(|state| state != "ok") {
            unavailable.push(label);
        }
    }

    unavailable
}

fn metric_state<'a>(adapter: &'a AdlAdapterRecord, metric_key: &str) -> Option<&'a str> {
    adapter
        .adl_calls
        .iter()
        .find(|call| call.metric_key == metric_key)
        .map(|call| call.state)
}

fn stats_field(
    raw: Value,
    source_api: &'static str,
    state: &'static str,
    unit: Option<&'static str>,
    error_message: Option<String>,
) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("raw".to_string(), raw);
    object.insert(
        "source_api".to_string(),
        Value::String(source_api.to_string()),
    );
    object.insert("state".to_string(), Value::String(state.to_string()));
    if let Some(unit) = unit {
        object.insert("unit".to_string(), Value::String(unit.to_string()));
    }
    if let Some(error_message) = error_message {
        object.insert("error_message".to_string(), Value::String(error_message));
    }
    Value::Object(object)
}

fn stats_number_field(
    raw: usize,
    source_api: &'static str,
    state: &'static str,
    unit: Option<&'static str>,
    error_message: Option<String>,
) -> Value {
    stats_field(
        Value::Number(serde_json::Number::from(raw as u64)),
        source_api,
        state,
        unit,
        error_message,
    )
}

fn stats_string_field(
    raw: impl Into<String>,
    source_api: &'static str,
    state: &'static str,
    error_message: Option<String>,
) -> Value {
    stats_field(
        Value::String(raw.into()),
        source_api,
        state,
        None,
        error_message,
    )
}

fn stats_call_field(
    call: Option<&AdlApiCall>,
    source_api: &'static str,
    raw: Option<Value>,
    unit: Option<&'static str>,
) -> Value {
    match call {
        Some(call) => stats_field(
            raw.unwrap_or(Value::Null),
            call.source_api,
            call.state,
            unit.or(call.unit),
            call.error_message.clone(),
        ),
        None => stats_field(
            Value::Null,
            source_api,
            "not_available",
            unit,
            Some(format!(
                "{source_api} was not attempted for this adapter group."
            )),
        ),
    }
}

fn stats_adapter_json(sample: &ProviderSample, group: &PhysicalAdapterGroup) -> Value {
    let Some(adapter) = primary_adapter_for_group(sample, group) else {
        return json!({
            "physical_key": stats_string_field(
                group.physical_adapter_key.clone(),
                "wtg.adl.physical_grouping",
                "ok",
                None
            ),
            "state": stats_string_field(
                "missing_primary_record",
                "wtg.adl.physical_grouping",
                "error",
                Some("WTG ADL physical grouping found no primary record.".to_string())
            )
        });
    };

    let mut object = serde_json::Map::new();
    object.insert(
        "physical_key".to_string(),
        stats_string_field(
            group.physical_adapter_key.clone(),
            "wtg.adl.physical_grouping",
            "ok",
            None,
        ),
    );
    object.insert(
        "name".to_string(),
        stats_string_field(
            adapter.adl_raw_identity.adl_adapter_name.clone(),
            "ADL_Adapter_AdapterInfo_Get",
            "ok",
            None,
        ),
    );
    object.insert(
        "vendor_kind".to_string(),
        stats_string_field(
            if group.physical_vendor_id == 1002 {
                "AMD"
            } else {
                "non-AMD"
            },
            "ADL_Adapter_AdapterInfo_Get",
            "ok",
            None,
        ),
    );
    object.insert(
        "logical_adapter_indexes".to_string(),
        stats_field(
            json!(group.logical_adapter_indices),
            "wtg.adl.physical_grouping",
            "ok",
            None,
            None,
        ),
    );

    if group.physical_vendor_id != 1002 {
        object.insert(
            "vendor_scope".to_string(),
            stats_string_field("topology_only", "wtg.adl.vendor_scope", "ok", None),
        );
        return Value::Object(object);
    }

    let active_call = adapter
        .adl_calls
        .iter()
        .find(|call| call.metric_key == "adapter_active");
    object.insert(
        "active".to_string(),
        stats_call_field(
            active_call,
            "ADL_Adapter_Active_Get",
            active_call.and_then(|call| call.raw.get("value")).cloned(),
            None,
        ),
    );

    let current_activity = adapter
        .adl_calls
        .iter()
        .find(|call| call.metric_key == "overdrive5_current_activity");
    object.insert(
        "activity_pct".to_string(),
        stats_call_field(
            current_activity,
            "ADL_Overdrive5_CurrentActivity_Get",
            current_activity
                .and_then(|call| call.raw.get("activity_percent"))
                .cloned(),
            Some("percent"),
        ),
    );
    object.insert(
        "engine_clock_mhz".to_string(),
        stats_call_field(
            current_activity,
            "ADL_Overdrive5_CurrentActivity_Get",
            current_activity
                .and_then(|call| call.raw.get("engine_clock_10khz"))
                .and_then(Value::as_i64)
                .map(|value| {
                    Value::Number(
                        serde_json::Number::from_f64(value as f64 / 100.0)
                            .expect("finite engine clock expected"),
                    )
                }),
            Some("mhz"),
        ),
    );
    object.insert(
        "memory_clock_mhz".to_string(),
        stats_call_field(
            current_activity,
            "ADL_Overdrive5_CurrentActivity_Get",
            current_activity
                .and_then(|call| call.raw.get("memory_clock_10khz"))
                .and_then(Value::as_i64)
                .map(|value| {
                    Value::Number(
                        serde_json::Number::from_f64(value as f64 / 100.0)
                            .expect("finite memory clock expected"),
                    )
                }),
            Some("mhz"),
        ),
    );
    object.insert(
        "bus_current".to_string(),
        stats_call_field(
            current_activity,
            "ADL_Overdrive5_CurrentActivity_Get",
            current_activity.map(|call| {
                json!({
                    "speed": call.raw.get("current_bus_speed").and_then(Value::as_i64),
                    "lanes": call.raw.get("current_bus_lanes").and_then(Value::as_i64)
                })
            }),
            None,
        ),
    );

    let observed_clock = adapter
        .adl_calls
        .iter()
        .find(|call| call.metric_key == "adapter_observed_clock_info");
    object.insert(
        "observed_core_clock_mhz".to_string(),
        stats_call_field(
            observed_clock,
            "ADL_Adapter_ObservedClockInfo_Get",
            observed_clock
                .and_then(|call| call.raw.get("core_clock_10khz"))
                .and_then(Value::as_i64)
                .map(|value| {
                    Value::Number(
                        serde_json::Number::from_f64(value as f64 / 100.0)
                            .expect("finite observed core clock expected"),
                    )
                }),
            Some("mhz"),
        ),
    );
    object.insert(
        "observed_memory_clock_mhz".to_string(),
        stats_call_field(
            observed_clock,
            "ADL_Adapter_ObservedClockInfo_Get",
            observed_clock
                .and_then(|call| call.raw.get("memory_clock_10khz"))
                .and_then(Value::as_i64)
                .map(|value| {
                    Value::Number(
                        serde_json::Number::from_f64(value as f64 / 100.0)
                            .expect("finite observed memory clock expected"),
                    )
                }),
            Some("mhz"),
        ),
    );

    let bios = adapter
        .adl_calls
        .iter()
        .find(|call| call.metric_key == "adapter_video_bios_info");
    object.insert(
        "vbios_version".to_string(),
        stats_call_field(
            bios,
            "ADL_Adapter_VideoBiosInfo_Get",
            bios.and_then(|call| call.raw.get("version")).cloned(),
            None,
        ),
    );

    object.insert(
        "temperature_c".to_string(),
        stats_call_field(
            adapter
                .adl_calls
                .iter()
                .find(|call| call.metric_key == "overdrive5_temperature"),
            "ADL_Overdrive5_Temperature_Get",
            None,
            Some("celsius"),
        ),
    );

    Value::Object(object)
}

#[derive(Default)]
struct OptionalCallSummary {
    attempted: usize,
    ok: usize,
    unsupported: usize,
    not_available: usize,
    error: usize,
}

fn optional_call_summary(sample: &ProviderSample) -> OptionalCallSummary {
    let mut summary = OptionalCallSummary::default();

    for call in sample
        .adapters
        .iter()
        .flat_map(|adapter| adapter.adl_calls.iter())
        .filter(|call| is_optional_telemetry_call(call))
    {
        summary.attempted += 1;
        match call.state {
            "ok" => summary.ok += 1,
            "unsupported" => summary.unsupported += 1,
            "not_available" => summary.not_available += 1,
            "error" => summary.error += 1,
            _ => {}
        }
    }

    summary
}

fn is_optional_telemetry_call(call: &AdlApiCall) -> bool {
    call.metric_key != "adapter_active" && call.metric_key != "extended_discovery"
}

fn primary_adapter_for_group<'a>(
    sample: &'a ProviderSample,
    group: &PhysicalAdapterGroup,
) -> Option<&'a AdlAdapterRecord> {
    sample.adapters.iter().find(|adapter| {
        adapter.adl_raw_identity.adl_adapter_index == group.physical_adapter_primary_index
    })
}

impl AdlLibrary {
    fn telemetry_exports_matched(&self) -> usize {
        [
            self.overdrive_caps.is_some(),
            self.overdrive5_current_activity_get.is_some(),
            self.overdrive5_temperature_get.is_some(),
            self.overdrive5_fan_speed_info_get.is_some(),
            self.overdrive5_fan_speed_get.is_some(),
            self.overdrive5_od_parameters_get.is_some(),
            self.adapter_memory_info_get.is_some(),
            self.adapter_video_bios_info_get.is_some(),
            self.adapter_asic_family_type_get.is_some(),
            self.adapter_observed_clock_info_get.is_some(),
        ]
        .into_iter()
        .filter(|matched| *matched)
        .count()
    }

    fn load() -> Result<Self, String> {
        for dll_name in ["atiadlxx.dll", "atiadlxy.dll"] {
            match load_adl_library(dll_name) {
                Ok(library) => return Ok(library),
                Err(_) => continue,
            }
        }

        Err("AMD ADL DLL not found via atiadlxx.dll or atiadlxy.dll.".to_string())
    }
}

fn load_adl_library(dll_name: &str) -> Result<AdlLibrary, String> {
    let wide_name = to_wide(dll_name);
    let module = unsafe { LoadLibraryW(wide_name.as_ptr()) };
    let module = NonNull::new(module).ok_or_else(|| format!("failed to load {dll_name}"))?;
    let dll_path = unsafe { module_path(module.as_ptr())? };

    let main_control_create =
        unsafe { load_symbol::<AdlMainControlCreate>(module, b"ADL_Main_Control_Create\0")? };
    let main_control_destroy =
        unsafe { load_symbol::<AdlMainControlDestroy>(module, b"ADL_Main_Control_Destroy\0")? };
    let adapter_number_of_adapters_get = unsafe {
        load_symbol::<AdlAdapterNumberOfAdaptersGet>(module, b"ADL_Adapter_NumberOfAdapters_Get\0")?
    };
    let adapter_info_get = unsafe {
        load_symbol::<AdlAdapterAdapterInfoGet>(module, b"ADL_Adapter_AdapterInfo_Get\0")?
    };
    let adapter_active_get =
        unsafe { load_optional_symbol::<AdlAdapterActiveGet>(module, b"ADL_Adapter_Active_Get\0") };
    let overdrive_caps =
        unsafe { load_optional_symbol::<AdlOverdriveCaps>(module, b"ADL_Overdrive_Caps\0") };
    let overdrive5_current_activity_get = unsafe {
        load_optional_symbol::<AdlOverdrive5CurrentActivityGet>(
            module,
            b"ADL_Overdrive5_CurrentActivity_Get\0",
        )
    };
    let overdrive5_temperature_get = unsafe {
        load_optional_symbol::<AdlOverdrive5TemperatureGet>(
            module,
            b"ADL_Overdrive5_Temperature_Get\0",
        )
    };
    let overdrive5_fan_speed_info_get = unsafe {
        load_optional_symbol::<AdlOverdrive5FanSpeedInfoGet>(
            module,
            b"ADL_Overdrive5_FanSpeedInfo_Get\0",
        )
    };
    let overdrive5_fan_speed_get = unsafe {
        load_optional_symbol::<AdlOverdrive5FanSpeedGet>(module, b"ADL_Overdrive5_FanSpeed_Get\0")
    };
    let overdrive5_od_parameters_get = unsafe {
        load_optional_symbol::<AdlOverdrive5OdParametersGet>(
            module,
            b"ADL_Overdrive5_ODParameters_Get\0",
        )
    };
    let adapter_memory_info_get = unsafe {
        load_optional_symbol::<AdlAdapterMemoryInfoGet>(module, b"ADL_Adapter_MemoryInfo_Get\0")
    };
    let adapter_video_bios_info_get = unsafe {
        load_optional_symbol::<AdlAdapterVideoBiosInfoGet>(
            module,
            b"ADL_Adapter_VideoBiosInfo_Get\0",
        )
    };
    let adapter_asic_family_type_get = unsafe {
        load_optional_symbol::<AdlAdapterAsicFamilyTypeGet>(
            module,
            b"ADL_Adapter_ASICFamilyType_Get\0",
        )
    };
    let adapter_observed_clock_info_get = unsafe {
        load_optional_symbol::<AdlAdapterObservedClockInfoGet>(
            module,
            b"ADL_Adapter_ObservedClockInfo_Get\0",
        )
    };

    Ok(AdlLibrary {
        module,
        dll_name: dll_name.to_string(),
        dll_path,
        main_control_create,
        main_control_destroy,
        adapter_number_of_adapters_get,
        adapter_info_get,
        adapter_active_get,
        overdrive_caps,
        overdrive5_current_activity_get,
        overdrive5_temperature_get,
        overdrive5_fan_speed_info_get,
        overdrive5_fan_speed_get,
        overdrive5_od_parameters_get,
        adapter_memory_info_get,
        adapter_video_bios_info_get,
        adapter_asic_family_type_get,
        adapter_observed_clock_info_get,
    })
}

impl<'a> AdlSession<'a> {
    fn create(library: &'a AdlLibrary) -> Result<Self, AdlInitError> {
        let result = unsafe { (library.main_control_create)(adl_alloc, 1) };
        if result != ADL_OK {
            return Err(AdlInitError {
                adl_main_control_create: result,
                message: format!("ADL_Main_Control_Create failed with status {result}."),
            });
        }

        Ok(Self {
            library,
            initialized: true,
        })
    }
}

struct EnumerationResult {
    adapter_number_of_adapters_get: i32,
    adapter_adapter_info_get: i32,
    physical_adapter_group_count: usize,
    amd_physical_adapter_count: usize,
    non_amd_physical_adapter_count: usize,
    extended_amd_discovery_run_count: usize,
    physical_adapter_groups: Vec<PhysicalAdapterGroup>,
    adapters: Vec<AdlAdapterRecord>,
}

struct PhysicalAdapterGroupState {
    physical_adapter_key: String,
    physical_vendor_id: i32,
    physical_adapter_primary_index: i32,
    logical_record_kind: &'static str,
    logical_adapter_indices: Vec<i32>,
    extended_amd_discovery_attempted: bool,
}

fn enumerate_adapters(library: &AdlLibrary) -> Result<EnumerationResult, EnumerationError> {
    let mut adapter_count = 0i32;
    let count_result = unsafe { (library.adapter_number_of_adapters_get)(&mut adapter_count) };
    if count_result != ADL_OK {
        return Err(EnumerationError {
            adl_adapter_number_of_adapters_get: Some(count_result),
            adl_adapter_adapter_info_get: None,
            message: format!("ADL_Adapter_NumberOfAdapters_Get failed with status {count_result}."),
        });
    }

    if adapter_count <= 0 {
        return Ok(EnumerationResult {
            adapter_number_of_adapters_get: count_result,
            adapter_adapter_info_get: ADL_OK,
            physical_adapter_group_count: 0,
            amd_physical_adapter_count: 0,
            non_amd_physical_adapter_count: 0,
            extended_amd_discovery_run_count: 0,
            physical_adapter_groups: Vec::new(),
            adapters: Vec::new(),
        });
    }

    let mut adapter_info = vec![zeroed_adapter_info(); adapter_count as usize];
    let bytes = (adapter_info.len() * size_of::<AdapterInfo>()) as i32;
    let info_result = unsafe { (library.adapter_info_get)(adapter_info.as_mut_ptr(), bytes) };
    if info_result != ADL_OK {
        return Err(EnumerationError {
            adl_adapter_number_of_adapters_get: Some(count_result),
            adl_adapter_adapter_info_get: Some(info_result),
            message: format!("ADL_Adapter_AdapterInfo_Get failed with status {info_result}."),
        });
    }

    let mut physical_adapter_groups_state = Vec::<PhysicalAdapterGroupState>::new();
    let mut adapters = Vec::with_capacity(adapter_info.len());
    let mut extended_amd_discovery_run_count = 0usize;

    for info in adapter_info.into_iter() {
        let physical_adapter_key = physical_adapter_key(&info);
        let existing_group_index = physical_adapter_groups_state
            .iter()
            .position(|group| group.physical_adapter_key == physical_adapter_key);

        let (physical_adapter_role, physical_adapter_primary_index, duplicate_amd_record) =
            if let Some(group_index) = existing_group_index {
                let group = &mut physical_adapter_groups_state[group_index];
                group.logical_adapter_indices.push(info.i_adapter_index);
                (
                    "duplicate_logical_record",
                    group.physical_adapter_primary_index,
                    info.i_vendor_id == 1002,
                )
            } else {
                physical_adapter_groups_state.push(PhysicalAdapterGroupState {
                    physical_adapter_key: physical_adapter_key.clone(),
                    physical_vendor_id: info.i_vendor_id,
                    physical_adapter_primary_index: info.i_adapter_index,
                    logical_record_kind: "adl_display_record",
                    logical_adapter_indices: vec![info.i_adapter_index],
                    extended_amd_discovery_attempted: info.i_vendor_id == 1002,
                });
                if info.i_vendor_id == 1002 {
                    extended_amd_discovery_run_count += 1;
                }
                ("primary_physical_record", info.i_adapter_index, false)
            };

        adapters.push(build_adapter_record(
            library,
            info,
            physical_adapter_key,
            physical_adapter_role,
            physical_adapter_primary_index,
            duplicate_amd_record,
        ));
    }

    let physical_adapter_groups = physical_adapter_groups_state
        .iter()
        .map(|group| PhysicalAdapterGroup {
            physical_adapter_key: group.physical_adapter_key.clone(),
            physical_vendor_id: group.physical_vendor_id,
            physical_adapter_primary_index: group.physical_adapter_primary_index,
            logical_record_kind: group.logical_record_kind,
            logical_adapter_indices: group.logical_adapter_indices.clone(),
            logical_record_count: group.logical_adapter_indices.len(),
            extended_amd_discovery_attempted: group.extended_amd_discovery_attempted,
        })
        .collect::<Vec<_>>();
    let amd_physical_adapter_count = physical_adapter_groups_state
        .iter()
        .filter(|group| group.physical_vendor_id == 1002)
        .count();
    let non_amd_physical_adapter_count =
        physical_adapter_groups_state.len() - amd_physical_adapter_count;

    Ok(EnumerationResult {
        adapter_number_of_adapters_get: count_result,
        adapter_adapter_info_get: info_result,
        physical_adapter_group_count: physical_adapter_groups_state.len(),
        amd_physical_adapter_count,
        non_amd_physical_adapter_count,
        extended_amd_discovery_run_count,
        physical_adapter_groups,
        adapters,
    })
}

fn build_adapter_record(
    library: &AdlLibrary,
    info: AdapterInfo,
    physical_adapter_key: String,
    physical_adapter_role: &'static str,
    physical_adapter_primary_index: i32,
    duplicate_amd_record: bool,
) -> AdlAdapterRecord {
    let mut provider_warnings = Vec::new();
    let adl_calls = if info.i_vendor_id != 1002 {
        provider_warnings.push(
            "ADL returned non-AMD adapter identity; record preserved, but extended AMD ADL telemetry discovery was skipped for this adapter."
                .to_string(),
        );
        vec![query_adapter_active(library, info.i_adapter_index)]
    } else if duplicate_amd_record {
        vec![
            query_adapter_active(library, info.i_adapter_index),
            duplicate_extended_discovery_call(physical_adapter_primary_index),
        ]
    } else {
        collect_adapter_calls(library, info.i_adapter_index)
    };
    let provider_errors = adl_calls
        .iter()
        .filter(|call| call.state == "error")
        .filter_map(|call| call.error_message.clone())
        .collect::<Vec<_>>();
    provider_warnings.extend(
        adl_calls
            .iter()
            .filter(|call| call.state == "unsupported" || call.state == "not_available")
            .filter_map(|call| call.error_message.clone()),
    );

    AdlAdapterRecord {
        adl_raw_identity: AdlRawIdentity {
            adl_adapter_index: info.i_adapter_index,
            adl_udid: adl_c_string(&info.str_udid),
            adl_adapter_name: adl_c_string(&info.str_adapter_name),
            adl_display_name: adl_c_string(&info.str_display_name),
            adl_vendor_id: info.i_vendor_id,
            adl_present: info.i_present != 0,
            adl_exists: info.i_exist != 0,
            adl_bus_number: info.i_bus_number,
            adl_device_number: info.i_device_number,
            adl_function_number: info.i_function_number,
            adl_driver_path: adl_c_string(&info.str_driver_path),
            adl_driver_path_ext: adl_c_string(&info.str_driver_path_ext),
            adl_pnp_string: adl_c_string(&info.str_pnp_string),
            adl_os_display_index: info.i_os_display_index,
        },
        physical_adapter_key,
        physical_adapter_role,
        physical_adapter_primary_index,
        logical_record_kind: "adl_display_record",
        adl_calls,
        provider_warnings,
        provider_errors,
    }
}

fn collect_adapter_calls(library: &AdlLibrary, adapter_index: i32) -> Vec<AdlApiCall> {
    vec![
        query_adapter_active(library, adapter_index),
        query_overdrive_caps(library, adapter_index),
        query_current_activity(library, adapter_index),
        query_observed_clock_info(library, adapter_index),
        query_temperature(library, adapter_index),
        query_od_parameters(library, adapter_index),
        query_fan_speed_info(library, adapter_index),
        query_fan_speed(library, adapter_index, ADL_FANCTRL_SPEED_TYPE_PERCENT),
        query_fan_speed(library, adapter_index, ADL_FANCTRL_SPEED_TYPE_RPM),
        query_memory_info(library, adapter_index),
        query_video_bios_info(library, adapter_index),
        query_asic_family_type(library, adapter_index),
    ]
}

fn physical_adapter_key(info: &AdapterInfo) -> String {
    format!(
        "vendor={},bus={},device={},function={}",
        info.i_vendor_id, info.i_bus_number, info.i_device_number, info.i_function_number
    )
}

fn duplicate_extended_discovery_call(primary_adapter_index: i32) -> AdlApiCall {
    AdlApiCall {
        metric_key: "extended_discovery",
        source_api: "WTG_ADL_PROVIDER_DEDUP",
        state: "not_available",
        adl_return_code: None,
        raw: json!({
            "reason": "duplicate_physical_adapter_record",
            "primary_adapter_index": primary_adapter_index
        }),
        unit: None,
        error_message: Some(
            "Extended AMD ADL telemetry discovery skipped because this ADL record duplicates an already-probed physical AMD adapter."
                .to_string(),
        ),
    }
}

fn zeroed_adapter_info() -> AdapterInfo {
    let mut info = unsafe { MaybeUninit::<AdapterInfo>::zeroed().assume_init() };
    info.i_size = size_of::<AdapterInfo>() as i32;
    info
}

fn zeroed_with_size<T>(size: usize) -> T {
    let mut value = unsafe { MaybeUninit::<T>::zeroed().assume_init() };
    unsafe {
        ptr::write((&mut value as *mut T).cast::<c_int>(), size as c_int);
    }
    value
}

fn adl_c_string(raw: &[c_char]) -> String {
    let ptr = raw.as_ptr();
    unsafe { CStr::from_ptr(ptr) }.to_string_lossy().to_string()
}

unsafe extern "C" fn adl_alloc(size: c_int) -> *mut c_void {
    if size <= 0 {
        return ptr::null_mut();
    }

    let mut buffer = Vec::<u8>::with_capacity(size as usize);
    let ptr = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    ptr.cast()
}

unsafe fn load_symbol<T>(module: NonNull<c_void>, name: &[u8]) -> Result<T, String> {
    let symbol = GetProcAddress(module.as_ptr(), name.as_ptr().cast());
    if symbol.is_null() {
        let label = String::from_utf8_lossy(&name[..name.len().saturating_sub(1)]).to_string();
        return Err(format!("missing ADL symbol {label}."));
    }

    Ok(std::mem::transmute_copy(&symbol))
}

unsafe fn load_optional_symbol<T>(module: NonNull<c_void>, name: &[u8]) -> Option<T> {
    let symbol = GetProcAddress(module.as_ptr(), name.as_ptr().cast());
    if symbol.is_null() {
        return None;
    }

    Some(std::mem::transmute_copy(&symbol))
}

fn query_adapter_active(library: &AdlLibrary, adapter_index: i32) -> AdlApiCall {
    let Some(active_get) = library.adapter_active_get else {
        return unsupported_call(
            "adapter_active",
            "ADL_Adapter_Active_Get",
            "ADL_Adapter_Active_Get symbol unavailable in loaded ADL DLL.",
        );
    };

    let mut active = 0i32;
    let result = unsafe { active_get(adapter_index, &mut active) };
    if result != ADL_OK {
        return call_from_error(
            "adapter_active",
            "ADL_Adapter_Active_Get",
            result,
            format!(
                "ADL_Adapter_Active_Get failed for adapter index {adapter_index} with status {result}."
            ),
        );
    }

    AdlApiCall {
        metric_key: "adapter_active",
        source_api: "ADL_Adapter_Active_Get",
        state: "ok",
        adl_return_code: Some(result),
        raw: json!({ "value": active != 0 }),
        unit: None,
        error_message: None,
    }
}

fn query_overdrive_caps(library: &AdlLibrary, adapter_index: i32) -> AdlApiCall {
    let Some(overdrive_caps) = library.overdrive_caps else {
        return unsupported_call(
            "overdrive_caps",
            "ADL_Overdrive_Caps",
            "ADL_Overdrive_Caps symbol unavailable in loaded ADL DLL.",
        );
    };

    let mut supported = 0i32;
    let mut enabled = 0i32;
    let mut version = 0i32;
    let result =
        unsafe { overdrive_caps(adapter_index, &mut supported, &mut enabled, &mut version) };
    if result != ADL_OK {
        return call_from_error(
            "overdrive_caps",
            "ADL_Overdrive_Caps",
            result,
            format!(
                "ADL_Overdrive_Caps failed for adapter index {adapter_index} with status {result}."
            ),
        );
    }

    let state = if supported == 0 {
        "not_available"
    } else {
        "ok"
    };
    let error_message = if supported == 0 {
        Some(
            "ADL_Overdrive_Caps succeeded but reported overdrive unsupported for this adapter."
                .to_string(),
        )
    } else {
        None
    };

    AdlApiCall {
        metric_key: "overdrive_caps",
        source_api: "ADL_Overdrive_Caps",
        state,
        adl_return_code: Some(result),
        raw: json!({
            "supported": supported != 0,
            "enabled": enabled != 0,
            "version": version
        }),
        unit: None,
        error_message,
    }
}

fn query_current_activity(library: &AdlLibrary, adapter_index: i32) -> AdlApiCall {
    let Some(current_activity_get) = library.overdrive5_current_activity_get else {
        return unsupported_call(
            "overdrive5_current_activity",
            "ADL_Overdrive5_CurrentActivity_Get",
            "ADL_Overdrive5_CurrentActivity_Get symbol unavailable in loaded ADL DLL.",
        );
    };

    let mut activity = zeroed_with_size::<AdlPmActivity>(size_of::<AdlPmActivity>());
    let result = unsafe { current_activity_get(adapter_index, &mut activity) };
    if result != ADL_OK {
        return call_from_error(
            "overdrive5_current_activity",
            "ADL_Overdrive5_CurrentActivity_Get",
            result,
            format!("ADL_Overdrive5_CurrentActivity_Get failed for adapter index {adapter_index} with status {result}."),
        );
    }

    AdlApiCall {
        metric_key: "overdrive5_current_activity",
        source_api: "ADL_Overdrive5_CurrentActivity_Get",
        state: "ok",
        adl_return_code: Some(result),
        raw: json!({
            "engine_clock_10khz": activity.i_engine_clock,
            "memory_clock_10khz": activity.i_memory_clock,
            "vddc_mv": activity.i_vddc,
            "activity_percent": activity.i_activity_percent,
            "current_performance_level": activity.i_current_performance_level,
            "current_bus_speed": activity.i_current_bus_speed,
            "current_bus_lanes": activity.i_current_bus_lanes,
            "maximum_bus_lanes": activity.i_maximum_bus_lanes,
            "reserved": activity.i_reserved
        }),
        unit: None,
        error_message: None,
    }
}

fn query_observed_clock_info(library: &AdlLibrary, adapter_index: i32) -> AdlApiCall {
    let Some(observed_clock_info_get) = library.adapter_observed_clock_info_get else {
        return unsupported_call(
            "adapter_observed_clock_info",
            "ADL_Adapter_ObservedClockInfo_Get",
            "ADL_Adapter_ObservedClockInfo_Get symbol unavailable in loaded ADL DLL.",
        );
    };

    let mut core_clock = 0i32;
    let mut memory_clock = 0i32;
    let result =
        unsafe { observed_clock_info_get(adapter_index, &mut core_clock, &mut memory_clock) };
    if result != ADL_OK {
        return call_from_error(
            "adapter_observed_clock_info",
            "ADL_Adapter_ObservedClockInfo_Get",
            result,
            format!("ADL_Adapter_ObservedClockInfo_Get failed for adapter index {adapter_index} with status {result}."),
        );
    }

    AdlApiCall {
        metric_key: "adapter_observed_clock_info",
        source_api: "ADL_Adapter_ObservedClockInfo_Get",
        state: "ok",
        adl_return_code: Some(result),
        raw: json!({
            "core_clock_10khz": core_clock,
            "memory_clock_10khz": memory_clock
        }),
        unit: Some("10_khz"),
        error_message: None,
    }
}

fn query_temperature(library: &AdlLibrary, adapter_index: i32) -> AdlApiCall {
    let Some(temperature_get) = library.overdrive5_temperature_get else {
        return unsupported_call(
            "overdrive5_temperature",
            "ADL_Overdrive5_Temperature_Get",
            "ADL_Overdrive5_Temperature_Get symbol unavailable in loaded ADL DLL.",
        );
    };

    let mut temperature = zeroed_with_size::<AdlTemperature>(size_of::<AdlTemperature>());
    let thermal_controller_index = 0i32;
    let result =
        unsafe { temperature_get(adapter_index, thermal_controller_index, &mut temperature) };
    if result != ADL_OK {
        return call_from_error(
            "overdrive5_temperature",
            "ADL_Overdrive5_Temperature_Get",
            result,
            format!("ADL_Overdrive5_Temperature_Get failed for adapter index {adapter_index} with status {result}."),
        );
    }

    AdlApiCall {
        metric_key: "overdrive5_temperature",
        source_api: "ADL_Overdrive5_Temperature_Get",
        state: "ok",
        adl_return_code: Some(result),
        raw: json!({
            "thermal_controller_index": thermal_controller_index,
            "temperature_millidegrees_celsius": temperature.i_temperature
        }),
        unit: Some("millidegrees_celsius"),
        error_message: None,
    }
}

fn query_od_parameters(library: &AdlLibrary, adapter_index: i32) -> AdlApiCall {
    let Some(od_parameters_get) = library.overdrive5_od_parameters_get else {
        return unsupported_call(
            "overdrive5_od_parameters",
            "ADL_Overdrive5_ODParameters_Get",
            "ADL_Overdrive5_ODParameters_Get symbol unavailable in loaded ADL DLL.",
        );
    };

    let mut params = zeroed_with_size::<AdlOdParameters>(size_of::<AdlOdParameters>());
    let result = unsafe { od_parameters_get(adapter_index, &mut params) };
    if result != ADL_OK {
        return call_from_error(
            "overdrive5_od_parameters",
            "ADL_Overdrive5_ODParameters_Get",
            result,
            format!("ADL_Overdrive5_ODParameters_Get failed for adapter index {adapter_index} with status {result}."),
        );
    }

    AdlApiCall {
        metric_key: "overdrive5_od_parameters",
        source_api: "ADL_Overdrive5_ODParameters_Get",
        state: "ok",
        adl_return_code: Some(result),
        raw: json!({
            "number_of_performance_levels": params.i_number_of_performance_levels,
            "activity_reporting_supported": params.i_activity_reporting_supported != 0,
            "discrete_performance_levels": params.i_discrete_performance_levels != 0,
            "reserved": params.i_reserved,
            "engine_clock_range": range_json(params.s_engine_clock),
            "memory_clock_range": range_json(params.s_memory_clock),
            "vddc_range": range_json(params.s_vddc)
        }),
        unit: None,
        error_message: None,
    }
}

fn query_fan_speed_info(library: &AdlLibrary, adapter_index: i32) -> AdlApiCall {
    let Some(fan_speed_info_get) = library.overdrive5_fan_speed_info_get else {
        return unsupported_call(
            "overdrive5_fan_speed_info",
            "ADL_Overdrive5_FanSpeedInfo_Get",
            "ADL_Overdrive5_FanSpeedInfo_Get symbol unavailable in loaded ADL DLL.",
        );
    };

    let mut fan_speed_info = zeroed_with_size::<AdlFanSpeedInfo>(size_of::<AdlFanSpeedInfo>());
    let thermal_controller_index = 0i32;
    let result =
        unsafe { fan_speed_info_get(adapter_index, thermal_controller_index, &mut fan_speed_info) };
    if result != ADL_OK {
        return call_from_error(
            "overdrive5_fan_speed_info",
            "ADL_Overdrive5_FanSpeedInfo_Get",
            result,
            format!("ADL_Overdrive5_FanSpeedInfo_Get failed for adapter index {adapter_index} with status {result}."),
        );
    }

    AdlApiCall {
        metric_key: "overdrive5_fan_speed_info",
        source_api: "ADL_Overdrive5_FanSpeedInfo_Get",
        state: "ok",
        adl_return_code: Some(result),
        raw: json!({
            "thermal_controller_index": thermal_controller_index,
            "flags": fan_speed_info.i_flags,
            "min_percent": fan_speed_info.i_min_percent,
            "max_percent": fan_speed_info.i_max_percent,
            "min_rpm": fan_speed_info.i_min_rpm,
            "max_rpm": fan_speed_info.i_max_rpm
        }),
        unit: None,
        error_message: None,
    }
}

fn query_fan_speed(library: &AdlLibrary, adapter_index: i32, speed_type: i32) -> AdlApiCall {
    let Some(fan_speed_get) = library.overdrive5_fan_speed_get else {
        return unsupported_call(
            fan_metric_key(speed_type),
            "ADL_Overdrive5_FanSpeed_Get",
            "ADL_Overdrive5_FanSpeed_Get symbol unavailable in loaded ADL DLL.",
        );
    };

    let mut fan_speed = zeroed_with_size::<AdlFanSpeedValue>(size_of::<AdlFanSpeedValue>());
    fan_speed.i_speed_type = speed_type;
    let thermal_controller_index = 0i32;
    let result = unsafe { fan_speed_get(adapter_index, thermal_controller_index, &mut fan_speed) };
    if result != ADL_OK {
        return call_from_error(
            fan_metric_key(speed_type),
            "ADL_Overdrive5_FanSpeed_Get",
            result,
            format!("ADL_Overdrive5_FanSpeed_Get failed for adapter index {adapter_index} with status {result} for speed_type {speed_type}."),
        );
    }

    AdlApiCall {
        metric_key: fan_metric_key(speed_type),
        source_api: "ADL_Overdrive5_FanSpeed_Get",
        state: "ok",
        adl_return_code: Some(result),
        raw: json!({
            "thermal_controller_index": thermal_controller_index,
            "speed_type": fan_speed.i_speed_type,
            "fan_speed": fan_speed.i_fan_speed,
            "flags": fan_speed.i_flags
        }),
        unit: fan_unit(speed_type),
        error_message: None,
    }
}

fn query_memory_info(library: &AdlLibrary, adapter_index: i32) -> AdlApiCall {
    let Some(memory_info_get) = library.adapter_memory_info_get else {
        return unsupported_call(
            "adapter_memory_info",
            "ADL_Adapter_MemoryInfo_Get",
            "ADL_Adapter_MemoryInfo_Get symbol unavailable in loaded ADL DLL.",
        );
    };

    let mut memory_info = unsafe { MaybeUninit::<AdlMemoryInfo>::zeroed().assume_init() };
    let result = unsafe { memory_info_get(adapter_index, &mut memory_info) };
    if result != ADL_OK {
        return call_from_error(
            "adapter_memory_info",
            "ADL_Adapter_MemoryInfo_Get",
            result,
            format!("ADL_Adapter_MemoryInfo_Get failed for adapter index {adapter_index} with status {result}."),
        );
    }

    let memory_type = adl_c_string(&memory_info.str_memory_type);
    let memory_bandwidth = adl_c_string(&memory_info.str_memory_bandwidth);
    let state = if memory_type.is_empty()
        && memory_bandwidth.is_empty()
        && memory_info.i_memory_size <= 0
    {
        "not_available"
    } else {
        "ok"
    };
    let error_message = if state == "not_available" {
        Some(
            "ADL_Adapter_MemoryInfo_Get succeeded but returned no populated memory details."
                .to_string(),
        )
    } else {
        None
    };

    AdlApiCall {
        metric_key: "adapter_memory_info",
        source_api: "ADL_Adapter_MemoryInfo_Get",
        state,
        adl_return_code: Some(result),
        raw: json!({
            "memory_type": memory_type,
            "memory_size_raw": memory_info.i_memory_size,
            "memory_bandwidth": memory_bandwidth
        }),
        unit: None,
        error_message,
    }
}

fn query_video_bios_info(library: &AdlLibrary, adapter_index: i32) -> AdlApiCall {
    let Some(video_bios_info_get) = library.adapter_video_bios_info_get else {
        return unsupported_call(
            "adapter_video_bios_info",
            "ADL_Adapter_VideoBiosInfo_Get",
            "ADL_Adapter_VideoBiosInfo_Get symbol unavailable in loaded ADL DLL.",
        );
    };

    let mut bios_info = unsafe { MaybeUninit::<AdlBiosInfo>::zeroed().assume_init() };
    let result = unsafe { video_bios_info_get(adapter_index, &mut bios_info) };
    if result != ADL_OK {
        return call_from_error(
            "adapter_video_bios_info",
            "ADL_Adapter_VideoBiosInfo_Get",
            result,
            format!("ADL_Adapter_VideoBiosInfo_Get failed for adapter index {adapter_index} with status {result}."),
        );
    }

    let part_number = adl_c_string(&bios_info.str_part_number);
    let version = adl_c_string(&bios_info.str_version);
    let date = adl_c_string(&bios_info.str_date);
    let state = if part_number.is_empty() && version.is_empty() && date.is_empty() {
        "not_available"
    } else {
        "ok"
    };
    let error_message = if state == "not_available" {
        Some(
            "ADL_Adapter_VideoBiosInfo_Get succeeded but returned no populated BIOS strings."
                .to_string(),
        )
    } else {
        None
    };

    AdlApiCall {
        metric_key: "adapter_video_bios_info",
        source_api: "ADL_Adapter_VideoBiosInfo_Get",
        state,
        adl_return_code: Some(result),
        raw: json!({
            "part_number": part_number,
            "version": version,
            "date": date
        }),
        unit: None,
        error_message,
    }
}

fn query_asic_family_type(library: &AdlLibrary, adapter_index: i32) -> AdlApiCall {
    let Some(asic_family_type_get) = library.adapter_asic_family_type_get else {
        return unsupported_call(
            "adapter_asic_family_type",
            "ADL_Adapter_ASICFamilyType_Get",
            "ADL_Adapter_ASICFamilyType_Get symbol unavailable in loaded ADL DLL.",
        );
    };

    let mut asic_family_type = 0i32;
    let mut valids = 0i32;
    let result = unsafe { asic_family_type_get(adapter_index, &mut asic_family_type, &mut valids) };
    if result != ADL_OK {
        return call_from_error(
            "adapter_asic_family_type",
            "ADL_Adapter_ASICFamilyType_Get",
            result,
            format!("ADL_Adapter_ASICFamilyType_Get failed for adapter index {adapter_index} with status {result}."),
        );
    }

    let state = if asic_family_type == 0 && valids == 0 {
        "not_available"
    } else {
        "ok"
    };
    let error_message = if state == "not_available" {
        Some(
            "ADL_Adapter_ASICFamilyType_Get succeeded but returned zeroed ASIC family details."
                .to_string(),
        )
    } else {
        None
    };

    AdlApiCall {
        metric_key: "adapter_asic_family_type",
        source_api: "ADL_Adapter_ASICFamilyType_Get",
        state,
        adl_return_code: Some(result),
        raw: json!({
            "asic_family_type": asic_family_type,
            "valids": valids
        }),
        unit: None,
        error_message,
    }
}

fn unsupported_call(
    metric_key: &'static str,
    source_api: &'static str,
    message: &str,
) -> AdlApiCall {
    AdlApiCall {
        metric_key,
        source_api,
        state: "unsupported",
        adl_return_code: None,
        raw: Value::Null,
        unit: None,
        error_message: Some(message.to_string()),
    }
}

fn call_from_error(
    metric_key: &'static str,
    source_api: &'static str,
    result: i32,
    message: String,
) -> AdlApiCall {
    AdlApiCall {
        metric_key,
        source_api,
        state: if result == ADL_ERR_NOT_SUPPORTED {
            "unsupported"
        } else {
            "error"
        },
        adl_return_code: Some(result),
        raw: Value::Null,
        unit: None,
        error_message: Some(message),
    }
}

fn range_json(range: AdlOdParameterRange) -> Value {
    json!({
        "min": range.i_min,
        "max": range.i_max,
        "step": range.i_step
    })
}

fn fan_metric_key(speed_type: i32) -> &'static str {
    match speed_type {
        ADL_FANCTRL_SPEED_TYPE_PERCENT => "overdrive5_fan_speed_percent",
        ADL_FANCTRL_SPEED_TYPE_RPM => "overdrive5_fan_speed_rpm",
        _ => "overdrive5_fan_speed_unknown",
    }
}

fn fan_unit(speed_type: i32) -> Option<&'static str> {
    match speed_type {
        ADL_FANCTRL_SPEED_TYPE_PERCENT => Some("percent"),
        ADL_FANCTRL_SPEED_TYPE_RPM => Some("rpm"),
        _ => None,
    }
}

unsafe fn module_path(module: *mut c_void) -> Result<String, String> {
    let mut buffer = [0u16; 260];
    let len = GetModuleFileNameW(module, buffer.as_mut_ptr(), buffer.len() as u32);
    if len == 0 {
        return Err("failed to query loaded ADL module path".to_string());
    }

    Ok(String::from_utf16_lossy(&buffer[..len as usize]))
}

fn parse_args<I>(args: I) -> Result<CliArgs, String>
where
    I: IntoIterator<Item = String>,
{
    let args = args.into_iter().collect::<Vec<_>>();
    let mut mode = None;
    let mut interval_ms = DEFAULT_INTERVAL_MS;
    let mut i = 0usize;

    while i < args.len() {
        match args[i].as_str() {
            "--once" => {
                if mode.replace(Mode::Once).is_some() {
                    return Err("choose only one of --once or --watch".to_string());
                }
                i += 1;
            }
            "--watch" => {
                if mode.replace(Mode::Watch).is_some() {
                    return Err("choose only one of --once or --watch".to_string());
                }
                i += 1;
            }
            "--interval-ms" => {
                let value = args
                    .get(i + 1)
                    .ok_or_else(|| "--interval-ms requires a value".to_string())?;
                interval_ms = value
                    .parse::<u64>()
                    .map_err(|_| format!("--interval-ms must be an integer. Got: {value}"))?;
                i += 2;
            }
            "--help" | "-h" => return Err("help requested".to_string()),
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    let mode = mode.ok_or_else(|| "one of --once or --watch is required".to_string())?;
    if mode == Mode::Once && interval_ms != DEFAULT_INTERVAL_MS {
        return Err("--interval-ms is valid only with --watch".to_string());
    }

    Ok(CliArgs { mode, interval_ms })
}

fn usage_error(message: &str) -> i32 {
    eprintln!("WTG AMD ADL provider usage error: {message}");
    2
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
        call_from_error, duplicate_extended_discovery_call, fan_metric_key, fan_unit,
        format_snapshot, format_watch_sample, parse_args, physical_adapter_key, AdapterInfo,
        AdlAdapterRecord, AdlApiCall, AdlRawIdentity, CliArgs, Mode, PhysicalAdapterGroup,
        ProviderSample, SampleReturnCodes, ADL_MAX_PATH, DEFAULT_INTERVAL_MS, PROVIDER, SOURCE,
        TELEMETRY_CLASS,
    };
    use serde_json::json;

    fn amd_sample_with_unavailable_active() -> ProviderSample {
        ProviderSample {
            wtg_version: "0.3.0",
            source: SOURCE,
            telemetry_class: TELEMETRY_CLASS,
            provider: PROVIDER,
            provider_authority: "AMD ADL",
            status: "ok",
            sample_seq: 0,
            timestamp_unix_ms: 0,
            dll_name: Some("atiadlxx.dll".to_string()),
            dll_path: Some("C:\\Windows\\System32\\atiadlxx.dll".to_string()),
            adl_initialized: true,
            adl_return_codes: SampleReturnCodes {
                adl_main_control_create: Some(0),
                adl_adapter_number_of_adapters_get: Some(0),
                adl_adapter_adapter_info_get: Some(0),
            },
            telemetry_exports_matched: 1,
            physical_adapter_group_count: 1,
            amd_physical_adapter_count: 1,
            non_amd_physical_adapter_count: 0,
            extended_amd_discovery_run_count: 1,
            physical_adapter_groups: vec![PhysicalAdapterGroup {
                physical_adapter_key: "vendor=1002,bus=1,device=0,function=0".to_string(),
                physical_vendor_id: 1002,
                physical_adapter_primary_index: 7,
                logical_record_kind: "adl_display_record",
                logical_adapter_indices: vec![7],
                logical_record_count: 1,
                extended_amd_discovery_attempted: true,
            }],
            adapters: vec![AdlAdapterRecord {
                adl_raw_identity: AdlRawIdentity {
                    adl_adapter_index: 7,
                    adl_udid: String::new(),
                    adl_adapter_name: "AMD Radeon Test".to_string(),
                    adl_display_name: String::new(),
                    adl_vendor_id: 1002,
                    adl_present: true,
                    adl_exists: true,
                    adl_bus_number: 1,
                    adl_device_number: 0,
                    adl_function_number: 0,
                    adl_driver_path: String::new(),
                    adl_driver_path_ext: String::new(),
                    adl_pnp_string: String::new(),
                    adl_os_display_index: 0,
                },
                physical_adapter_key: "vendor=1002,bus=1,device=0,function=0".to_string(),
                physical_adapter_role: "primary",
                physical_adapter_primary_index: 7,
                logical_record_kind: "adl_display_record",
                adl_calls: vec![AdlApiCall {
                    metric_key: "adapter_active",
                    source_api: "ADL_Adapter_Active_Get",
                    state: "unsupported",
                    adl_return_code: None,
                    raw: serde_json::Value::Null,
                    unit: None,
                    error_message: Some(
                        "ADL_Adapter_Active_Get symbol unavailable in loaded ADL DLL.".to_string(),
                    ),
                }],
                provider_warnings: vec![
                    "ADL_Adapter_Active_Get symbol unavailable in loaded ADL DLL.".to_string(),
                ],
                provider_errors: Vec::new(),
            }],
            errors: Vec::new(),
        }
    }

    #[test]
    fn parse_once_args() {
        let args = parse_args(vec!["--once".to_string()]).unwrap();
        assert_eq!(
            args,
            CliArgs {
                mode: Mode::Once,
                interval_ms: DEFAULT_INTERVAL_MS,
            }
        );
    }

    #[test]
    fn parse_watch_args() {
        let args = parse_args(vec![
            "--watch".to_string(),
            "--interval-ms".to_string(),
            "1500".to_string(),
        ])
        .unwrap();
        assert_eq!(
            args,
            CliArgs {
                mode: Mode::Watch,
                interval_ms: 1500,
            }
        );
    }

    #[test]
    fn parse_rejects_once_with_interval() {
        let err = parse_args(vec![
            "--once".to_string(),
            "--interval-ms".to_string(),
            "1500".to_string(),
        ])
        .unwrap_err();
        assert!(err.contains("--interval-ms is valid only with --watch"));
    }

    #[test]
    fn provider_constants_match_contract() {
        assert_eq!(SOURCE, "wtg.provider.amd.adl");
        assert_eq!(TELEMETRY_CLASS, "provider_telemetry");
        assert_eq!(PROVIDER, "amd.adl");
    }

    #[test]
    fn fan_speed_metric_labels_match_speed_type() {
        assert_eq!(fan_metric_key(1), "overdrive5_fan_speed_percent");
        assert_eq!(fan_metric_key(2), "overdrive5_fan_speed_rpm");
        assert_eq!(fan_unit(1), Some("percent"));
        assert_eq!(fan_unit(2), Some("rpm"));
    }

    #[test]
    fn unsupported_return_code_maps_to_unsupported_state() {
        let call = call_from_error("x", "ADL_Test", -8, "boom".to_string());
        assert_eq!(call.state, "unsupported");
    }

    #[test]
    fn physical_adapter_key_uses_vendor_bus_device_function() {
        let info = AdapterInfo {
            i_size: 0,
            i_adapter_index: 7,
            str_udid: [0; ADL_MAX_PATH],
            i_bus_number: 6,
            i_device_number: 0,
            i_function_number: 0,
            i_vendor_id: 1002,
            str_adapter_name: [0; ADL_MAX_PATH],
            str_display_name: [0; ADL_MAX_PATH],
            i_present: 1,
            i_exist: 1,
            str_driver_path: [0; ADL_MAX_PATH],
            str_driver_path_ext: [0; ADL_MAX_PATH],
            str_pnp_string: [0; ADL_MAX_PATH],
            i_os_display_index: 0,
        };

        assert_eq!(
            physical_adapter_key(&info),
            "vendor=1002,bus=6,device=0,function=0"
        );
    }

    #[test]
    fn duplicate_extended_discovery_call_is_explicit() {
        let call = duplicate_extended_discovery_call(0);

        assert_eq!(call.metric_key, "extended_discovery");
        assert_eq!(call.source_api, "WTG_ADL_PROVIDER_DEDUP");
        assert_eq!(call.state, "not_available");
        assert_eq!(
            call.raw,
            json!({
                "reason": "duplicate_physical_adapter_record",
                "primary_adapter_index": 0
            })
        );
    }

    #[test]
    fn human_snapshot_marks_unavailable_active_as_unavailable() {
        let rendered = format_snapshot(&amd_sample_with_unavailable_active());
        assert!(rendered.contains("  Active: unavailable"));
    }

    #[test]
    fn human_watch_marks_unavailable_active_as_unavailable() {
        let rendered = format_watch_sample(&amd_sample_with_unavailable_active());
        assert!(rendered.contains("  active: unavailable"));
    }
}
