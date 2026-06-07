use std::ffi::{c_char, c_int, c_void, CStr, OsStr};
use std::iter;
use std::mem::{size_of, MaybeUninit};
use std::os::windows::ffi::OsStrExt;
use std::ptr::{self, NonNull};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;

const SCHEMA: &str = "wtg.provider.amd.adl.sample.v1";
const SOURCE: &str = "wtg.provider.amd.adl";
const TELEMETRY_CLASS: &str = "provider_telemetry";
const PROVIDER: &str = "amd.adl";
const PROVIDER_AUTHORITY: &str = "AMD ADL";
const DEFAULT_INTERVAL_MS: u64 = 1000;

const ADL_OK: i32 = 0;
const ADL_MAX_PATH: usize = 256;

type AdlMainMemoryAlloc = unsafe extern "C" fn(c_int) -> *mut c_void;
type AdlMainControlCreate = unsafe extern "C" fn(AdlMainMemoryAlloc, c_int) -> c_int;
type AdlMainControlDestroy = unsafe extern "C" fn() -> c_int;
type AdlAdapterNumberOfAdaptersGet = unsafe extern "C" fn(*mut c_int) -> c_int;
type AdlAdapterAdapterInfoGet = unsafe extern "C" fn(*mut AdapterInfo, c_int) -> c_int;
type AdlAdapterActiveGet = unsafe extern "C" fn(c_int, *mut c_int) -> c_int;

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
struct ProviderSample {
    schema: &'static str,
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
    adapters: Vec<AdlAdapterRecord>,
    errors: Vec<String>,
}

#[derive(Debug, Serialize)]
struct AdlAdapterRecord {
    adl_raw_identity: AdlRawIdentity,
    adl_metrics: AdlMetrics,
    adl_return_codes: AdapterReturnCodes,
    provider_warnings: Vec<String>,
    provider_errors: Vec<String>,
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
struct AdlMetrics {
    adl_adapter_active: AdlMetricBoolCall,
}

#[derive(Debug, Serialize)]
struct AdlMetricBoolCall {
    attempted: bool,
    state: &'static str,
    adl_return_code: Option<i32>,
    value: Option<bool>,
}

#[derive(Debug, Serialize)]
struct SampleReturnCodes {
    adl_main_control_create: Option<i32>,
    adl_adapter_number_of_adapters_get: Option<i32>,
    adl_adapter_adapter_info_get: Option<i32>,
}

#[derive(Debug, Serialize)]
struct AdapterReturnCodes {
    adl_adapter_active_get: Option<i32>,
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

struct AdlLibrary {
    module: NonNull<c_void>,
    dll_name: String,
    dll_path: String,
    main_control_create: AdlMainControlCreate,
    main_control_destroy: AdlMainControlDestroy,
    adapter_number_of_adapters_get: AdlAdapterNumberOfAdaptersGet,
    adapter_info_get: AdlAdapterAdapterInfoGet,
    adapter_active_get: Option<AdlAdapterActiveGet>,
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
    let sample = collect_sample(sample_seq);
    println!(
        "{}",
        serde_json::to_string(&sample).expect("provider JSON serialization should succeed")
    );
}

fn collect_sample(sample_seq: u64) -> ProviderSample {
    match AdlLibrary::load() {
        Ok(library) => match AdlSession::create(&library) {
            Ok(_session) => match enumerate_adapters(&library) {
                Ok(enumeration) if enumeration.adapters.is_empty() => ProviderSample {
                    schema: SCHEMA,
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
                        adl_adapter_adapter_info_get: Some(
                            enumeration.adapter_adapter_info_get,
                        ),
                    },
                    adapters: enumeration.adapters,
                    errors: vec!["ADL initialized but returned zero adapters.".to_string()],
                },
                Ok(enumeration) => ProviderSample {
                    schema: SCHEMA,
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
                        adl_adapter_adapter_info_get: Some(
                            enumeration.adapter_adapter_info_get,
                        ),
                    },
                    adapters: enumeration.adapters,
                    errors: Vec::new(),
                },
                Err(err) => ProviderSample {
                    schema: SCHEMA,
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
                    adapters: Vec::new(),
                    errors: vec![err.message],
                },
            },
            Err(err) => ProviderSample {
                schema: SCHEMA,
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
                adapters: Vec::new(),
                errors: vec![err.message],
            },
        },
        Err(err) => ProviderSample {
            schema: SCHEMA,
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
            adapters: Vec::new(),
            errors: vec![err],
        },
    }
}

impl AdlLibrary {
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
        load_symbol::<AdlAdapterNumberOfAdaptersGet>(
            module,
            b"ADL_Adapter_NumberOfAdapters_Get\0",
        )?
    };
    let adapter_info_get = unsafe {
        load_symbol::<AdlAdapterAdapterInfoGet>(module, b"ADL_Adapter_AdapterInfo_Get\0")?
    };
    let adapter_active_get =
        unsafe { load_optional_symbol::<AdlAdapterActiveGet>(module, b"ADL_Adapter_Active_Get\0") };

    Ok(AdlLibrary {
        module,
        dll_name: dll_name.to_string(),
        dll_path,
        main_control_create,
        main_control_destroy,
        adapter_number_of_adapters_get,
        adapter_info_get,
        adapter_active_get,
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
    adapters: Vec<AdlAdapterRecord>,
}

fn enumerate_adapters(library: &AdlLibrary) -> Result<EnumerationResult, EnumerationError> {
    let mut adapter_count = 0i32;
    let count_result = unsafe { (library.adapter_number_of_adapters_get)(&mut adapter_count) };
    if count_result != ADL_OK {
        return Err(EnumerationError {
            adl_adapter_number_of_adapters_get: Some(count_result),
            adl_adapter_adapter_info_get: None,
            message: format!(
                "ADL_Adapter_NumberOfAdapters_Get failed with status {count_result}."
            ),
        });
    }

    if adapter_count <= 0 {
        return Ok(EnumerationResult {
            adapter_number_of_adapters_get: count_result,
            adapter_adapter_info_get: ADL_OK,
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
            message: format!(
                "ADL_Adapter_AdapterInfo_Get failed with status {info_result}."
            ),
        });
    }

    let adapters = adapter_info
        .into_iter()
        .map(|info| {
            let mut provider_warnings = Vec::new();
            if info.i_vendor_id != 1002 {
                provider_warnings.push(
                    "ADL returned unexpected adapter identity; record preserved without cross-provider validation."
                        .to_string(),
                );
            }

            let (adl_adapter_active, active_return_code, active_errors) =
                query_adapter_active(library, info.i_adapter_index);

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
                adl_metrics: AdlMetrics { adl_adapter_active },
                adl_return_codes: AdapterReturnCodes {
                    adl_adapter_active_get: active_return_code,
                },
                provider_warnings,
                provider_errors: active_errors,
            }
        })
        .collect::<Vec<_>>();

    Ok(EnumerationResult {
        adapter_number_of_adapters_get: count_result,
        adapter_adapter_info_get: info_result,
        adapters,
    })
}

fn zeroed_adapter_info() -> AdapterInfo {
    let mut info = unsafe { MaybeUninit::<AdapterInfo>::zeroed().assume_init() };
    info.i_size = size_of::<AdapterInfo>() as i32;
    info
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

fn query_adapter_active(
    library: &AdlLibrary,
    adapter_index: i32,
) -> (AdlMetricBoolCall, Option<i32>, Vec<String>) {
    let Some(active_get) = library.adapter_active_get else {
        return (
            AdlMetricBoolCall {
                attempted: false,
                state: "not_attempted",
                adl_return_code: None,
                value: None,
            },
            None,
            vec!["ADL_Adapter_Active_Get symbol unavailable in loaded ADL DLL.".to_string()],
        );
    };

    let mut active = 0i32;
    let result = unsafe { active_get(adapter_index, &mut active) };
    if result != ADL_OK {
        return (
            AdlMetricBoolCall {
                attempted: true,
                state: "error",
                adl_return_code: Some(result),
                value: None,
            },
            Some(result),
            vec![format!(
                "ADL_Adapter_Active_Get failed for adapter index {adapter_index} with status {result}."
            )],
        );
    }

    (
        AdlMetricBoolCall {
            attempted: true,
            state: "ok",
            adl_return_code: Some(result),
            value: Some(active != 0),
        },
        Some(result),
        Vec::new(),
    )
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
    use super::{parse_args, CliArgs, Mode, DEFAULT_INTERVAL_MS, PROVIDER, SCHEMA, SOURCE, TELEMETRY_CLASS};

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
        assert_eq!(SCHEMA, "wtg.provider.amd.adl.sample.v1");
        assert_eq!(SOURCE, "wtg.provider.amd.adl");
        assert_eq!(TELEMETRY_CLASS, "provider_telemetry");
        assert_eq!(PROVIDER, "amd.adl");
    }
}
