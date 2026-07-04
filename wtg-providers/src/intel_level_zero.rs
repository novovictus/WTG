use std::ffi::{c_char, c_void, CStr, OsStr};
use std::iter;
use std::mem::MaybeUninit;
use std::os::windows::ffi::OsStrExt;
use std::ptr::{self, NonNull};
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

            match enumerate_devices(&library, sysman_probe.zes_init_ok) {
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

pub fn format_snapshot(sample: &ProviderSample) -> String {
    let mut lines = Vec::new();
    if sample.status != "ok" {
        lines.push(format!("Provider status: {}", sample.status));
        if let Some(reason) = sample.errors.first() {
            lines.push(format!("Reason: {reason}"));
        }
        return lines.join("\n");
    }

    lines.push(format!(
        "Intel driver records returned: {}",
        sample.driver_record_count
    ));
    lines.push(format!(
        "Intel device records returned: {}",
        sample.device_record_count
    ));
    lines.push(String::new());

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

    lines.push(format!(
        "Intel drivers: {} | devices: {}",
        sample.driver_record_count, sample.device_record_count
    ));
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

            extend_sysman_device_facts(library, device, &mut facts, &mut unavailable, sysman_ready);

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
            extend_sysman_device_facts(library, device, &mut facts, &mut unavailable, sysman_ready);
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
            extend_sysman_device_facts(library, device, &mut facts, &mut unavailable, sysman_ready);
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
            facts.push(ok_fact(
                format!("sysman.{}.{}.handle", group.domain_key, handle_index),
                group.enum_source_api,
                json!(handle_pointer_json(handle_index, handle)),
            ));

            match group.get_properties {
                Some(get_properties) => match query_sysman_buffer(get_properties, handle) {
                    Ok(buffer) => {
                        saw_success = true;
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

fn push_snapshot_device_lines(lines: &mut Vec<String>, device: &DeviceRecord) {
    if let Some(device_name) = fact_string(device, "device_name") {
        lines.push(format!(
            "Intel device {}: {device_name}",
            device.device_index
        ));
        lines.push(format!("  Device key: {}", device.key));
    } else {
        lines.push(format!(
            "Intel device {} [{}]",
            device.device_index, device.key
        ));
    }
    if let Some(device_type) = fact_string(device, "device_type") {
        lines.push(format!("  Device type: {device_type}"));
    }
    if let Some(vendor_id) = fact_u64(device, "vendor_id") {
        lines.push(format!("  Vendor ID: 0x{vendor_id:04x} ({vendor_id})"));
    }
    if let Some(device_id) = fact_u64(device, "device_id") {
        lines.push(format!("  Device ID: 0x{device_id:04x} ({device_id})"));
    }
    if let Some(uuid) = fact_string(device, "uuid") {
        lines.push(format!("  UUID: {uuid}"));
    }
    if let Some(core_clock_mhz) = fact_number(device, "core_clock_mhz") {
        lines.push(format!("  Core clock: {core_clock_mhz:.1} MHz"));
    }
    if !device.unavailable.is_empty() {
        lines.push(format!("  Unavailable: {}", device.unavailable.join(", ")));
    }
}

fn push_watch_device_lines(lines: &mut Vec<String>, device: &DeviceRecord) {
    if let Some(device_name) = fact_string(device, "device_name") {
        lines.push(format!("{device_name} [{}]", device.key));
    } else {
        lines.push(format!(
            "Intel device {} [{}]",
            device.device_index, device.key
        ));
    }
    if let Some(device_type) = fact_string(device, "device_type") {
        lines.push(format!("  device type: {device_type}"));
    }
    if let Some(core_clock_mhz) = fact_number(device, "core_clock_mhz") {
        lines.push(format!("  core clock: {core_clock_mhz:.1} MHz"));
    }
    if !device.unavailable.is_empty() {
        lines.push(format!("  unavailable: {}", device.unavailable.join(", ")));
    }
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
    use std::mem::{ManuallyDrop, MaybeUninit};
    use std::ptr::NonNull;

    use serde_json::json;

    use super::{
        build_device_record, format_probe_snapshot, format_snapshot, format_stats_snapshot_json,
        not_available_fact, ok_fact, sample_status, DeviceRecord, IntelFact, LevelZeroLibrary,
        ProviderSample, SysmanBuffer, ZeDeviceProperties, PROVIDER, PROVIDER_AUTHORITY, SOURCE,
        STATS_SCHEMA, TELEMETRY_CLASS, ZE_RESULT_SUCCESS,
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
        assert!(rendered.contains("Vendor ID: 0x8086 (32902)"));
        assert!(rendered.contains("Device ID: 0x4c8b (19595)"));
        assert!(
            rendered.contains("Unavailable: name, activity, memory, power, temperature, frequency")
        );
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
}
