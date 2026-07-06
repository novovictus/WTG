use std::collections::{BTreeSet, HashMap};
use std::ffi::{c_char, c_void, CStr, OsStr};
use std::iter;
use std::mem::{size_of, MaybeUninit};
use std::os::windows::ffi::OsStrExt;
use std::ptr::{self, NonNull};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{json, Value};

const SOURCE: &str = "wtg.provider.intel.level_zero";
const TELEMETRY_CLASS: &str = "provider_telemetry";
const PROVIDER: &str = "intel";
const PROVIDER_AUTHORITY: &str = "Intel Level Zero";
const STATS_SCHEMA: &str = "wtg.intel_level_zero.stats.v3";
const ZE_RESULT_SUCCESS: i32 = 0;
const ZE_MAX_DEVICE_NAME: usize = 256;
const SYSMAN_BUFFER_BYTES: usize = 512;
const FIRST_VISIBLE_SAMPLE_PRIMING_DELAY_MS: u64 = 250;

type ZeInit = unsafe extern "C" fn(u32) -> i32;
type ZeDriverGet = unsafe extern "C" fn(*mut u32, *mut *mut c_void) -> i32;
type ZeDeviceGet = unsafe extern "C" fn(*mut c_void, *mut u32, *mut *mut c_void) -> i32;
type ZeDeviceGetProperties = unsafe extern "C" fn(*mut c_void, *mut ZeDeviceProperties) -> i32;
type ZesInit = unsafe extern "C" fn(u32) -> i32;
type ZesEnumHandles = unsafe extern "C" fn(*mut c_void, *mut u32, *mut *mut c_void) -> i32;
type ZesGetBuffer = unsafe extern "C" fn(*mut c_void, *mut SysmanBuffer) -> i32;
type ZesGetTemperatureState = unsafe extern "C" fn(*mut c_void, *mut f64) -> i32;

#[repr(C)]
#[derive(Clone, Copy)]
struct ZeDeviceProperties {
    stype: u32,
    p_next: *mut c_void,
    device_type: u32,
    vendor_id: u32,
    device_id: u32,
    flags: u32,
    subdevice_id: u32,
    core_clock_rate: u32,
    max_mem_alloc_size: u64,
    max_hardware_contexts: u32,
    max_command_queue_priority: u32,
    num_threads_per_eu: u32,
    physical_eu_simd_width: u32,
    num_eus_per_subslice: u32,
    num_subslices_per_slice: u32,
    num_slices: u32,
    timer_resolution: u32,
    timestamp_valid_bits: u32,
    kernel_timestamp_valid_bits: u32,
    uuid: [u8; 16],
    name: [c_char; ZE_MAX_DEVICE_NAME],
}

#[repr(C, align(8))]
#[derive(Clone, Copy)]
struct SysmanBuffer {
    stype: u32,
    p_next: *mut c_void,
    bytes: [u8; SYSMAN_BUFFER_BYTES],
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct ZesMemProperties {
    stype: u32,
    p_next: *mut c_void,
    mem_type: u32,
    on_subdevice: u32,
    subdevice_id: u32,
    location: u32,
    physical_size: u64,
    bus_width: i32,
    num_channels: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct ZesMemState {
    stype: u32,
    p_next: *const c_void,
    health: u32,
    free: u64,
    size: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct ZesFreqProperties {
    stype: u32,
    p_next: *mut c_void,
    domain_type: u32,
    on_subdevice: u32,
    subdevice_id: u32,
    can_control: u32,
    is_throttle_event_supported: u32,
    min: f64,
    max: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct ZesFreqState {
    stype: u32,
    p_next: *const c_void,
    current_voltage: f64,
    request: f64,
    tdp: f64,
    efficient: f64,
    actual: f64,
    throttle_reasons: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct ZesPowerEnergyCounter {
    energy: u64,
    timestamp: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct ZesEngineStats {
    active_time: u64,
    timestamp: u64,
}

#[derive(Clone, Copy)]
struct PowerDeltaBaseline {
    energy_uj: u64,
    timestamp_us: u64,
}

#[derive(Clone, Copy)]
struct EngineDeltaBaseline {
    active_time: u64,
    timestamp: u64,
}

// These baselines currently live across one-shot collect_once calls in module-level statics.
// If Intel moves to a long-lived provider session, rerun the idle-vs-reset delta tests because
// the lifecycle boundary changes what a backward counter means in practice.
static POWER_DELTA_BASELINES: OnceLock<Mutex<HashMap<String, PowerDeltaBaseline>>> =
    OnceLock::new();
static ENGINE_DELTA_BASELINES: OnceLock<Mutex<HashMap<String, EngineDeltaBaseline>>> =
    OnceLock::new();

#[derive(Debug, Serialize)]
pub struct ProviderSample {
    wtg_version: &'static str,
    provider: &'static str,
    provider_authority: &'static str,
    provider_source: &'static str,
    telemetry_class: &'static str,
    status: &'static str,
    sample_seq: u64,
    timestamp_unix_ms: u128,
    dll_name: Option<String>,
    dll_path: Option<String>,
    telemetry_exports_matched: usize,
    sysman_exports_matched: usize,
    optional_calls_attempted: usize,
    optional_calls_ok: usize,
    optional_calls_unsupported: usize,
    optional_calls_not_available: usize,
    optional_calls_error: usize,
    driver_record_count: usize,
    device_record_count: usize,
    sysman_facts: Vec<IntelFact>,
    devices: Vec<DeviceRecord>,
    errors: Vec<String>,
}

#[derive(Debug, Serialize)]
struct DeviceRecord {
    driver_index: usize,
    device_index: usize,
    key: String,
    facts: Vec<IntelFact>,
    unavailable: Vec<&'static str>,
}

#[derive(Debug, Serialize, Clone)]
struct IntelFact {
    metric_key: String,
    source_api: &'static str,
    state: &'static str,
    raw: Value,
    unit: Option<&'static str>,
    error_message: Option<String>,
}

struct LevelZeroLibrary {
    module: NonNull<c_void>,
    dll_name: String,
    dll_path: String,
    ze_init: ZeInit,
    ze_driver_get: ZeDriverGet,
    ze_device_get: ZeDeviceGet,
    ze_device_get_properties: ZeDeviceGetProperties,
    zes_init: Option<ZesInit>,
    zes_device_enum_engine_groups: Option<ZesEnumHandles>,
    zes_engine_get_properties: Option<ZesGetBuffer>,
    zes_engine_get_activity: Option<ZesGetBuffer>,
    zes_device_enum_memory_modules: Option<ZesEnumHandles>,
    zes_memory_get_properties: Option<ZesGetBuffer>,
    zes_memory_get_state: Option<ZesGetBuffer>,
    zes_device_enum_power_domains: Option<ZesEnumHandles>,
    zes_power_get_properties: Option<ZesGetBuffer>,
    zes_power_get_energy_counter: Option<ZesGetBuffer>,
    zes_device_enum_temperature_sensors: Option<ZesEnumHandles>,
    zes_temperature_get_properties: Option<ZesGetBuffer>,
    zes_temperature_get_state: Option<ZesGetTemperatureState>,
    zes_device_enum_frequency_domains: Option<ZesEnumHandles>,
    zes_frequency_get_properties: Option<ZesGetBuffer>,
    zes_frequency_get_state: Option<ZesGetBuffer>,
}

struct SysmanExportSpec {
    metric_key: &'static str,
    symbol_name: &'static [u8],
}

struct SysmanProbe {
    exports_matched: usize,
    facts: Vec<IntelFact>,
    zes_init_ok: bool,
}

struct SysmanDomainGroup {
    domain_key: &'static str,
    unavailable_label: &'static str,
    enum_source_api: &'static str,
    enum_handles: Option<ZesEnumHandles>,
    property_source_api: &'static str,
    get_properties: Option<ZesGetBuffer>,
    state_source_api: &'static str,
    get_state_buffer: Option<ZesGetBuffer>,
    get_state_temperature: Option<ZesGetTemperatureState>,
}

const SYSMAN_EXPORT_SPECS: &[SysmanExportSpec] = &[
    SysmanExportSpec {
        metric_key: "zesInit_export",
        symbol_name: b"zesInit\0",
    },
    SysmanExportSpec {
        metric_key: "zesDeviceEnumEngineGroups_export",
        symbol_name: b"zesDeviceEnumEngineGroups\0",
    },
    SysmanExportSpec {
        metric_key: "zesEngineGetProperties_export",
        symbol_name: b"zesEngineGetProperties\0",
    },
    SysmanExportSpec {
        metric_key: "zesEngineGetActivity_export",
        symbol_name: b"zesEngineGetActivity\0",
    },
    SysmanExportSpec {
        metric_key: "zesDeviceEnumMemoryModules_export",
        symbol_name: b"zesDeviceEnumMemoryModules\0",
    },
    SysmanExportSpec {
        metric_key: "zesMemoryGetProperties_export",
        symbol_name: b"zesMemoryGetProperties\0",
    },
    SysmanExportSpec {
        metric_key: "zesMemoryGetState_export",
        symbol_name: b"zesMemoryGetState\0",
    },
    SysmanExportSpec {
        metric_key: "zesDeviceEnumPowerDomains_export",
        symbol_name: b"zesDeviceEnumPowerDomains\0",
    },
    SysmanExportSpec {
        metric_key: "zesPowerGetProperties_export",
        symbol_name: b"zesPowerGetProperties\0",
    },
    SysmanExportSpec {
        metric_key: "zesPowerGetEnergyCounter_export",
        symbol_name: b"zesPowerGetEnergyCounter\0",
    },
    SysmanExportSpec {
        metric_key: "zesDeviceEnumTemperatureSensors_export",
        symbol_name: b"zesDeviceEnumTemperatureSensors\0",
    },
    SysmanExportSpec {
        metric_key: "zesTemperatureGetProperties_export",
        symbol_name: b"zesTemperatureGetProperties\0",
    },
    SysmanExportSpec {
        metric_key: "zesTemperatureGetState_export",
        symbol_name: b"zesTemperatureGetState\0",
    },
    SysmanExportSpec {
        metric_key: "zesDeviceEnumFrequencyDomains_export",
        symbol_name: b"zesDeviceEnumFrequencyDomains\0",
    },
    SysmanExportSpec {
        metric_key: "zesFrequencyGetProperties_export",
        symbol_name: b"zesFrequencyGetProperties\0",
    },
    SysmanExportSpec {
        metric_key: "zesFrequencyGetState_export",
        symbol_name: b"zesFrequencyGetState\0",
    },
];

impl Drop for LevelZeroLibrary {
    fn drop(&mut self) {
        unsafe {
            FreeLibrary(self.module.as_ptr());
        }
    }
}

pub fn provider_source() -> &'static str {
    SOURCE
}

pub fn telemetry_class() -> &'static str {
    TELEMETRY_CLASS
}

pub fn provider_authority() -> &'static str {
    PROVIDER_AUTHORITY
}

pub fn sample_status(sample: &ProviderSample) -> &'static str {
    sample.status
}

pub fn collect_once(sample_seq: u64) -> ProviderSample {
    match LevelZeroLibrary::load() {
        Ok(library) => {
            let init_result = unsafe { (library.ze_init)(0) };
            let sysman_probe = library.probe_sysman();
            if init_result != ZE_RESULT_SUCCESS {
                return unavailable_sample(
                    sample_seq,
                    Some(library.dll_name.clone()),
                    Some(library.dll_path.clone()),
                    library.telemetry_exports_matched(),
                    sysman_probe.exports_matched,
                    sysman_probe.facts,
                    format!("zeInit failed with status {init_result}."),
                );
            }

            match enumerate_devices(&library, sample_seq, sysman_probe.zes_init_ok) {
                Ok(enumeration) => enumeration.into_sample(sample_seq, library, sysman_probe),
                Err(reason) => unavailable_sample(
                    sample_seq,
                    Some(library.dll_name.clone()),
                    Some(library.dll_path.clone()),
                    library.telemetry_exports_matched(),
                    sysman_probe.exports_matched,
                    sysman_probe.facts,
                    reason,
                ),
            }
        }
        Err(reason) => unavailable_sample(sample_seq, None, None, 0, 0, Vec::new(), reason),
    }
}

fn collect_visible_sample_with_priming_impl<F, S>(
    sample_seq: u64,
    priming_delay_ms: u64,
    mut collect: F,
    sleep: S,
) -> ProviderSample
where
    F: FnMut(u64) -> ProviderSample,
    S: FnOnce(std::time::Duration),
{
    let priming_sample = collect(sample_seq);
    if sample_status(&priming_sample) != "ok" {
        return priming_sample;
    }

    sleep(std::time::Duration::from_millis(priming_delay_ms));
    collect(sample_seq)
}

pub fn collect_visible_sample(sample_seq: u64) -> ProviderSample {
    collect_visible_sample_with_priming_impl(
        sample_seq,
        FIRST_VISIBLE_SAMPLE_PRIMING_DELAY_MS,
        collect_once,
        thread::sleep,
    )
}

pub fn format_snapshot(sample: &ProviderSample) -> String {
    let mut lines = Vec::new();
    if sample.status != "ok" {
        lines.push(format!("Provider status: {}", sample.status));
        if let Some(reason) = sample.errors.first() {
            lines.push(format!("Reason: {reason}"));
        }
        return lines.join("\n");
    }

    for device in sample.devices.iter() {
        push_snapshot_device_lines(&mut lines, device);
        lines.push(String::new());
    }
    lines.pop();
    lines.join("\n")
}

pub fn format_watch_sample(sample: &ProviderSample) -> String {
    let mut lines = Vec::new();
    lines.push(format!("sample_seq: {}", sample.sample_seq));
    if sample.status != "ok" {
        lines.push(format!("provider.status: {}", sample.status));
        if let Some(reason) = sample.errors.first() {
            lines.push(format!("reason: {reason}"));
        }
        return lines.join("\n");
    }

    lines.push(String::new());
    for (index, device) in sample.devices.iter().enumerate() {
        push_watch_device_lines(&mut lines, device);
        if index + 1 != sample.devices.len() {
            lines.push(String::new());
        }
    }
    lines.join("\n")
}

pub fn format_probe_snapshot(sample: &ProviderSample) -> String {
    let mut lines = Vec::new();
    lines.push("[probe] provider=intel_level_zero".to_string());
    lines.push(format!("wtg.version: {}", sample.wtg_version));
    lines.push(format!("provider.authority: {}", sample.provider_authority));
    lines.push(format!("provider.source: {}", sample.provider_source));
    lines.push(format!("telemetry.class: {}", sample.telemetry_class));
    lines.push(format!("provider.status: {}", sample.status));
    if sample.status != "ok" {
        if let Some(reason) = sample.errors.first() {
            lines.push(format!("reason: {reason}"));
        }
        return lines.join("\n");
    }

    lines.push(String::new());
    lines.push(format!(
        "intel.driver_records: {}",
        sample.driver_record_count
    ));
    lines.push(format!(
        "intel.device_records: {}",
        sample.device_record_count
    ));
    lines.push(format!(
        "intel.telemetry_exports_matched: {}",
        sample.telemetry_exports_matched
    ));
    lines.push(format!(
        "intel.sysman_exports_matched: {}",
        sample.sysman_exports_matched
    ));
    lines.push(format!(
        "intel.optional_calls_attempted: {}",
        sample.optional_calls_attempted
    ));
    lines.push(format!(
        "intel.optional_calls.ok: {}",
        sample.optional_calls_ok
    ));
    lines.push(format!(
        "intel.optional_calls.unsupported: {}",
        sample.optional_calls_unsupported
    ));
    lines.push(format!(
        "intel.optional_calls.not_available: {}",
        sample.optional_calls_not_available
    ));
    lines.push(format!(
        "intel.optional_calls.error: {}",
        sample.optional_calls_error
    ));
    for fact in sample.sysman_facts.iter() {
        lines.push(format_probe_fact_line("intel.sysman", fact));
    }

    for device in sample.devices.iter() {
        lines.push(String::new());
        lines.push(format!("[probe] device={}", device.device_index));
        push_probe_device_lines(&mut lines, device);
    }

    lines.join("\n")
}

pub fn format_stats_snapshot_json(sample: &ProviderSample, tick_seq: u64, tick_ts: &str) -> String {
    let devices = sample
        .devices
        .iter()
        .map(stats_device_json)
        .collect::<Vec<_>>();
    let payload = json!({
        "devices": devices,
        "intel": {
            "driver_record_count": stats_number_field(
                sample.driver_record_count,
                "zeDriverGet",
                sample.status,
                None,
                sample.errors.first().cloned()
            ),
            "device_record_count": stats_number_field(
                sample.device_record_count,
                "zeDeviceGet",
                sample.status,
                None,
                sample.errors.first().cloned()
            ),
            "telemetry_exports_matched": stats_number_field(
                sample.telemetry_exports_matched,
                "wtg.intel.level_zero.dynamic_load",
                "ok",
                None,
                None
            ),
            "sysman_exports_matched": stats_number_field(
                sample.sysman_exports_matched,
                "wtg.intel.level_zero.dynamic_load",
                "ok",
                None,
                None
            ),
            "sysman": stats_facts_json(&sample.sysman_facts),
            "optional_calls_attempted": stats_number_field(
                sample.optional_calls_attempted,
                "zeDeviceGetProperties",
                "ok",
                None,
                None
            ),
            "optional_calls_ok": stats_number_field(
                sample.optional_calls_ok,
                "zeDeviceGetProperties",
                "ok",
                None,
                None
            ),
            "optional_calls_unsupported": stats_number_field(
                sample.optional_calls_unsupported,
                "zeDeviceGetProperties",
                "ok",
                None,
                None
            ),
            "optional_calls_not_available": stats_number_field(
                sample.optional_calls_not_available,
                "zeDeviceGetProperties",
                "ok",
                None,
                None
            ),
            "optional_calls_error": stats_number_field(
                sample.optional_calls_error,
                "zeDeviceGetProperties",
                "ok",
                None,
                None
            )
        },
        "provider": PROVIDER,
        "provider_authority": PROVIDER_AUTHORITY,
        "provider_source": SOURCE,
        "schema": STATS_SCHEMA,
        "telemetry_class": TELEMETRY_CLASS,
        "tick_seq": tick_seq,
        "tick_ts": tick_ts,
        "timestamp_unix_ms": sample.timestamp_unix_ms,
        "wtg_version": sample.wtg_version
    });

    serde_json::to_string_pretty(&payload).expect("Intel stats JSON serialization should succeed")
}

struct Enumeration {
    driver_record_count: usize,
    device_record_count: usize,
    devices: Vec<DeviceRecord>,
    optional_calls_attempted: usize,
    optional_calls_ok: usize,
    optional_calls_unsupported: usize,
    optional_calls_not_available: usize,
    optional_calls_error: usize,
}

impl Enumeration {
    fn into_sample(
        self,
        sample_seq: u64,
        library: LevelZeroLibrary,
        sysman_probe: SysmanProbe,
    ) -> ProviderSample {
        let status = if self.device_record_count == 0 {
            "unavailable"
        } else {
            "ok"
        };
        let errors = if self.device_record_count == 0 {
            vec!["Level Zero initialized but returned zero device handles.".to_string()]
        } else {
            Vec::new()
        };

        ProviderSample {
            wtg_version: env!("CARGO_PKG_VERSION"),
            provider: PROVIDER,
            provider_authority: PROVIDER_AUTHORITY,
            provider_source: SOURCE,
            telemetry_class: TELEMETRY_CLASS,
            status,
            sample_seq,
            timestamp_unix_ms: now_unix_ms(),
            dll_name: Some(library.dll_name.clone()),
            dll_path: Some(library.dll_path.clone()),
            telemetry_exports_matched: library.telemetry_exports_matched(),
            sysman_exports_matched: sysman_probe.exports_matched,
            optional_calls_attempted: self.optional_calls_attempted,
            optional_calls_ok: self.optional_calls_ok,
            optional_calls_unsupported: self.optional_calls_unsupported,
            optional_calls_not_available: self.optional_calls_not_available,
            optional_calls_error: self.optional_calls_error,
            driver_record_count: self.driver_record_count,
            device_record_count: self.device_record_count,
            sysman_facts: sysman_probe.facts,
            devices: self.devices,
            errors,
        }
    }
}

fn unavailable_sample(
    sample_seq: u64,
    dll_name: Option<String>,
    dll_path: Option<String>,
    telemetry_exports_matched: usize,
    sysman_exports_matched: usize,
    sysman_facts: Vec<IntelFact>,
    reason: String,
) -> ProviderSample {
    ProviderSample {
        wtg_version: env!("CARGO_PKG_VERSION"),
        provider: PROVIDER,
        provider_authority: PROVIDER_AUTHORITY,
        provider_source: SOURCE,
        telemetry_class: TELEMETRY_CLASS,
        status: "unavailable",
        sample_seq,
        timestamp_unix_ms: now_unix_ms(),
        dll_name,
        dll_path,
        telemetry_exports_matched,
        sysman_exports_matched,
        optional_calls_attempted: 0,
        optional_calls_ok: 0,
        optional_calls_unsupported: 0,
        optional_calls_not_available: 0,
        optional_calls_error: 0,
        driver_record_count: 0,
        device_record_count: 0,
        sysman_facts,
        devices: Vec::new(),
        errors: vec![reason],
    }
}

impl LevelZeroLibrary {
    fn load() -> Result<Self, String> {
        let dll_name = "ze_loader.dll";
        let wide_name = to_wide(dll_name);
        let module = unsafe { LoadLibraryW(wide_name.as_ptr()) };
        let module = NonNull::new(module).ok_or_else(|| {
            "Intel Level Zero runtime DLL ze_loader.dll was not found.".to_string()
        })?;
        let dll_path = unsafe { module_path(module.as_ptr())? };

        let ze_init = unsafe { load_symbol::<ZeInit>(module, b"zeInit\0")? };
        let ze_driver_get = unsafe { load_symbol::<ZeDriverGet>(module, b"zeDriverGet\0")? };
        let ze_device_get = unsafe { load_symbol::<ZeDeviceGet>(module, b"zeDeviceGet\0")? };
        let ze_device_get_properties =
            unsafe { load_symbol::<ZeDeviceGetProperties>(module, b"zeDeviceGetProperties\0")? };
        let zes_init = unsafe { load_optional_symbol::<ZesInit>(module, b"zesInit\0") };
        let zes_device_enum_engine_groups = unsafe {
            load_optional_symbol::<ZesEnumHandles>(module, b"zesDeviceEnumEngineGroups\0")
        };
        let zes_engine_get_properties =
            unsafe { load_optional_symbol::<ZesGetBuffer>(module, b"zesEngineGetProperties\0") };
        let zes_engine_get_activity =
            unsafe { load_optional_symbol::<ZesGetBuffer>(module, b"zesEngineGetActivity\0") };
        let zes_device_enum_memory_modules = unsafe {
            load_optional_symbol::<ZesEnumHandles>(module, b"zesDeviceEnumMemoryModules\0")
        };
        let zes_memory_get_properties =
            unsafe { load_optional_symbol::<ZesGetBuffer>(module, b"zesMemoryGetProperties\0") };
        let zes_memory_get_state =
            unsafe { load_optional_symbol::<ZesGetBuffer>(module, b"zesMemoryGetState\0") };
        let zes_device_enum_power_domains = unsafe {
            load_optional_symbol::<ZesEnumHandles>(module, b"zesDeviceEnumPowerDomains\0")
        };
        let zes_power_get_properties =
            unsafe { load_optional_symbol::<ZesGetBuffer>(module, b"zesPowerGetProperties\0") };
        let zes_power_get_energy_counter =
            unsafe { load_optional_symbol::<ZesGetBuffer>(module, b"zesPowerGetEnergyCounter\0") };
        let zes_device_enum_temperature_sensors = unsafe {
            load_optional_symbol::<ZesEnumHandles>(module, b"zesDeviceEnumTemperatureSensors\0")
        };
        let zes_temperature_get_properties = unsafe {
            load_optional_symbol::<ZesGetBuffer>(module, b"zesTemperatureGetProperties\0")
        };
        let zes_temperature_get_state = unsafe {
            load_optional_symbol::<ZesGetTemperatureState>(module, b"zesTemperatureGetState\0")
        };
        let zes_device_enum_frequency_domains = unsafe {
            load_optional_symbol::<ZesEnumHandles>(module, b"zesDeviceEnumFrequencyDomains\0")
        };
        let zes_frequency_get_properties =
            unsafe { load_optional_symbol::<ZesGetBuffer>(module, b"zesFrequencyGetProperties\0") };
        let zes_frequency_get_state =
            unsafe { load_optional_symbol::<ZesGetBuffer>(module, b"zesFrequencyGetState\0") };

        Ok(Self {
            module,
            dll_name: dll_name.to_string(),
            dll_path,
            ze_init,
            ze_driver_get,
            ze_device_get,
            ze_device_get_properties,
            zes_init,
            zes_device_enum_engine_groups,
            zes_engine_get_properties,
            zes_engine_get_activity,
            zes_device_enum_memory_modules,
            zes_memory_get_properties,
            zes_memory_get_state,
            zes_device_enum_power_domains,
            zes_power_get_properties,
            zes_power_get_energy_counter,
            zes_device_enum_temperature_sensors,
            zes_temperature_get_properties,
            zes_temperature_get_state,
            zes_device_enum_frequency_domains,
            zes_frequency_get_properties,
            zes_frequency_get_state,
        })
    }

    fn telemetry_exports_matched(&self) -> usize {
        4
    }

    fn probe_sysman(&self) -> SysmanProbe {
        let mut exports_matched = 0usize;
        let mut facts = Vec::new();

        for spec in SYSMAN_EXPORT_SPECS.iter() {
            let is_available = unsafe { has_symbol(self.module, spec.symbol_name) };
            if is_available {
                exports_matched += 1;
                facts.push(ok_fact(
                    spec.metric_key,
                    "wtg.intel.level_zero.dynamic_load",
                    json!(true),
                ));
            } else {
                facts.push(IntelFact {
                    metric_key: spec.metric_key.to_string(),
                    source_api: "wtg.intel.level_zero.dynamic_load",
                    state: "not_available",
                    raw: json!(false),
                    unit: None,
                    error_message: Some(format!(
                        "missing optional Sysman symbol {}.",
                        symbol_label(spec.symbol_name)
                    )),
                });
            }
        }

        let zes_init_ok = match self.zes_init {
            Some(zes_init) => {
                let result = unsafe { zes_init(0) };
                if result == ZE_RESULT_SUCCESS {
                    facts.push(IntelFact {
                        metric_key: "zesInit_result".to_string(),
                        source_api: "zesInit",
                        state: "ok",
                        raw: json!(result),
                        unit: None,
                        error_message: None,
                    });
                    true
                } else {
                    facts.push(IntelFact {
                        metric_key: "zesInit_result".to_string(),
                        source_api: "zesInit",
                        state: "error",
                        raw: json!(result),
                        unit: None,
                        error_message: Some(format!("zesInit failed with status {result}.")),
                    });
                    false
                }
            }
            None => {
                facts.push(IntelFact {
                    metric_key: "zesInit_result".to_string(),
                    source_api: "zesInit",
                    state: "not_available",
                    raw: Value::Null,
                    unit: None,
                    error_message: Some("missing optional Sysman symbol zesInit.".to_string()),
                });
                false
            }
        };

        SysmanProbe {
            exports_matched,
            facts,
            zes_init_ok,
        }
    }

    fn sysman_domain_groups(&self) -> [SysmanDomainGroup; 5] {
        [
            SysmanDomainGroup {
                domain_key: "engine_groups",
                unavailable_label: "activity",
                enum_source_api: "zesDeviceEnumEngineGroups",
                enum_handles: self.zes_device_enum_engine_groups,
                property_source_api: "zesEngineGetProperties",
                get_properties: self.zes_engine_get_properties,
                state_source_api: "zesEngineGetActivity",
                get_state_buffer: self.zes_engine_get_activity,
                get_state_temperature: None,
            },
            SysmanDomainGroup {
                domain_key: "memory_modules",
                unavailable_label: "memory",
                enum_source_api: "zesDeviceEnumMemoryModules",
                enum_handles: self.zes_device_enum_memory_modules,
                property_source_api: "zesMemoryGetProperties",
                get_properties: self.zes_memory_get_properties,
                state_source_api: "zesMemoryGetState",
                get_state_buffer: self.zes_memory_get_state,
                get_state_temperature: None,
            },
            SysmanDomainGroup {
                domain_key: "power_domains",
                unavailable_label: "power",
                enum_source_api: "zesDeviceEnumPowerDomains",
                enum_handles: self.zes_device_enum_power_domains,
                property_source_api: "zesPowerGetProperties",
                get_properties: self.zes_power_get_properties,
                state_source_api: "zesPowerGetEnergyCounter",
                get_state_buffer: self.zes_power_get_energy_counter,
                get_state_temperature: None,
            },
            SysmanDomainGroup {
                domain_key: "temperature_sensors",
                unavailable_label: "temperature",
                enum_source_api: "zesDeviceEnumTemperatureSensors",
                enum_handles: self.zes_device_enum_temperature_sensors,
                property_source_api: "zesTemperatureGetProperties",
                get_properties: self.zes_temperature_get_properties,
                state_source_api: "zesTemperatureGetState",
                get_state_buffer: None,
                get_state_temperature: self.zes_temperature_get_state,
            },
            SysmanDomainGroup {
                domain_key: "frequency_domains",
                unavailable_label: "frequency",
                enum_source_api: "zesDeviceEnumFrequencyDomains",
                enum_handles: self.zes_device_enum_frequency_domains,
                property_source_api: "zesFrequencyGetProperties",
                get_properties: self.zes_frequency_get_properties,
                state_source_api: "zesFrequencyGetState",
                get_state_buffer: self.zes_frequency_get_state,
                get_state_temperature: None,
            },
        ]
    }
}

fn enumerate_devices(
    library: &LevelZeroLibrary,
    sample_seq: u64,
    sysman_ready: bool,
) -> Result<Enumeration, String> {
    let mut driver_count = 0u32;
    let result = unsafe { (library.ze_driver_get)(&mut driver_count, ptr::null_mut()) };
    if result != ZE_RESULT_SUCCESS {
        return Err(format!("zeDriverGet(count) failed with status {result}."));
    }

    if driver_count == 0 {
        return Err("Level Zero initialized but returned zero drivers.".to_string());
    }

    let mut drivers = vec![ptr::null_mut(); driver_count as usize];
    let result = unsafe { (library.ze_driver_get)(&mut driver_count, drivers.as_mut_ptr()) };
    if result != ZE_RESULT_SUCCESS {
        return Err(format!("zeDriverGet(handles) failed with status {result}."));
    }

    let mut devices = Vec::new();
    let mut optional_calls_attempted = 0usize;
    let mut optional_calls_ok = 0usize;
    let mut optional_calls_unsupported = 0usize;
    let mut optional_calls_not_available = 0usize;
    let mut optional_calls_error = 0usize;

    for (driver_index, driver) in drivers.into_iter().enumerate() {
        let mut device_count = 0u32;
        let result = unsafe { (library.ze_device_get)(driver, &mut device_count, ptr::null_mut()) };
        if result != ZE_RESULT_SUCCESS {
            return Err(format!(
                "zeDeviceGet(count) failed for driver index {driver_index} with status {result}."
            ));
        }
        if device_count == 0 {
            continue;
        }

        let mut driver_devices = vec![ptr::null_mut(); device_count as usize];
        let result = unsafe {
            (library.ze_device_get)(driver, &mut device_count, driver_devices.as_mut_ptr())
        };
        if result != ZE_RESULT_SUCCESS {
            return Err(format!(
                "zeDeviceGet(handles) failed for driver index {driver_index} with status {result}."
            ));
        }

        for (device_index, device) in driver_devices.into_iter().enumerate() {
            optional_calls_attempted += 1;
            let property_result = query_device_properties(library, device);
            match property_result.as_ref().map(|(_, result)| *result) {
                Ok(ZE_RESULT_SUCCESS) => optional_calls_ok += 1,
                Ok(result) if result == 0x78000001u32 as i32 => optional_calls_unsupported += 1,
                Ok(_) => optional_calls_error += 1,
                Err(_) => optional_calls_not_available += 1,
            }

            devices.push(build_device_record(
                library,
                sample_seq,
                driver_index,
                device_index,
                device,
                property_result,
                sysman_ready,
            ));
        }
    }

    Ok(Enumeration {
        driver_record_count: driver_count as usize,
        device_record_count: devices.len(),
        devices,
        optional_calls_attempted,
        optional_calls_ok,
        optional_calls_unsupported,
        optional_calls_not_available,
        optional_calls_error,
    })
}

fn build_device_record(
    library: &LevelZeroLibrary,
    sample_seq: u64,
    driver_index: usize,
    device_index: usize,
    device: *mut c_void,
    property_result: Result<(ZeDeviceProperties, i32), String>,
    sysman_ready: bool,
) -> DeviceRecord {
    let mut facts = Vec::new();
    let mut unavailable = Vec::new();

    match property_result {
        Ok((properties, result)) if result == ZE_RESULT_SUCCESS => {
            let name = c_string(&properties.name);
            let device_type = ze_device_type_name(properties.device_type).to_string();
            let key = format!(
                "driver={driver_index},device={device_index},vendor=0x{:04x},device=0x{:04x}",
                properties.vendor_id, properties.device_id
            );

            facts.push(ok_fact(
                "device_key",
                "wtg.intel.level_zero.device_key",
                json!(key.clone()),
            ));
            if name.is_empty() {
                facts.push(not_available_fact(
                    "device_name",
                    "zeDeviceGetProperties",
                    "Level Zero returned an empty device name.".to_string(),
                ));
                unavailable.push("name");
            } else {
                facts.push(ok_fact("device_name", "zeDeviceGetProperties", json!(name)));
            }
            facts.push(ok_fact(
                "device_type",
                "zeDeviceGetProperties",
                json!(device_type),
            ));
            facts.push(ok_fact(
                "vendor_id",
                "zeDeviceGetProperties",
                json!(properties.vendor_id),
            ));
            facts.push(ok_fact(
                "device_id",
                "zeDeviceGetProperties",
                json!(properties.device_id),
            ));
            facts.push(IntelFact {
                metric_key: "core_clock_mhz".to_string(),
                source_api: "zeDeviceGetProperties",
                state: "ok",
                raw: json!(properties.core_clock_rate),
                unit: Some("mhz"),
                error_message: None,
            });

            let uuid_hex = properties
                .uuid
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            if properties.uuid.iter().any(|byte| *byte != 0) {
                facts.push(ok_fact("uuid", "zeDeviceGetProperties", json!(uuid_hex)));
            }

            extend_sysman_device_facts(
                library,
                sample_seq,
                &key,
                device,
                &mut facts,
                &mut unavailable,
                sysman_ready,
            );

            DeviceRecord {
                driver_index,
                device_index,
                key,
                facts,
                unavailable,
            }
        }
        Ok((_, result)) => {
            let key = format!("driver={driver_index},device={device_index}");
            facts.push(IntelFact {
                metric_key: "device_key".to_string(),
                source_api: "wtg.intel.level_zero.device_key",
                state: "ok",
                raw: json!(key.clone()),
                unit: None,
                error_message: None,
            });
            facts.push(IntelFact {
                metric_key: "device_name".to_string(),
                source_api: "zeDeviceGetProperties",
                state: "error",
                raw: Value::Null,
                unit: None,
                error_message: Some(format!(
                    "zeDeviceGetProperties failed for driver {driver_index} device {device_index} with status {result}."
                )),
            });
            unavailable.extend(["name", "type"]);
            extend_sysman_device_facts(
                library,
                sample_seq,
                &key,
                device,
                &mut facts,
                &mut unavailable,
                sysman_ready,
            );
            DeviceRecord {
                driver_index,
                device_index,
                key,
                facts,
                unavailable,
            }
        }
        Err(error_message) => {
            let key = format!("driver={driver_index},device={device_index}");
            facts.push(IntelFact {
                metric_key: "device_key".to_string(),
                source_api: "wtg.intel.level_zero.device_key",
                state: "ok",
                raw: json!(key.clone()),
                unit: None,
                error_message: None,
            });
            facts.push(IntelFact {
                metric_key: "device_name".to_string(),
                source_api: "zeDeviceGetProperties",
                state: "error",
                raw: Value::Null,
                unit: None,
                error_message: Some(error_message),
            });
            unavailable.extend(["name", "type"]);
            extend_sysman_device_facts(
                library,
                sample_seq,
                &key,
                device,
                &mut facts,
                &mut unavailable,
                sysman_ready,
            );
            DeviceRecord {
                driver_index,
                device_index,
                key,
                facts,
                unavailable,
            }
        }
    }
}

fn query_device_properties(
    library: &LevelZeroLibrary,
    device: *mut c_void,
) -> Result<(ZeDeviceProperties, i32), String> {
    let mut last_result = None;
    for stype in 0..32u32 {
        let mut properties = unsafe { MaybeUninit::<ZeDeviceProperties>::zeroed().assume_init() };
        properties.stype = stype;
        properties.p_next = ptr::null_mut();
        let result = unsafe { (library.ze_device_get_properties)(device, &mut properties) };
        last_result = Some(result);
        if result == ZE_RESULT_SUCCESS {
            return Ok((properties, result));
        }
    }

    Err(format!(
        "zeDeviceGetProperties failed for all attempted structure type values; last status {}.",
        last_result.unwrap_or_default()
    ))
}

fn extend_sysman_device_facts(
    library: &LevelZeroLibrary,
    sample_seq: u64,
    device_key: &str,
    device: *mut c_void,
    facts: &mut Vec<IntelFact>,
    unavailable: &mut Vec<&'static str>,
    sysman_ready: bool,
) {
    for group in library.sysman_domain_groups().iter() {
        if !sysman_ready {
            facts.push(not_available_fact(
                format!("sysman.{}.status", group.domain_key),
                group.enum_source_api,
                "Intel Sysman is unavailable because zesInit did not complete successfully."
                    .to_string(),
            ));
            facts.push(not_available_fact(
                format!("sysman.{}.count", group.domain_key),
                group.enum_source_api,
                "Intel Sysman is unavailable because zesInit did not complete successfully."
                    .to_string(),
            ));
            unavailable.push(group.unavailable_label);
            continue;
        }

        let Some(enum_handles) = group.enum_handles else {
            facts.push(not_available_fact(
                format!("sysman.{}.status", group.domain_key),
                group.enum_source_api,
                format!("missing optional Sysman symbol {}.", group.enum_source_api),
            ));
            facts.push(not_available_fact(
                format!("sysman.{}.count", group.domain_key),
                group.enum_source_api,
                format!("missing optional Sysman symbol {}.", group.enum_source_api),
            ));
            unavailable.push(group.unavailable_label);
            continue;
        };

        let mut handle_count = 0u32;
        let result = unsafe { enum_handles(device, &mut handle_count, ptr::null_mut()) };
        if result != ZE_RESULT_SUCCESS {
            facts.push(error_fact(
                format!("sysman.{}.status", group.domain_key),
                group.enum_source_api,
                result,
                format!(
                    "{}(count) failed with status {result}.",
                    group.enum_source_api
                ),
            ));
            facts.push(error_fact(
                format!("sysman.{}.count", group.domain_key),
                group.enum_source_api,
                result,
                format!(
                    "{}(count) failed with status {result}.",
                    group.enum_source_api
                ),
            ));
            unavailable.push(group.unavailable_label);
            continue;
        }

        facts.push(IntelFact {
            metric_key: format!("sysman.{}.count", group.domain_key),
            source_api: group.enum_source_api,
            state: "ok",
            raw: json!(handle_count),
            unit: None,
            error_message: None,
        });

        if handle_count == 0 {
            facts.push(not_available_fact(
                format!("sysman.{}.status", group.domain_key),
                group.enum_source_api,
                format!("{} returned zero handles.", group.enum_source_api),
            ));
            unavailable.push(group.unavailable_label);
            continue;
        }

        let mut handles = vec![ptr::null_mut(); handle_count as usize];
        let result = unsafe { enum_handles(device, &mut handle_count, handles.as_mut_ptr()) };
        if result != ZE_RESULT_SUCCESS {
            facts.push(error_fact(
                format!("sysman.{}.status", group.domain_key),
                group.enum_source_api,
                result,
                format!(
                    "{}(handles) failed with status {result}.",
                    group.enum_source_api
                ),
            ));
            unavailable.push(group.unavailable_label);
            continue;
        }

        facts.push(ok_fact(
            format!("sysman.{}.status", group.domain_key),
            group.enum_source_api,
            json!("ok"),
        ));

        let mut saw_success = false;
        for (handle_index, handle) in handles.into_iter().enumerate() {
            let mut property_buffer = None;
            let mut state_buffer = None;
            facts.push(ok_fact(
                format!("sysman.{}.{}.handle", group.domain_key, handle_index),
                group.enum_source_api,
                json!(handle_pointer_json(handle_index, handle)),
            ));

            match group.get_properties {
                Some(get_properties) => match query_sysman_buffer(get_properties, handle) {
                    Ok(buffer) => {
                        saw_success = true;
                        property_buffer = Some(buffer);
                        facts.push(ok_fact(
                            format!("sysman.{}.{}.properties", group.domain_key, handle_index),
                            group.property_source_api,
                            json!(handle_buffer_json(handle_index, handle, &buffer)),
                        ));
                    }
                    Err(result) => facts.push(error_fact(
                        format!("sysman.{}.{}.properties", group.domain_key, handle_index),
                        group.property_source_api,
                        result,
                        format!(
                            "{} failed for handle index {handle_index} with status {result}.",
                            group.property_source_api
                        ),
                    )),
                },
                None => facts.push(not_available_fact(
                    format!("sysman.{}.{}.properties", group.domain_key, handle_index),
                    group.property_source_api,
                    format!(
                        "missing optional Sysman symbol {}.",
                        group.property_source_api
                    ),
                )),
            }

            if let Some(get_state) = group.get_state_buffer {
                match query_sysman_buffer(get_state, handle) {
                    Ok(buffer) => {
                        saw_success = true;
                        state_buffer = Some(buffer);
                        facts.push(ok_fact(
                            format!("sysman.{}.{}.state", group.domain_key, handle_index),
                            group.state_source_api,
                            json!(handle_buffer_json(handle_index, handle, &buffer)),
                        ));
                    }
                    Err(result) => facts.push(error_fact(
                        format!("sysman.{}.{}.state", group.domain_key, handle_index),
                        group.state_source_api,
                        result,
                        format!(
                            "{} failed for handle index {handle_index} with status {result}.",
                            group.state_source_api
                        ),
                    )),
                }
            } else if let Some(get_temp_state) = group.get_state_temperature {
                let mut temperature_c = 0.0f64;
                let result = unsafe { get_temp_state(handle, &mut temperature_c) };
                if result == ZE_RESULT_SUCCESS {
                    saw_success = true;
                    facts.push(ok_fact(
                        format!("sysman.{}.{}.state", group.domain_key, handle_index),
                        group.state_source_api,
                        json!({
                            "handle_index": handle_index,
                            "handle_ptr": pointer_hex(handle),
                            "temperature_c": temperature_c
                        }),
                    ));
                } else {
                    facts.push(error_fact(
                        format!("sysman.{}.{}.state", group.domain_key, handle_index),
                        group.state_source_api,
                        result,
                        format!(
                            "{} failed for handle index {handle_index} with status {result}.",
                            group.state_source_api
                        ),
                    ));
                }
            } else {
                facts.push(not_available_fact(
                    format!("sysman.{}.{}.state", group.domain_key, handle_index),
                    group.state_source_api,
                    format!("missing optional Sysman symbol {}.", group.state_source_api),
                ));
            }

            extend_typed_sysman_facts(
                sample_seq,
                device_key,
                group.domain_key,
                handle_index,
                group.property_source_api,
                property_buffer.as_ref(),
                group.state_source_api,
                state_buffer.as_ref(),
                facts,
            );
        }

        if !saw_success {
            unavailable.push(group.unavailable_label);
        }
    }
}

fn query_sysman_buffer(call: ZesGetBuffer, handle: *mut c_void) -> Result<SysmanBuffer, i32> {
    let mut last_result = None;
    for stype in 0..64u32 {
        let mut buffer = SysmanBuffer {
            stype,
            p_next: ptr::null_mut(),
            bytes: [0u8; SYSMAN_BUFFER_BYTES],
        };
        let result = unsafe { call(handle, &mut buffer) };
        last_result = Some(result);
        if result == ZE_RESULT_SUCCESS {
            return Ok(buffer);
        }
    }

    Err(last_result.unwrap_or_default())
}

fn extend_typed_sysman_facts(
    sample_seq: u64,
    device_key: &str,
    domain_key: &str,
    handle_index: usize,
    property_source_api: &'static str,
    property_buffer: Option<&SysmanBuffer>,
    state_source_api: &'static str,
    state_buffer: Option<&SysmanBuffer>,
    facts: &mut Vec<IntelFact>,
) {
    match domain_key {
        "memory_modules" => extend_memory_typed_facts(
            handle_index,
            property_source_api,
            property_buffer,
            state_source_api,
            state_buffer,
            facts,
        ),
        "frequency_domains" => extend_frequency_typed_facts(
            handle_index,
            property_source_api,
            property_buffer,
            state_source_api,
            state_buffer,
            facts,
        ),
        "power_domains" => extend_power_typed_facts(
            sample_seq,
            device_key,
            handle_index,
            state_source_api,
            state_buffer,
            facts,
        ),
        "engine_groups" => extend_engine_typed_facts(
            sample_seq,
            device_key,
            handle_index,
            state_source_api,
            state_buffer,
            facts,
        ),
        _ => {}
    }
}

fn extend_memory_typed_facts(
    handle_index: usize,
    property_source_api: &'static str,
    property_buffer: Option<&SysmanBuffer>,
    state_source_api: &'static str,
    state_buffer: Option<&SysmanBuffer>,
    facts: &mut Vec<IntelFact>,
) {
    let property = property_buffer.and_then(decode_sysman_struct::<ZesMemProperties>);
    let state = state_buffer.and_then(decode_sysman_struct::<ZesMemState>);
    let base_key = format!("sysman.memory_modules.{handle_index}");

    if let Some(property) = property {
        let size_bytes = if property.physical_size > 0 {
            Some(property.physical_size)
        } else {
            state.map(|value| value.size).filter(|value| *value > 0)
        };
        if let Some(size_bytes) = size_bytes {
            facts.push(number_fact_u64(
                format!("{base_key}.size_bytes"),
                property_source_api,
                size_bytes,
                Some("bytes"),
            ));
        }
    }

    if let Some(state) = state {
        facts.push(number_fact_u64(
            format!("{base_key}.free_bytes"),
            state_source_api,
            state.free,
            Some("bytes"),
        ));

        let size_for_used = property
            .map(|value| value.physical_size)
            .filter(|value| *value > 0)
            .or_else(|| (state.size > 0).then_some(state.size));
        if let Some(size_bytes) = size_for_used.filter(|value| state.free <= *value) {
            facts.push(number_fact_u64(
                format!("{base_key}.used_bytes"),
                state_source_api,
                size_bytes.saturating_sub(state.free),
                Some("bytes"),
            ));
        }

        if let Some(health) = mem_health_name(state.health) {
            facts.push(ok_fact(
                format!("{base_key}.health"),
                state_source_api,
                json!(health),
            ));
        }
    }
}

fn extend_frequency_typed_facts(
    handle_index: usize,
    property_source_api: &'static str,
    property_buffer: Option<&SysmanBuffer>,
    state_source_api: &'static str,
    state_buffer: Option<&SysmanBuffer>,
    facts: &mut Vec<IntelFact>,
) {
    let base_key = format!("sysman.frequency_domains.{handle_index}");

    if let Some(property) = property_buffer.and_then(decode_sysman_struct::<ZesFreqProperties>) {
        push_non_negative_f64_fact(
            facts,
            format!("{base_key}.min_mhz"),
            property_source_api,
            property.min,
            Some("mhz"),
        );
        push_non_negative_f64_fact(
            facts,
            format!("{base_key}.max_mhz"),
            property_source_api,
            property.max,
            Some("mhz"),
        );
    }

    if let Some(state) = state_buffer.and_then(decode_sysman_struct::<ZesFreqState>) {
        push_non_negative_f64_fact(
            facts,
            format!("{base_key}.request_mhz"),
            state_source_api,
            state.request,
            Some("mhz"),
        );
        push_non_negative_f64_fact(
            facts,
            format!("{base_key}.actual_mhz"),
            state_source_api,
            state.actual,
            Some("mhz"),
        );
    }
}

fn extend_power_typed_facts(
    _sample_seq: u64,
    device_key: &str,
    handle_index: usize,
    state_source_api: &'static str,
    state_buffer: Option<&SysmanBuffer>,
    facts: &mut Vec<IntelFact>,
) {
    let Some(state) = state_buffer.and_then(decode_sysman_struct::<ZesPowerEnergyCounter>) else {
        return;
    };

    let base_key = format!("sysman.power_domains.{handle_index}");
    facts.push(number_fact_f64(
        format!("{base_key}.energy_j"),
        state_source_api,
        state.energy as f64 / 1_000_000.0,
        Some("joules"),
    ));
    facts.push(number_fact_u64(
        format!("{base_key}.timestamp_ns"),
        state_source_api,
        state.timestamp.saturating_mul(1_000),
        Some("ns"),
    ));

    let delta_key = format!("{device_key}::{base_key}");
    match update_power_delta(delta_key, state) {
        DeltaValue::Ok(watts) => facts.push(number_fact_f64(
            format!("{base_key}.watts_delta"),
            state_source_api,
            watts,
            Some("watts"),
        )),
        DeltaValue::NotAvailable(error_message) => facts.push(not_available_fact(
            format!("{base_key}.watts_delta"),
            state_source_api,
            error_message,
        )),
    }
}

fn extend_engine_typed_facts(
    _sample_seq: u64,
    device_key: &str,
    handle_index: usize,
    state_source_api: &'static str,
    state_buffer: Option<&SysmanBuffer>,
    facts: &mut Vec<IntelFact>,
) {
    let Some(state) = state_buffer.and_then(decode_sysman_struct::<ZesEngineStats>) else {
        return;
    };

    let base_key = format!("sysman.engine_groups.{handle_index}");
    facts.push(number_fact_u64(
        format!("{base_key}.active_time_ns"),
        state_source_api,
        state.active_time,
        Some("ns"),
    ));
    facts.push(number_fact_u64(
        format!("{base_key}.timestamp_ns"),
        state_source_api,
        state.timestamp,
        Some("ns"),
    ));

    let delta_key = format!("{device_key}::{base_key}");
    match update_engine_delta(delta_key, state) {
        DeltaValue::Ok(utilization_pct) => facts.push(number_fact_f64(
            format!("{base_key}.utilization_pct_delta"),
            state_source_api,
            utilization_pct,
            Some("pct"),
        )),
        DeltaValue::NotAvailable(error_message) => facts.push(not_available_fact(
            format!("{base_key}.utilization_pct_delta"),
            state_source_api,
            error_message,
        )),
    }
}

fn update_power_delta(key: String, current: ZesPowerEnergyCounter) -> DeltaValue {
    let cache = POWER_DELTA_BASELINES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().expect("power delta cache should lock");
    let previous = cache.insert(
        key,
        PowerDeltaBaseline {
            energy_uj: current.energy,
            timestamp_us: current.timestamp,
        },
    );

    let Some(previous) = previous else {
        return DeltaValue::NotAvailable("requires previous sample.".to_string());
    };
    if current.timestamp <= previous.timestamp_us {
        return DeltaValue::NotAvailable("requires positive timestamp delta.".to_string());
    }
    if current.energy < previous.energy_uj {
        return DeltaValue::NotAvailable("counter reset detected.".to_string());
    }
    let delta_time_us = current.timestamp - previous.timestamp_us;
    let delta_energy_uj = current.energy - previous.energy_uj;
    DeltaValue::Ok(delta_energy_uj as f64 / delta_time_us as f64)
}

fn update_engine_delta(key: String, current: ZesEngineStats) -> DeltaValue {
    let cache = ENGINE_DELTA_BASELINES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().expect("engine delta cache should lock");
    let previous = cache.insert(
        key,
        EngineDeltaBaseline {
            active_time: current.active_time,
            timestamp: current.timestamp,
        },
    );

    let Some(previous) = previous else {
        return DeltaValue::NotAvailable("requires previous sample.".to_string());
    };
    if current.timestamp <= previous.timestamp {
        return DeltaValue::NotAvailable("requires positive timestamp delta.".to_string());
    }
    if current.active_time < previous.active_time {
        return DeltaValue::NotAvailable("counter reset detected.".to_string());
    }
    let delta_timestamp = current.timestamp - previous.timestamp;
    let delta_active_time = current.active_time - previous.active_time;
    DeltaValue::Ok((delta_active_time as f64 * 100.0) / delta_timestamp as f64)
}

enum DeltaValue {
    Ok(f64),
    NotAvailable(String),
}

fn decode_sysman_struct<T: Copy>(buffer: &SysmanBuffer) -> Option<T> {
    if size_of::<T>() > size_of::<SysmanBuffer>() {
        return None;
    }
    Some(unsafe { ptr::read_unaligned((buffer as *const SysmanBuffer).cast::<T>()) })
}

fn mem_health_name(value: u32) -> Option<&'static str> {
    match value {
        0 => Some("unknown"),
        1 => Some("ok"),
        2 => Some("degraded"),
        3 => Some("critical"),
        4 => Some("replace"),
        _ => None,
    }
}

fn push_non_negative_f64_fact(
    facts: &mut Vec<IntelFact>,
    metric_key: String,
    source_api: &'static str,
    value: f64,
    unit: Option<&'static str>,
) {
    if value >= 0.0 {
        facts.push(number_fact_f64(metric_key, source_api, value, unit));
    }
}

fn find_fact_any<'a>(device: &'a DeviceRecord, metric_key: &str) -> Option<&'a IntelFact> {
    device
        .facts
        .iter()
        .find(|fact| fact.metric_key == metric_key)
}

fn fact_reason(device: &DeviceRecord, metric_key: &str) -> Option<String> {
    let fact = find_fact_any(device, metric_key)?;
    match fact.state {
        "not_available" | "error" => fact
            .error_message
            .as_deref()
            .map(concise_unavailable_reason),
        _ => None,
    }
}

fn concise_unavailable_reason(message: &str) -> String {
    let message = message.trim().trim_end_matches('.');
    if message.ends_with("returned zero handles") {
        return "zero handles".to_string();
    }
    if message.starts_with("missing optional Sysman symbol ") {
        return "missing optional symbol".to_string();
    }
    if message == "Intel Sysman is unavailable because zesInit did not complete successfully" {
        return "Sysman unavailable".to_string();
    }
    message.to_string()
}

fn format_human_unavailable(label: &str, reason: Option<String>) -> String {
    match reason {
        Some(reason) if !reason.is_empty() => format!("{label}: unavailable, {reason}"),
        _ => format!("{label}: unavailable"),
    }
}

fn format_decimal(value: f64, decimals: usize) -> String {
    let rendered = format!("{value:.decimals$}");
    if let Some(trimmed) = rendered.strip_suffix(".0") {
        trimmed.to_string()
    } else {
        rendered
    }
}

fn format_mhz(value: f64) -> String {
    format_decimal(value, 1)
}

fn format_mib(bytes: u64) -> String {
    format_decimal(bytes as f64 / 1024.0 / 1024.0, 1)
}

fn format_watts(value: f64) -> String {
    format_decimal(value, 1)
}

fn format_pct_delta(value: f64) -> String {
    format!("{value:.2}")
}

fn indexed_metric_indexes(device: &DeviceRecord, prefix: &str, suffix: &str) -> Vec<usize> {
    let mut indexes = BTreeSet::new();
    for fact in device.facts.iter() {
        let Some(index_text) = fact
            .metric_key
            .strip_prefix(prefix)
            .and_then(|rest| rest.strip_suffix(suffix))
        else {
            continue;
        };
        if let Ok(index) = index_text.parse::<usize>() {
            indexes.insert(index);
        }
    }
    indexes.into_iter().collect()
}

fn sum_indexed_u64_facts(device: &DeviceRecord, prefix: &str, suffix: &str) -> Option<u64> {
    let mut total = 0u64;
    let mut saw_any = false;
    for index in indexed_metric_indexes(device, prefix, suffix) {
        if let Some(value) = fact_u64(device, &format!("{prefix}{index}{suffix}")) {
            total = total.saturating_add(value);
            saw_any = true;
        }
    }
    saw_any.then_some(total)
}

fn push_memory_line(lines: &mut Vec<String>, device: &DeviceRecord) {
    let prefix = "sysman.memory_modules.";
    let used = sum_indexed_u64_facts(device, prefix, ".used_bytes");
    let size = sum_indexed_u64_facts(device, prefix, ".size_bytes");
    match (used, size) {
        (Some(used), Some(size)) => lines.push(format!(
            "  Memory: {} MiB / {} MiB",
            format_mib(used),
            format_mib(size)
        )),
        _ => lines.push(format_human_unavailable(
            "  Memory",
            fact_reason(device, "sysman.memory_modules.status"),
        )),
    }
}

fn push_power_line(lines: &mut Vec<String>, device: &DeviceRecord) {
    let prefix = "sysman.power_domains.";
    let indexes = indexed_metric_indexes(device, prefix, ".watts_delta");
    if let Some(index) = indexes.first().copied() {
        let metric_key = format!("{prefix}{index}.watts_delta");
        if let Some(value) = fact_number(device, &metric_key) {
            lines.push(format!("  Power: {} W", format_watts(value)));
        } else {
            lines.push(format_human_unavailable(
                "  Power",
                fact_reason(device, &metric_key)
                    .or_else(|| fact_reason(device, "sysman.power_domains.status")),
            ));
        }
        return;
    }

    lines.push(format_human_unavailable(
        "  Power",
        fact_reason(device, "sysman.power_domains.status"),
    ));
}

fn push_engine_lines(lines: &mut Vec<String>, device: &DeviceRecord) {
    let prefix = "sysman.engine_groups.";
    let indexes = indexed_metric_indexes(device, prefix, ".utilization_pct_delta");
    if indexes.is_empty() {
        lines.push(format_human_unavailable(
            "  Engine activity",
            fact_reason(device, "sysman.engine_groups.status"),
        ));
        return;
    }

    for index in indexes {
        let metric_key = format!("{prefix}{index}.utilization_pct_delta");
        if let Some(value) = fact_number(device, &metric_key) {
            lines.push(format!("  Engine {index}: {}%", format_pct_delta(value)));
        } else {
            lines.push(format_human_unavailable(
                &format!("  Engine {index}"),
                fact_reason(device, &metric_key)
                    .or_else(|| fact_reason(device, "sysman.engine_groups.status")),
            ));
        }
    }
}

fn push_frequency_line(lines: &mut Vec<String>, device: &DeviceRecord) {
    let prefix = "sysman.frequency_domains.";
    let mut indexes = indexed_metric_indexes(device, prefix, ".actual_mhz");
    indexes.extend(indexed_metric_indexes(device, prefix, ".request_mhz"));
    indexes.sort_unstable();
    indexes.dedup();
    for index in indexes {
        let actual = fact_number(device, &format!("{prefix}{index}.actual_mhz"));
        let request = fact_number(device, &format!("{prefix}{index}.request_mhz"));
        if actual.is_none() && request.is_none() {
            continue;
        }

        let mut parts = Vec::new();
        if let Some(actual) = actual {
            parts.push(format!("actual {} MHz", format_mhz(actual)));
        }
        if let Some(request) = request {
            parts.push(format!("requested {} MHz", format_mhz(request)));
        }
        lines.push(format!("  Frequency: {}", parts.join(", ")));
        return;
    }

    lines.push(format_human_unavailable(
        "  Frequency",
        fact_reason(device, "sysman.frequency_domains.status"),
    ));
}

fn temperature_reading_c(device: &DeviceRecord) -> Option<f64> {
    let prefix = "sysman.temperature_sensors.";
    for index in indexed_metric_indexes(device, prefix, ".state") {
        let Some(fact) = find_fact_any(device, &format!("{prefix}{index}.state")) else {
            continue;
        };
        let Some(raw) = fact.raw.as_object() else {
            continue;
        };
        if let Some(value) = raw.get("temperature_c").and_then(Value::as_f64) {
            return Some(value);
        }
    }
    None
}

fn push_temperature_line(lines: &mut Vec<String>, device: &DeviceRecord) {
    if let Some(temperature_c) = temperature_reading_c(device) {
        lines.push(format!(
            "  Temperature: {} C",
            format_decimal(temperature_c, 1)
        ));
        return;
    }

    lines.push(format_human_unavailable(
        "  Temperature",
        fact_reason(device, "sysman.temperature_sensors.status"),
    ));
}

fn push_compact_device_lines(lines: &mut Vec<String>, device: &DeviceRecord) {
    if let Some(device_name) = fact_string(device, "device_name") {
        lines.push(format!(
            "Intel device {}: {device_name}",
            device.device_index
        ));
    } else {
        lines.push(format!(
            "Intel device {} [{}]",
            device.device_index, device.key
        ));
    }
    lines.push(format!("  Device key: {}", device.key));

    if let Some(uuid) = fact_string(device, "uuid") {
        lines.push(format!("  UUID: {uuid}"));
    }
    if let Some(core_clock_mhz) = fact_number(device, "core_clock_mhz") {
        lines.push(format!("  Core clock: {} MHz", format_mhz(core_clock_mhz)));
    }

    push_memory_line(lines, device);
    push_power_line(lines, device);
    push_engine_lines(lines, device);
    push_frequency_line(lines, device);
    push_temperature_line(lines, device);
}

fn push_snapshot_device_lines(lines: &mut Vec<String>, device: &DeviceRecord) {
    push_compact_device_lines(lines, device);
}

fn push_watch_device_lines(lines: &mut Vec<String>, device: &DeviceRecord) {
    push_compact_device_lines(lines, device);
}

fn push_probe_device_lines(lines: &mut Vec<String>, device: &DeviceRecord) {
    if let Some(device_name) = fact_string(device, "device_name") {
        lines.push(format!("device.name: {device_name}"));
    }
    lines.push(format!("device.key: {}", device.key));
    if let Some(device_type) = fact_string(device, "device_type") {
        lines.push(format!("device.type: {device_type}"));
    }
    if let Some(vendor_id) = fact_u64(device, "vendor_id") {
        lines.push(format!("device.vendor_id: 0x{vendor_id:04x} ({vendor_id})"));
    }
    if let Some(device_id) = fact_u64(device, "device_id") {
        lines.push(format!("device.device_id: 0x{device_id:04x} ({device_id})"));
    }
    if let Some(uuid) = fact_string(device, "uuid") {
        lines.push(format!("device.uuid: {uuid}"));
    }
    if let Some(core_clock_mhz) = fact_number(device, "core_clock_mhz") {
        lines.push(format!("device.core_clock_mhz: {core_clock_mhz:.1}"));
    }
    for fact in device.facts.iter() {
        if is_base_probe_fact_key(&fact.metric_key) {
            continue;
        }
        lines.push(format_probe_fact_line("device", fact));
    }
    if !device.unavailable.is_empty() {
        lines.push(format!("unavailable: {}", device.unavailable.join(", ")));
    }
}

fn stats_device_json(device: &DeviceRecord) -> Value {
    let mut object = serde_json::Map::new();
    for fact in device.facts.iter() {
        let mut entry = serde_json::Map::new();
        entry.insert("raw".to_string(), fact.raw.clone());
        entry.insert(
            "source_api".to_string(),
            Value::String(fact.source_api.to_string()),
        );
        entry.insert("state".to_string(), Value::String(fact.state.to_string()));
        if let Some(unit) = fact.unit {
            entry.insert("unit".to_string(), Value::String(unit.to_string()));
        }
        if let Some(error_message) = &fact.error_message {
            entry.insert(
                "error_message".to_string(),
                Value::String(error_message.clone()),
            );
        }
        object.insert(fact.metric_key.clone(), Value::Object(entry));
    }
    if !device.unavailable.is_empty() {
        object.insert(
            "unavailable".to_string(),
            stats_field(
                json!(device.unavailable),
                "wtg.intel.level_zero.unavailable_summary",
                "ok",
                None,
                None,
            ),
        );
    }
    Value::Object(object)
}

fn stats_facts_json(facts: &[IntelFact]) -> Value {
    let mut object = serde_json::Map::new();
    for fact in facts.iter() {
        let mut entry = serde_json::Map::new();
        entry.insert("raw".to_string(), fact.raw.clone());
        entry.insert(
            "source_api".to_string(),
            Value::String(fact.source_api.to_string()),
        );
        entry.insert("state".to_string(), Value::String(fact.state.to_string()));
        if let Some(unit) = fact.unit {
            entry.insert("unit".to_string(), Value::String(unit.to_string()));
        }
        if let Some(error_message) = &fact.error_message {
            entry.insert(
                "error_message".to_string(),
                Value::String(error_message.clone()),
            );
        }
        object.insert(fact.metric_key.clone(), Value::Object(entry));
    }
    Value::Object(object)
}

fn ok_fact(metric_key: impl Into<String>, source_api: &'static str, raw: Value) -> IntelFact {
    IntelFact {
        metric_key: metric_key.into(),
        source_api,
        state: "ok",
        raw,
        unit: None,
        error_message: None,
    }
}

fn number_fact_u64(
    metric_key: impl Into<String>,
    source_api: &'static str,
    raw: u64,
    unit: Option<&'static str>,
) -> IntelFact {
    IntelFact {
        metric_key: metric_key.into(),
        source_api,
        state: "ok",
        raw: json!(raw),
        unit,
        error_message: None,
    }
}

fn number_fact_f64(
    metric_key: impl Into<String>,
    source_api: &'static str,
    raw: f64,
    unit: Option<&'static str>,
) -> IntelFact {
    IntelFact {
        metric_key: metric_key.into(),
        source_api,
        state: "ok",
        raw: json!(raw),
        unit,
        error_message: None,
    }
}

fn not_available_fact(
    metric_key: impl Into<String>,
    source_api: &'static str,
    error_message: String,
) -> IntelFact {
    IntelFact {
        metric_key: metric_key.into(),
        source_api,
        state: "not_available",
        raw: Value::Null,
        unit: None,
        error_message: Some(error_message),
    }
}

fn error_fact(
    metric_key: impl Into<String>,
    source_api: &'static str,
    raw_code: i32,
    error_message: String,
) -> IntelFact {
    IntelFact {
        metric_key: metric_key.into(),
        source_api,
        state: "error",
        raw: json!(raw_code),
        unit: None,
        error_message: Some(error_message),
    }
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
    object.insert("source_api".to_string(), json!(source_api));
    object.insert("state".to_string(), json!(state));
    if let Some(unit) = unit {
        object.insert("unit".to_string(), json!(unit));
    }
    if let Some(error_message) = error_message {
        object.insert("error_message".to_string(), json!(error_message));
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
    stats_field(json!(raw), source_api, state, unit, error_message)
}

fn format_probe_fact_line(prefix: &str, fact: &IntelFact) -> String {
    let raw = serde_json::to_string(&fact.raw).unwrap_or_else(|_| "null".to_string());
    match &fact.error_message {
        Some(error_message) => format!(
            "{prefix}.{}: {} (raw={raw}) [{error_message}]",
            fact.metric_key, fact.state
        ),
        None => format!("{prefix}.{}: {} (raw={raw})", fact.metric_key, fact.state),
    }
}

fn is_base_probe_fact_key(metric_key: &str) -> bool {
    matches!(
        metric_key,
        "device_name"
            | "device_key"
            | "device_type"
            | "vendor_id"
            | "device_id"
            | "uuid"
            | "core_clock_mhz"
    )
}

fn handle_pointer_json(handle_index: usize, handle: *mut c_void) -> Value {
    json!({
        "handle_index": handle_index,
        "handle_ptr": pointer_hex(handle)
    })
}

fn handle_buffer_json(handle_index: usize, handle: *mut c_void, buffer: &SysmanBuffer) -> Value {
    json!({
        "handle_index": handle_index,
        "handle_ptr": pointer_hex(handle),
        "stype": buffer.stype,
        "buffer_hex": sysman_buffer_hex(buffer)
    })
}

fn fact_string<'a>(device: &'a DeviceRecord, metric_key: &str) -> Option<&'a str> {
    device
        .facts
        .iter()
        .find(|fact| fact.metric_key == metric_key && fact.state == "ok")
        .and_then(|fact| fact.raw.as_str())
}

fn fact_number(device: &DeviceRecord, metric_key: &str) -> Option<f64> {
    device
        .facts
        .iter()
        .find(|fact| fact.metric_key == metric_key && fact.state == "ok")
        .and_then(|fact| fact.raw.as_f64())
}

fn fact_u64(device: &DeviceRecord, metric_key: &str) -> Option<u64> {
    device
        .facts
        .iter()
        .find(|fact| fact.metric_key == metric_key && fact.state == "ok")
        .and_then(|fact| fact.raw.as_u64())
}

fn ze_device_type_name(device_type: u32) -> &'static str {
    match device_type {
        1 => "gpu",
        2 => "cpu",
        3 => "fpga",
        4 => "mca",
        5 => "vpu",
        _ => "unknown",
    }
}

unsafe fn load_symbol<T>(module: NonNull<c_void>, name: &[u8]) -> Result<T, String> {
    let symbol = GetProcAddress(module.as_ptr(), name.as_ptr().cast());
    if symbol.is_null() {
        let label = symbol_label(name);
        return Err(format!("missing Level Zero symbol {label}."));
    }

    Ok(std::mem::transmute_copy(&symbol))
}

unsafe fn load_optional_symbol<T>(module: NonNull<c_void>, name: &[u8]) -> Option<T> {
    let symbol = GetProcAddress(module.as_ptr(), name.as_ptr().cast());
    if symbol.is_null() {
        None
    } else {
        Some(std::mem::transmute_copy(&symbol))
    }
}

unsafe fn has_symbol(module: NonNull<c_void>, name: &[u8]) -> bool {
    !GetProcAddress(module.as_ptr(), name.as_ptr().cast()).is_null()
}

unsafe fn module_path(module: *mut c_void) -> Result<String, String> {
    let mut buffer = [0u16; 260];
    let len = GetModuleFileNameW(module, buffer.as_mut_ptr(), buffer.len() as u32);
    if len == 0 {
        return Err("failed to query loaded Level Zero module path".to_string());
    }

    Ok(String::from_utf16_lossy(&buffer[..len as usize]))
}

fn c_string(raw: &[c_char]) -> String {
    unsafe { CStr::from_ptr(raw.as_ptr()) }
        .to_string_lossy()
        .trim()
        .to_string()
}

fn symbol_label(name: &[u8]) -> String {
    String::from_utf8_lossy(&name[..name.len().saturating_sub(1)]).to_string()
}

fn pointer_hex(handle: *mut c_void) -> String {
    format!("0x{:x}", handle as usize)
}

fn sysman_buffer_hex(buffer: &SysmanBuffer) -> String {
    let bytes = unsafe {
        std::slice::from_raw_parts(
            (buffer as *const SysmanBuffer).cast::<u8>(),
            std::mem::size_of::<SysmanBuffer>(),
        )
    };
    let last_non_zero = bytes
        .iter()
        .rposition(|byte| *byte != 0)
        .map(|index| index + 1);
    bytes[..last_non_zero.unwrap_or(0)]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
fn clear_delta_caches_for_tests() {
    if let Some(cache) = POWER_DELTA_BASELINES.get() {
        cache.lock().expect("power delta cache should lock").clear();
    }
    if let Some(cache) = ENGINE_DELTA_BASELINES.get() {
        cache
            .lock()
            .expect("engine delta cache should lock")
            .clear();
    }
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
    use std::ffi::c_void;
    use std::mem::{size_of, ManuallyDrop, MaybeUninit};
    use std::ptr::NonNull;

    use serde_json::json;

    use super::{
        build_device_record, clear_delta_caches_for_tests,
        collect_visible_sample_with_priming_impl, extend_typed_sysman_facts, format_probe_snapshot,
        format_snapshot, format_stats_snapshot_json, format_watch_sample, not_available_fact,
        number_fact_f64, number_fact_u64, ok_fact, sample_status, update_engine_delta,
        update_power_delta, DeltaValue, DeviceRecord, IntelFact, LevelZeroLibrary, ProviderSample,
        SysmanBuffer, ZeDeviceProperties, ZesEngineStats, ZesFreqProperties, ZesFreqState,
        ZesMemProperties, ZesMemState, ZesPowerEnergyCounter, PROVIDER, PROVIDER_AUTHORITY, SOURCE,
        STATS_SCHEMA, SYSMAN_BUFFER_BYTES, TELEMETRY_CLASS, ZE_RESULT_SUCCESS,
    };

    unsafe extern "C" fn stub_init(_: u32) -> i32 {
        0
    }

    unsafe extern "C" fn stub_driver_get(_: *mut u32, _: *mut *mut c_void) -> i32 {
        0
    }

    unsafe extern "C" fn stub_device_get(_: *mut c_void, _: *mut u32, _: *mut *mut c_void) -> i32 {
        0
    }

    unsafe extern "C" fn stub_device_get_properties(
        _: *mut c_void,
        _: *mut ZeDeviceProperties,
    ) -> i32 {
        0
    }

    unsafe extern "C" fn stub_enum_handles(
        _: *mut c_void,
        _: *mut u32,
        _: *mut *mut c_void,
    ) -> i32 {
        0
    }

    unsafe extern "C" fn stub_get_buffer(_: *mut c_void, _: *mut SysmanBuffer) -> i32 {
        0
    }

    unsafe extern "C" fn stub_get_temperature(_: *mut c_void, _: *mut f64) -> i32 {
        0
    }

    fn provider_sample_for_test(status: &'static str, sample_seq: u64) -> ProviderSample {
        ProviderSample {
            wtg_version: "0.3.0",
            provider: PROVIDER,
            provider_authority: PROVIDER_AUTHORITY,
            provider_source: SOURCE,
            telemetry_class: TELEMETRY_CLASS,
            status,
            sample_seq,
            timestamp_unix_ms: 0,
            dll_name: None,
            dll_path: None,
            telemetry_exports_matched: 0,
            sysman_exports_matched: 0,
            optional_calls_attempted: 0,
            optional_calls_ok: 0,
            optional_calls_unsupported: 0,
            optional_calls_not_available: 0,
            optional_calls_error: 0,
            driver_record_count: 0,
            device_record_count: 0,
            sysman_facts: Vec::new(),
            devices: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn stub_library() -> ManuallyDrop<LevelZeroLibrary> {
        ManuallyDrop::new(LevelZeroLibrary {
            module: NonNull::dangling(),
            dll_name: "ze_loader.dll".to_string(),
            dll_path: "C:\\Intel\\ze_loader.dll".to_string(),
            ze_init: stub_init,
            ze_driver_get: stub_driver_get,
            ze_device_get: stub_device_get,
            ze_device_get_properties: stub_device_get_properties,
            zes_init: Some(stub_init),
            zes_device_enum_engine_groups: Some(stub_enum_handles),
            zes_engine_get_properties: Some(stub_get_buffer),
            zes_engine_get_activity: Some(stub_get_buffer),
            zes_device_enum_memory_modules: Some(stub_enum_handles),
            zes_memory_get_properties: Some(stub_get_buffer),
            zes_memory_get_state: Some(stub_get_buffer),
            zes_device_enum_power_domains: Some(stub_enum_handles),
            zes_power_get_properties: Some(stub_get_buffer),
            zes_power_get_energy_counter: Some(stub_get_buffer),
            zes_device_enum_temperature_sensors: Some(stub_enum_handles),
            zes_temperature_get_properties: Some(stub_get_buffer),
            zes_temperature_get_state: Some(stub_get_temperature),
            zes_device_enum_frequency_domains: Some(stub_enum_handles),
            zes_frequency_get_properties: Some(stub_get_buffer),
            zes_frequency_get_state: Some(stub_get_buffer),
        })
    }

    #[test]
    fn provider_constants_match_contract() {
        assert_eq!(PROVIDER, "intel");
        assert_eq!(PROVIDER_AUTHORITY, "Intel Level Zero");
        assert_eq!(SOURCE, "wtg.provider.intel.level_zero");
        assert_eq!(TELEMETRY_CLASS, "provider_telemetry");
        assert_eq!(STATS_SCHEMA, "wtg.intel_level_zero.stats.v3");
    }

    #[test]
    fn visible_sample_priming_collects_twice_when_priming_succeeds() {
        let mut collected = Vec::new();
        let mut slept = Vec::new();

        let sample = collect_visible_sample_with_priming_impl(
            0,
            250,
            |sample_seq| {
                collected.push(sample_seq);
                if collected.len() == 1 {
                    provider_sample_for_test("ok", sample_seq)
                } else {
                    provider_sample_for_test("ok", sample_seq + 10)
                }
            },
            |duration| slept.push(duration.as_millis() as u64),
        );

        assert_eq!(collected, vec![0, 0]);
        assert_eq!(slept, vec![250]);
        assert_eq!(sample.sample_seq, 10);
        assert_eq!(sample.status, "ok");
    }

    #[test]
    fn visible_sample_priming_returns_first_sample_when_priming_is_unavailable() {
        let mut collected = Vec::new();
        let mut slept = false;

        let sample = collect_visible_sample_with_priming_impl(
            7,
            250,
            |sample_seq| {
                collected.push(sample_seq);
                provider_sample_for_test("unavailable", sample_seq)
            },
            |_| slept = true,
        );

        assert_eq!(collected, vec![7]);
        assert!(!slept);
        assert_eq!(sample.sample_seq, 7);
        assert_eq!(sample.status, "unavailable");
    }

    #[test]
    fn empty_device_name_is_not_reported_as_ok() {
        let mut properties = unsafe { MaybeUninit::<ZeDeviceProperties>::zeroed().assume_init() };
        properties.device_type = 1;
        properties.vendor_id = 0x8086;
        properties.device_id = 0x4c8b;
        properties.core_clock_rate = 1300;
        properties.uuid = [
            0x24, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x86, 0x80, 0x8b, 0x4c, 0x04, 0x00,
            0x00, 0x00,
        ];

        let library = stub_library();
        let device = build_device_record(
            &library,
            0,
            0,
            0,
            std::ptr::null_mut(),
            Ok((properties, ZE_RESULT_SUCCESS)),
            false,
        );
        let name_fact = device
            .facts
            .iter()
            .find(|fact| fact.metric_key == "device_name")
            .expect("device_name fact");

        assert_eq!(name_fact.state, "not_available");
        assert!(name_fact.raw.is_null());
        assert!(device.unavailable.contains(&"name"));

        let sample = ProviderSample {
            wtg_version: "0.2.9",
            provider: PROVIDER,
            provider_authority: PROVIDER_AUTHORITY,
            provider_source: SOURCE,
            telemetry_class: TELEMETRY_CLASS,
            status: "ok",
            sample_seq: 0,
            timestamp_unix_ms: 0,
            dll_name: None,
            dll_path: None,
            telemetry_exports_matched: 4,
            sysman_exports_matched: 0,
            optional_calls_attempted: 1,
            optional_calls_ok: 1,
            optional_calls_unsupported: 0,
            optional_calls_not_available: 0,
            optional_calls_error: 0,
            driver_record_count: 1,
            device_record_count: 1,
            sysman_facts: Vec::new(),
            devices: vec![device],
            errors: Vec::new(),
        };

        assert_eq!(sample_status(&sample), "ok");
        let rendered = format_snapshot(&sample);
        assert!(rendered.contains("Intel device 0 [driver=0,device=0,vendor=0x8086,device=0x4c8b]"));
        assert!(rendered.contains("Device key: driver=0,device=0,vendor=0x8086,device=0x4c8b"));
        assert!(rendered.contains("UUID: 240000002000000086808b4c04000000"));
        assert!(rendered.contains("Core clock: 1300 MHz"));
        assert!(rendered.contains("Memory: unavailable"));
        assert!(rendered.contains("Power: unavailable"));
        assert!(rendered.contains("Engine activity: unavailable"));
        assert!(rendered.contains("Frequency: unavailable"));
        assert!(rendered.contains("Temperature: unavailable"));
    }

    #[test]
    fn sysman_probe_and_stats_output_remain_provider_scoped() {
        let sample = ProviderSample {
            wtg_version: "0.2.9",
            provider: PROVIDER,
            provider_authority: PROVIDER_AUTHORITY,
            provider_source: SOURCE,
            telemetry_class: TELEMETRY_CLASS,
            status: "ok",
            sample_seq: 7,
            timestamp_unix_ms: 1234,
            dll_name: Some("ze_loader.dll".to_string()),
            dll_path: Some("C:\\Intel\\ze_loader.dll".to_string()),
            telemetry_exports_matched: 4,
            sysman_exports_matched: 1,
            optional_calls_attempted: 0,
            optional_calls_ok: 0,
            optional_calls_unsupported: 0,
            optional_calls_not_available: 0,
            optional_calls_error: 0,
            driver_record_count: 1,
            device_record_count: 0,
            sysman_facts: vec![
                IntelFact {
                    metric_key: "zesInit_export".to_string(),
                    source_api: "wtg.intel.level_zero.dynamic_load",
                    state: "ok",
                    raw: json!(true),
                    unit: None,
                    error_message: None,
                },
                IntelFact {
                    metric_key: "zesDeviceEnumEngineGroups_export".to_string(),
                    source_api: "wtg.intel.level_zero.dynamic_load",
                    state: "not_available",
                    raw: json!(false),
                    unit: None,
                    error_message: Some(
                        "missing optional Sysman symbol zesDeviceEnumEngineGroups.".to_string(),
                    ),
                },
                IntelFact {
                    metric_key: "zesInit_result".to_string(),
                    source_api: "zesInit",
                    state: "error",
                    raw: json!(-7),
                    unit: None,
                    error_message: Some("zesInit failed with status -7.".to_string()),
                },
            ],
            devices: Vec::new(),
            errors: Vec::new(),
        };

        let probe = format_probe_snapshot(&sample);
        assert!(probe.contains("intel.sysman_exports_matched: 1"));
        assert!(probe.contains("intel.sysman.zesInit_export: ok (raw=true)"));
        assert!(probe
            .contains("intel.sysman.zesDeviceEnumEngineGroups_export: not_available (raw=false)"));
        assert!(probe.contains("intel.sysman.zesInit_result: error (raw=-7)"));

        let stats = format_stats_snapshot_json(&sample, 11, "2026-07-03T22:00:00Z");
        let parsed: serde_json::Value =
            serde_json::from_str(&stats).expect("stats payload should parse");
        assert_eq!(parsed["schema"], "wtg.intel_level_zero.stats.v3");
        assert_eq!(parsed["intel"]["sysman_exports_matched"]["raw"], 1);
        assert_eq!(parsed["intel"]["sysman"]["zesInit_export"]["state"], "ok");
        assert_eq!(
            parsed["intel"]["sysman"]["zesDeviceEnumEngineGroups_export"]["state"],
            "not_available"
        );
        assert_eq!(
            parsed["intel"]["sysman"]["zesInit_result"]["state"],
            "error"
        );
        assert_eq!(parsed["intel"]["sysman"]["zesInit_result"]["raw"], -7);
    }

    #[test]
    fn probe_output_includes_existing_device_sysman_facts() {
        let sample = ProviderSample {
            wtg_version: "0.2.9",
            provider: PROVIDER,
            provider_authority: PROVIDER_AUTHORITY,
            provider_source: SOURCE,
            telemetry_class: TELEMETRY_CLASS,
            status: "ok",
            sample_seq: 1,
            timestamp_unix_ms: 1234,
            dll_name: None,
            dll_path: None,
            telemetry_exports_matched: 4,
            sysman_exports_matched: 16,
            optional_calls_attempted: 1,
            optional_calls_ok: 1,
            optional_calls_unsupported: 0,
            optional_calls_not_available: 0,
            optional_calls_error: 0,
            driver_record_count: 1,
            device_record_count: 1,
            sysman_facts: Vec::new(),
            devices: vec![DeviceRecord {
                driver_index: 0,
                device_index: 0,
                key: "driver=0,device=0,vendor=0x8086,device=0x4c8b".to_string(),
                facts: vec![
                    ok_fact(
                        "device_key",
                        "wtg.intel.level_zero.device_key",
                        json!("driver=0,device=0,vendor=0x8086,device=0x4c8b"),
                    ),
                    ok_fact("device_type", "zeDeviceGetProperties", json!("gpu")),
                    ok_fact("vendor_id", "zeDeviceGetProperties", json!(0x8086)),
                    ok_fact("device_id", "zeDeviceGetProperties", json!(0x4c8b)),
                    ok_fact(
                        "sysman.engine_groups.count",
                        "zesDeviceEnumEngineGroups",
                        json!(3),
                    ),
                    ok_fact(
                        "sysman.engine_groups.status",
                        "zesDeviceEnumEngineGroups",
                        json!("ok"),
                    ),
                    ok_fact(
                        "sysman.engine_groups.0.state",
                        "zesEngineGetActivity",
                        json!({"handle_index": 0, "buffer_hex": "abcd"}),
                    ),
                    not_available_fact(
                        "sysman.temperature_sensors.status",
                        "zesDeviceEnumTemperatureSensors",
                        "zesDeviceEnumTemperatureSensors returned zero handles.".to_string(),
                    ),
                    ok_fact(
                        "sysman.temperature_sensors.count",
                        "zesDeviceEnumTemperatureSensors",
                        json!(0),
                    ),
                ],
                unavailable: vec!["temperature"],
            }],
            errors: Vec::new(),
        };

        let probe = format_probe_snapshot(&sample);
        assert!(probe.contains("device.sysman.engine_groups.count: ok (raw=3)"));
        assert!(probe.contains("device.sysman.engine_groups.status: ok (raw=\"ok\")"));
        assert!(probe.contains("device.sysman.engine_groups.0.state: ok"));
        assert!(probe.contains("device.sysman.temperature_sensors.count: ok (raw=0)"));
        assert!(
            probe.contains("device.sysman.temperature_sensors.status: not_available (raw=null)")
        );
    }

    #[test]
    fn compact_human_snapshot_uses_typed_sysman_facts_and_reasons() {
        clear_delta_caches_for_tests();
        let device = DeviceRecord {
            driver_index: 0,
            device_index: 0,
            key: "driver=0,device=0,vendor=0x8086,device=0x4c8b".to_string(),
            facts: vec![
                ok_fact(
                    "device_name",
                    "zeDeviceGetProperties",
                    json!("Intel UHD 730"),
                ),
                ok_fact(
                    "device_key",
                    "wtg.intel.level_zero.device_key",
                    json!("driver=0,device=0,vendor=0x8086,device=0x4c8b"),
                ),
                ok_fact(
                    "uuid",
                    "zeDeviceGetProperties",
                    json!("240000002000000086808b4c04000000"),
                ),
                number_fact_u64("core_clock_mhz", "zeDeviceGetProperties", 1300, Some("mhz")),
                number_fact_u64(
                    "sysman.memory_modules.0.used_bytes",
                    "zesMemoryGetState",
                    9_160 * 1024 * 1024,
                    Some("bytes"),
                ),
                number_fact_u64(
                    "sysman.memory_modules.0.size_bytes",
                    "zesMemoryGetProperties",
                    16_384 * 1024 * 1024,
                    Some("bytes"),
                ),
                not_available_fact(
                    "sysman.power_domains.0.watts_delta",
                    "zesPowerGetEnergyCounter",
                    "requires previous sample.".to_string(),
                ),
                not_available_fact(
                    "sysman.engine_groups.0.utilization_pct_delta",
                    "zesEngineGetActivity",
                    "requires previous sample.".to_string(),
                ),
                number_fact_f64(
                    "sysman.frequency_domains.0.actual_mhz",
                    "zesFrequencyGetState",
                    1300.0,
                    Some("mhz"),
                ),
                number_fact_f64(
                    "sysman.frequency_domains.0.request_mhz",
                    "zesFrequencyGetState",
                    350.0,
                    Some("mhz"),
                ),
                number_fact_f64(
                    "sysman.frequency_domains.0.max_mhz",
                    "zesFrequencyGetProperties",
                    0.0,
                    Some("mhz"),
                ),
                not_available_fact(
                    "sysman.temperature_sensors.status",
                    "zesDeviceEnumTemperatureSensors",
                    "zesDeviceEnumTemperatureSensors returned zero handles.".to_string(),
                ),
            ],
            unavailable: vec!["temperature"],
        };
        let sample = ProviderSample {
            wtg_version: "0.3.0",
            provider: PROVIDER,
            provider_authority: PROVIDER_AUTHORITY,
            provider_source: SOURCE,
            telemetry_class: TELEMETRY_CLASS,
            status: "ok",
            sample_seq: 0,
            timestamp_unix_ms: 0,
            dll_name: None,
            dll_path: None,
            telemetry_exports_matched: 4,
            sysman_exports_matched: 16,
            optional_calls_attempted: 0,
            optional_calls_ok: 0,
            optional_calls_unsupported: 0,
            optional_calls_not_available: 0,
            optional_calls_error: 0,
            driver_record_count: 1,
            device_record_count: 1,
            sysman_facts: Vec::new(),
            devices: vec![device],
            errors: Vec::new(),
        };

        let rendered = format_snapshot(&sample);
        assert!(rendered.contains("Intel device 0: Intel UHD 730"));
        assert!(rendered.contains("Device key: driver=0,device=0,vendor=0x8086,device=0x4c8b"));
        assert!(rendered.contains("UUID: 240000002000000086808b4c04000000"));
        assert!(rendered.contains("Core clock: 1300 MHz"));
        assert!(rendered.contains("Memory: 9160 MiB / 16384 MiB"));
        assert!(rendered.contains("Power: unavailable, requires previous sample"));
        assert!(rendered.contains("Engine 0: unavailable, requires previous sample"));
        assert!(rendered.contains("Frequency: actual 1300 MHz, requested 350 MHz"));
        assert!(rendered.contains("Temperature: unavailable, zero handles"));
        assert!(!rendered.contains("source_api"));
        assert!(!rendered.contains("buffer_hex"));
        assert!(!rendered.contains("stype"));
        assert!(!rendered.contains("max_mhz"));
    }

    fn sysman_buffer_from_struct<T: Copy>(value: T) -> SysmanBuffer {
        assert!(size_of::<T>() <= size_of::<SysmanBuffer>());
        let mut buffer = SysmanBuffer {
            stype: 0,
            p_next: std::ptr::null_mut(),
            bytes: [0u8; SYSMAN_BUFFER_BYTES],
        };
        unsafe {
            std::ptr::copy_nonoverlapping(
                (&value as *const T).cast::<u8>(),
                (&mut buffer as *mut SysmanBuffer).cast::<u8>(),
                size_of::<T>(),
            );
        }
        buffer
    }

    fn find_fact<'a>(facts: &'a [IntelFact], metric_key: &str) -> &'a IntelFact {
        facts
            .iter()
            .find(|fact| fact.metric_key == metric_key)
            .unwrap_or_else(|| panic!("missing fact {metric_key}"))
    }

    #[test]
    fn typed_sysman_facts_are_provider_scoped_and_first_sample_deltas_are_not_available() {
        clear_delta_caches_for_tests();
        let mem_props = sysman_buffer_from_struct(ZesMemProperties {
            stype: 0,
            p_next: std::ptr::null_mut(),
            mem_type: 0,
            on_subdevice: 0,
            subdevice_id: 0,
            location: 0,
            physical_size: 1024,
            bus_width: 128,
            num_channels: 2,
        });
        let mem_state = sysman_buffer_from_struct(ZesMemState {
            stype: 0,
            p_next: std::ptr::null(),
            health: 1,
            free: 384,
            size: 1024,
        });
        let freq_props = sysman_buffer_from_struct(ZesFreqProperties {
            stype: 0,
            p_next: std::ptr::null_mut(),
            domain_type: 0,
            on_subdevice: 0,
            subdevice_id: 0,
            can_control: 1,
            is_throttle_event_supported: 0,
            min: 300.0,
            max: 1300.0,
        });
        let freq_state = sysman_buffer_from_struct(ZesFreqState {
            stype: 0,
            p_next: std::ptr::null(),
            current_voltage: 0.9,
            request: 1100.0,
            tdp: 1200.0,
            efficient: 600.0,
            actual: 1050.0,
            throttle_reasons: 0,
        });
        let power_state = sysman_buffer_from_struct(ZesPowerEnergyCounter {
            energy: 2_500_000,
            timestamp: 50_000,
        });
        let engine_state = sysman_buffer_from_struct(ZesEngineStats {
            active_time: 4_000,
            timestamp: 10_000,
        });

        let mut facts = Vec::new();
        extend_typed_sysman_facts(
            1,
            "driver=0,device=0,vendor=0x8086,device=0x4c8b",
            "memory_modules",
            0,
            "zesMemoryGetProperties",
            Some(&mem_props),
            "zesMemoryGetState",
            Some(&mem_state),
            &mut facts,
        );
        extend_typed_sysman_facts(
            1,
            "driver=0,device=0,vendor=0x8086,device=0x4c8b",
            "frequency_domains",
            0,
            "zesFrequencyGetProperties",
            Some(&freq_props),
            "zesFrequencyGetState",
            Some(&freq_state),
            &mut facts,
        );
        extend_typed_sysman_facts(
            1,
            "driver=0,device=0,vendor=0x8086,device=0x4c8b",
            "power_domains",
            0,
            "zesPowerGetProperties",
            None,
            "zesPowerGetEnergyCounter",
            Some(&power_state),
            &mut facts,
        );
        extend_typed_sysman_facts(
            1,
            "driver=0,device=0,vendor=0x8086,device=0x4c8b",
            "engine_groups",
            0,
            "zesEngineGetProperties",
            None,
            "zesEngineGetActivity",
            Some(&engine_state),
            &mut facts,
        );

        assert_eq!(
            find_fact(&facts, "sysman.memory_modules.0.size_bytes").raw,
            json!(1024)
        );
        assert_eq!(
            find_fact(&facts, "sysman.memory_modules.0.free_bytes").raw,
            json!(384)
        );
        assert_eq!(
            find_fact(&facts, "sysman.memory_modules.0.used_bytes").raw,
            json!(640)
        );
        assert_eq!(
            find_fact(&facts, "sysman.memory_modules.0.health").raw,
            json!("ok")
        );
        assert_eq!(
            find_fact(&facts, "sysman.frequency_domains.0.min_mhz").raw,
            json!(300.0)
        );
        assert_eq!(
            find_fact(&facts, "sysman.frequency_domains.0.max_mhz").raw,
            json!(1300.0)
        );
        assert_eq!(
            find_fact(&facts, "sysman.frequency_domains.0.request_mhz").raw,
            json!(1100.0)
        );
        assert_eq!(
            find_fact(&facts, "sysman.frequency_domains.0.actual_mhz").raw,
            json!(1050.0)
        );
        assert_eq!(
            find_fact(&facts, "sysman.power_domains.0.energy_j").raw,
            json!(2.5)
        );
        assert_eq!(
            find_fact(&facts, "sysman.power_domains.0.timestamp_ns").raw,
            json!(50_000_000u64)
        );
        assert_eq!(
            find_fact(&facts, "sysman.engine_groups.0.active_time_ns").raw,
            json!(4_000u64)
        );
        assert_eq!(
            find_fact(&facts, "sysman.engine_groups.0.timestamp_ns").raw,
            json!(10_000u64)
        );

        let watts_delta = find_fact(&facts, "sysman.power_domains.0.watts_delta");
        assert_eq!(watts_delta.state, "not_available");
        assert_eq!(
            watts_delta.error_message.as_deref(),
            Some("requires previous sample.")
        );
        let utilization_delta = find_fact(&facts, "sysman.engine_groups.0.utilization_pct_delta");
        assert_eq!(utilization_delta.state, "not_available");
        assert_eq!(
            utilization_delta.error_message.as_deref(),
            Some("requires previous sample.")
        );
    }

    #[test]
    fn watch_style_second_sample_produces_power_and_engine_deltas() {
        clear_delta_caches_for_tests();
        let device_key = "driver=0,device=0,vendor=0x8086,device=0x4c8b";
        let mut first_facts = Vec::new();
        let mut second_facts = Vec::new();

        extend_typed_sysman_facts(
            1,
            device_key,
            "power_domains",
            0,
            "zesPowerGetProperties",
            None,
            "zesPowerGetEnergyCounter",
            Some(&sysman_buffer_from_struct(ZesPowerEnergyCounter {
                energy: 1_000_000,
                timestamp: 10_000,
            })),
            &mut first_facts,
        );
        extend_typed_sysman_facts(
            2,
            device_key,
            "power_domains",
            0,
            "zesPowerGetProperties",
            None,
            "zesPowerGetEnergyCounter",
            Some(&sysman_buffer_from_struct(ZesPowerEnergyCounter {
                energy: 2_000_000,
                timestamp: 20_000,
            })),
            &mut second_facts,
        );
        extend_typed_sysman_facts(
            1,
            device_key,
            "engine_groups",
            0,
            "zesEngineGetProperties",
            None,
            "zesEngineGetActivity",
            Some(&sysman_buffer_from_struct(ZesEngineStats {
                active_time: 1_000,
                timestamp: 10_000,
            })),
            &mut first_facts,
        );
        extend_typed_sysman_facts(
            2,
            device_key,
            "engine_groups",
            0,
            "zesEngineGetProperties",
            None,
            "zesEngineGetActivity",
            Some(&sysman_buffer_from_struct(ZesEngineStats {
                active_time: 4_000,
                timestamp: 20_000,
            })),
            &mut second_facts,
        );

        assert_eq!(
            find_fact(&second_facts, "sysman.power_domains.0.watts_delta").raw,
            json!(100.0)
        );
        assert_eq!(
            find_fact(
                &second_facts,
                "sysman.engine_groups.0.utilization_pct_delta"
            )
            .raw,
            json!(30.0)
        );
    }

    #[test]
    fn power_delta_requires_positive_timestamp_delta() {
        clear_delta_caches_for_tests();
        let device_key = "driver=0,device=0,vendor=0x8086,device=0x4c8b";
        let mut first_facts = Vec::new();
        let mut second_facts = Vec::new();

        extend_typed_sysman_facts(
            1,
            device_key,
            "power_domains",
            0,
            "zesPowerGetProperties",
            None,
            "zesPowerGetEnergyCounter",
            Some(&sysman_buffer_from_struct(ZesPowerEnergyCounter {
                energy: 1_000_000,
                timestamp: 10_000,
            })),
            &mut first_facts,
        );
        extend_typed_sysman_facts(
            2,
            device_key,
            "power_domains",
            0,
            "zesPowerGetProperties",
            None,
            "zesPowerGetEnergyCounter",
            Some(&sysman_buffer_from_struct(ZesPowerEnergyCounter {
                energy: 2_000_000,
                timestamp: 10_000,
            })),
            &mut second_facts,
        );

        let watts_delta = find_fact(&second_facts, "sysman.power_domains.0.watts_delta");
        assert_eq!(watts_delta.state, "not_available");
        assert_eq!(
            watts_delta.error_message.as_deref(),
            Some("requires positive timestamp delta.")
        );
    }

    #[test]
    fn power_delta_keeps_same_counter_value_as_idle_zero() {
        clear_delta_caches_for_tests();
        let key = "driver=0,device=0,vendor=0x8086,device=0x4c8b::sysman.power_domains.0";

        let first = update_power_delta(
            key.to_string(),
            ZesPowerEnergyCounter {
                energy: 1_000_000,
                timestamp: 10_000,
            },
        );
        assert!(matches!(first, DeltaValue::NotAvailable(_)));

        let second = update_power_delta(
            key.to_string(),
            ZesPowerEnergyCounter {
                energy: 1_000_000,
                timestamp: 20_000,
            },
        );

        match second {
            DeltaValue::Ok(watts) => assert_eq!(watts, 0.0),
            DeltaValue::NotAvailable(reason) => panic!("expected idle zero, got {reason}"),
        }
    }

    #[test]
    fn power_delta_marks_counter_reset_as_not_available() {
        clear_delta_caches_for_tests();
        let device_key = "driver=0,device=0,vendor=0x8086,device=0x4c8b";
        let mut first_facts = Vec::new();
        let mut second_facts = Vec::new();

        extend_typed_sysman_facts(
            1,
            device_key,
            "power_domains",
            0,
            "zesPowerGetProperties",
            None,
            "zesPowerGetEnergyCounter",
            Some(&sysman_buffer_from_struct(ZesPowerEnergyCounter {
                energy: 900_000,
                timestamp: 10_000,
            })),
            &mut first_facts,
        );
        extend_typed_sysman_facts(
            2,
            device_key,
            "power_domains",
            0,
            "zesPowerGetProperties",
            None,
            "zesPowerGetEnergyCounter",
            Some(&sysman_buffer_from_struct(ZesPowerEnergyCounter {
                energy: 1_000,
                timestamp: 20_000,
            })),
            &mut second_facts,
        );

        let watts_delta = find_fact(&second_facts, "sysman.power_domains.0.watts_delta");
        assert_eq!(watts_delta.state, "not_available");
        assert_eq!(
            watts_delta.error_message.as_deref(),
            Some("counter reset detected.")
        );
        assert_eq!(watts_delta.raw, json!(null));
    }

    #[test]
    fn engine_delta_requires_positive_timestamp_delta() {
        clear_delta_caches_for_tests();
        let device_key = "driver=0,device=0,vendor=0x8086,device=0x4c8b";
        let mut first_facts = Vec::new();
        let mut second_facts = Vec::new();

        extend_typed_sysman_facts(
            1,
            device_key,
            "engine_groups",
            0,
            "zesEngineGetProperties",
            None,
            "zesEngineGetActivity",
            Some(&sysman_buffer_from_struct(ZesEngineStats {
                active_time: 1_000,
                timestamp: 10_000,
            })),
            &mut first_facts,
        );
        extend_typed_sysman_facts(
            2,
            device_key,
            "engine_groups",
            0,
            "zesEngineGetProperties",
            None,
            "zesEngineGetActivity",
            Some(&sysman_buffer_from_struct(ZesEngineStats {
                active_time: 4_000,
                timestamp: 10_000,
            })),
            &mut second_facts,
        );

        let utilization_delta = find_fact(
            &second_facts,
            "sysman.engine_groups.0.utilization_pct_delta",
        );
        assert_eq!(utilization_delta.state, "not_available");
        assert_eq!(
            utilization_delta.error_message.as_deref(),
            Some("requires positive timestamp delta.")
        );
    }

    #[test]
    fn engine_delta_keeps_same_counter_value_as_idle_zero() {
        clear_delta_caches_for_tests();
        let key = "driver=0,device=0,vendor=0x8086,device=0x4c8b::sysman.engine_groups.0";

        let first = update_engine_delta(
            key.to_string(),
            ZesEngineStats {
                active_time: 1_000,
                timestamp: 10_000,
            },
        );
        assert!(matches!(first, DeltaValue::NotAvailable(_)));

        let second = update_engine_delta(
            key.to_string(),
            ZesEngineStats {
                active_time: 1_000,
                timestamp: 20_000,
            },
        );

        match second {
            DeltaValue::Ok(utilization_pct) => assert_eq!(utilization_pct, 0.0),
            DeltaValue::NotAvailable(reason) => panic!("expected idle zero, got {reason}"),
        }
    }

    #[test]
    fn engine_delta_marks_counter_reset_as_not_available() {
        clear_delta_caches_for_tests();
        let device_key = "driver=0,device=0,vendor=0x8086,device=0x4c8b";
        let mut first_facts = Vec::new();
        let mut second_facts = Vec::new();

        extend_typed_sysman_facts(
            1,
            device_key,
            "engine_groups",
            0,
            "zesEngineGetProperties",
            None,
            "zesEngineGetActivity",
            Some(&sysman_buffer_from_struct(ZesEngineStats {
                active_time: 9_000,
                timestamp: 10_000,
            })),
            &mut first_facts,
        );
        extend_typed_sysman_facts(
            2,
            device_key,
            "engine_groups",
            0,
            "zesEngineGetProperties",
            None,
            "zesEngineGetActivity",
            Some(&sysman_buffer_from_struct(ZesEngineStats {
                active_time: 1_000,
                timestamp: 20_000,
            })),
            &mut second_facts,
        );

        let utilization_delta = find_fact(
            &second_facts,
            "sysman.engine_groups.0.utilization_pct_delta",
        );
        assert_eq!(utilization_delta.state, "not_available");
        assert_eq!(
            utilization_delta.error_message.as_deref(),
            Some("counter reset detected.")
        );
        assert_eq!(utilization_delta.raw, json!(null));
    }

    #[test]
    fn compact_watch_output_shows_delta_values() {
        let sample = ProviderSample {
            wtg_version: "0.3.0",
            provider: PROVIDER,
            provider_authority: PROVIDER_AUTHORITY,
            provider_source: SOURCE,
            telemetry_class: TELEMETRY_CLASS,
            status: "ok",
            sample_seq: 2,
            timestamp_unix_ms: 0,
            dll_name: None,
            dll_path: None,
            telemetry_exports_matched: 4,
            sysman_exports_matched: 16,
            optional_calls_attempted: 0,
            optional_calls_ok: 0,
            optional_calls_unsupported: 0,
            optional_calls_not_available: 0,
            optional_calls_error: 0,
            driver_record_count: 1,
            device_record_count: 1,
            sysman_facts: Vec::new(),
            devices: vec![DeviceRecord {
                driver_index: 0,
                device_index: 0,
                key: "driver=0,device=0,vendor=0x8086,device=0x4c8b".to_string(),
                facts: vec![
                    ok_fact(
                        "device_name",
                        "zeDeviceGetProperties",
                        json!("Intel UHD 730"),
                    ),
                    number_fact_u64(
                        "sysman.memory_modules.0.used_bytes",
                        "zesMemoryGetState",
                        9_160 * 1024 * 1024,
                        Some("bytes"),
                    ),
                    number_fact_u64(
                        "sysman.memory_modules.0.size_bytes",
                        "zesMemoryGetProperties",
                        16_384 * 1024 * 1024,
                        Some("bytes"),
                    ),
                    number_fact_f64(
                        "sysman.power_domains.0.watts_delta",
                        "zesPowerGetEnergyCounter",
                        12.2,
                        Some("watts"),
                    ),
                    number_fact_f64(
                        "sysman.engine_groups.0.utilization_pct_delta",
                        "zesEngineGetActivity",
                        0.04,
                        Some("pct"),
                    ),
                    number_fact_f64(
                        "sysman.engine_groups.1.utilization_pct_delta",
                        "zesEngineGetActivity",
                        0.00,
                        Some("pct"),
                    ),
                    number_fact_f64(
                        "sysman.engine_groups.2.utilization_pct_delta",
                        "zesEngineGetActivity",
                        0.00,
                        Some("pct"),
                    ),
                    number_fact_f64(
                        "sysman.frequency_domains.0.actual_mhz",
                        "zesFrequencyGetState",
                        1300.0,
                        Some("mhz"),
                    ),
                    number_fact_f64(
                        "sysman.frequency_domains.0.request_mhz",
                        "zesFrequencyGetState",
                        350.0,
                        Some("mhz"),
                    ),
                    not_available_fact(
                        "sysman.temperature_sensors.status",
                        "zesDeviceEnumTemperatureSensors",
                        "zesDeviceEnumTemperatureSensors returned zero handles.".to_string(),
                    ),
                ],
                unavailable: vec!["temperature"],
            }],
            errors: Vec::new(),
        };

        let rendered = format_watch_sample(&sample);
        assert!(rendered.contains("sample_seq: 2"));
        assert!(rendered.contains("Intel device 0: Intel UHD 730"));
        assert!(rendered.contains("Power: 12.2 W"));
        assert!(rendered.contains("Engine 0: 0.04%"));
        assert!(rendered.contains("Engine 1: 0.00%"));
        assert!(rendered.contains("Engine 2: 0.00%"));
        assert!(rendered.contains("Frequency: actual 1300 MHz, requested 350 MHz"));
    }
}
