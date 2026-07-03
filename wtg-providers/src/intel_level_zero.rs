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
const STATS_SCHEMA: &str = "wtg.intel_level_zero.stats.v1";
const ZE_RESULT_SUCCESS: i32 = 0;
const ZE_MAX_DEVICE_NAME: usize = 256;

type ZeInit = unsafe extern "C" fn(u32) -> i32;
type ZeDriverGet = unsafe extern "C" fn(*mut u32, *mut *mut c_void) -> i32;
type ZeDeviceGet = unsafe extern "C" fn(*mut c_void, *mut u32, *mut *mut c_void) -> i32;
type ZeDeviceGetProperties = unsafe extern "C" fn(*mut c_void, *mut ZeDeviceProperties) -> i32;

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
    optional_calls_attempted: usize,
    optional_calls_ok: usize,
    optional_calls_unsupported: usize,
    optional_calls_not_available: usize,
    optional_calls_error: usize,
    driver_record_count: usize,
    device_record_count: usize,
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
    metric_key: &'static str,
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
}

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
            if init_result != ZE_RESULT_SUCCESS {
                return unavailable_sample(
                    sample_seq,
                    Some(library.dll_name.clone()),
                    Some(library.dll_path.clone()),
                    library.telemetry_exports_matched(),
                    format!("zeInit failed with status {init_result}."),
                );
            }

            match enumerate_devices(&library) {
                Ok(enumeration) => enumeration.into_sample(sample_seq, library),
                Err(reason) => unavailable_sample(
                    sample_seq,
                    Some(library.dll_name.clone()),
                    Some(library.dll_path.clone()),
                    library.telemetry_exports_matched(),
                    reason,
                ),
            }
        }
        Err(reason) => unavailable_sample(sample_seq, None, None, 0, reason),
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
    fn into_sample(self, sample_seq: u64, library: LevelZeroLibrary) -> ProviderSample {
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
            optional_calls_attempted: self.optional_calls_attempted,
            optional_calls_ok: self.optional_calls_ok,
            optional_calls_unsupported: self.optional_calls_unsupported,
            optional_calls_not_available: self.optional_calls_not_available,
            optional_calls_error: self.optional_calls_error,
            driver_record_count: self.driver_record_count,
            device_record_count: self.device_record_count,
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
        optional_calls_attempted: 0,
        optional_calls_ok: 0,
        optional_calls_unsupported: 0,
        optional_calls_not_available: 0,
        optional_calls_error: 0,
        driver_record_count: 0,
        device_record_count: 0,
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

        Ok(Self {
            module,
            dll_name: dll_name.to_string(),
            dll_path,
            ze_init,
            ze_driver_get,
            ze_device_get,
            ze_device_get_properties,
        })
    }

    fn telemetry_exports_matched(&self) -> usize {
        4
    }
}

fn enumerate_devices(library: &LevelZeroLibrary) -> Result<Enumeration, String> {
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
                driver_index,
                device_index,
                property_result,
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
    driver_index: usize,
    device_index: usize,
    property_result: Result<(ZeDeviceProperties, i32), String>,
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

            facts.push(ok_fact("device_name", "zeDeviceGetProperties", json!(name)));
            facts.push(ok_fact(
                "device_key",
                "wtg.intel.level_zero.device_key",
                json!(key.clone()),
            ));
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
                metric_key: "core_clock_mhz",
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

            unavailable.extend(["activity", "memory", "power", "temperature"]);

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
                metric_key: "device_key",
                source_api: "wtg.intel.level_zero.device_key",
                state: "ok",
                raw: json!(key.clone()),
                unit: None,
                error_message: None,
            });
            facts.push(IntelFact {
                metric_key: "device_name",
                source_api: "zeDeviceGetProperties",
                state: "error",
                raw: Value::Null,
                unit: None,
                error_message: Some(format!(
                    "zeDeviceGetProperties failed for driver {driver_index} device {device_index} with status {result}."
                )),
            });
            unavailable.extend(["name", "type", "activity", "memory", "power", "temperature"]);
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
                metric_key: "device_key",
                source_api: "wtg.intel.level_zero.device_key",
                state: "ok",
                raw: json!(key.clone()),
                unit: None,
                error_message: None,
            });
            facts.push(IntelFact {
                metric_key: "device_name",
                source_api: "zeDeviceGetProperties",
                state: "error",
                raw: Value::Null,
                unit: None,
                error_message: Some(error_message),
            });
            unavailable.extend(["name", "type", "activity", "memory", "power", "temperature"]);
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

fn push_snapshot_device_lines(lines: &mut Vec<String>, device: &DeviceRecord) {
    lines.push(format!(
        "Intel device {}: {}",
        device.device_index,
        fact_string(device, "device_name").unwrap_or("unknown")
    ));
    lines.push(format!("  Device key: {}", device.key));
    if let Some(device_type) = fact_string(device, "device_type") {
        lines.push(format!("  Device type: {device_type}"));
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
    lines.push(format!(
        "{} [{}]",
        fact_string(device, "device_name").unwrap_or("Intel device"),
        device.key
    ));
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
    lines.push(format!(
        "device.name: {}",
        fact_string(device, "device_name").unwrap_or("unknown")
    ));
    lines.push(format!("device.key: {}", device.key));
    if let Some(device_type) = fact_string(device, "device_type") {
        lines.push(format!("device.type: {device_type}"));
    }
    if let Some(uuid) = fact_string(device, "uuid") {
        lines.push(format!("device.uuid: {uuid}"));
    }
    if let Some(core_clock_mhz) = fact_number(device, "core_clock_mhz") {
        lines.push(format!("device.core_clock_mhz: {core_clock_mhz:.1}"));
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
        object.insert(fact.metric_key.to_string(), Value::Object(entry));
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

fn ok_fact(metric_key: &'static str, source_api: &'static str, raw: Value) -> IntelFact {
    IntelFact {
        metric_key,
        source_api,
        state: "ok",
        raw,
        unit: None,
        error_message: None,
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
        let label = String::from_utf8_lossy(&name[..name.len().saturating_sub(1)]).to_string();
        return Err(format!("missing Level Zero symbol {label}."));
    }

    Ok(std::mem::transmute_copy(&symbol))
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
    use super::{PROVIDER, PROVIDER_AUTHORITY, SOURCE, STATS_SCHEMA, TELEMETRY_CLASS};

    #[test]
    fn provider_constants_match_contract() {
        assert_eq!(PROVIDER, "intel");
        assert_eq!(PROVIDER_AUTHORITY, "Intel Level Zero");
        assert_eq!(SOURCE, "wtg.provider.intel.level_zero");
        assert_eq!(TELEMETRY_CLASS, "provider_telemetry");
        assert_eq!(STATS_SCHEMA, "wtg.intel_level_zero.stats.v1");
    }
}
