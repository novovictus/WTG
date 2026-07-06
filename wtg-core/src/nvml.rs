// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Adam Hooper
pub mod field_values;
pub mod probe_context;
pub mod provenance;

use nvml_wrapper::{enum_wrappers::device::TemperatureSensor, Nvml};
use std::fmt;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct GpuSnapshot {
    pub index: u32,
    pub name: String,
    pub uuid: String,
    pub mem_used_bytes: u64,
    pub mem_total_bytes: u64,
    pub gpu_util_pct: u32,
    pub mem_util_pct: u32,
    pub temp_c: Option<u32>,
    pub power_mw: Option<u32>,
    pub power_limit_mw: Option<u32>,
}

pub struct NvmlContext {
    pub nvml: Nvml,
    pub device_indices: Vec<u32>,
}

#[derive(Debug)]
pub struct GpuSampleResult {
    pub index: u32,
    pub result: Result<GpuSnapshot, String>,
}

#[derive(Debug)]
pub struct NvmlSnapshotReport {
    pub status: &'static str,
    pub reason: Option<String>,
    pub device_results: Vec<GpuSampleResult>,
}

impl NvmlSnapshotReport {
    pub fn successful_snapshots(&self) -> Vec<GpuSnapshot> {
        self.device_results
            .iter()
            .filter_map(|sample| sample.result.as_ref().ok().cloned())
            .collect()
    }
}

impl fmt::Display for GpuSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let used_mib = self.mem_used_bytes / (1024 * 1024);
        let total_mib = self.mem_total_bytes / (1024 * 1024);

        writeln!(f, "NVML device {}: {}", self.index, self.name)?;
        writeln!(f, "  UUID: {}", self.uuid)?;
        match self.temp_c {
            Some(t) => writeln!(f, "  Temp: {} C", t)?,
            None => writeln!(f, "  Temp: N/A")?,
        }
        writeln!(
            f,
            "  Util: {}% (gpu) {}% (mem)",
            self.gpu_util_pct, self.mem_util_pct
        )?;
        writeln!(f, "  VRAM: {} MiB / {} MiB", used_mib, total_mib)?;

        if let (Some(p), Some(plim)) = (self.power_mw, self.power_limit_mw) {
            writeln!(
                f,
                "  Power: {:.1} W / {:.1} W",
                p as f32 / 1000.0,
                plim as f32 / 1000.0
            )?;
        } else if let Some(p) = self.power_mw {
            writeln!(f, "  Power: {:.1} W", p as f32 / 1000.0)?;
        } else {
            writeln!(f, "  Power: N/A")?;
        }

        Ok(())
    }
}

/// One-shot NVML snapshot for all visible GPUs.
pub fn snapshot_all() -> Result<Vec<GpuSnapshot>, String> {
    let report = snapshot_report_once_unbounded();
    match report.status {
        "ok" => Ok(report.successful_snapshots()),
        _ => Err(report
            .reason
            .unwrap_or_else(|| "NVIDIA NVML snapshot failed.".to_string())),
    }
}

pub fn init_context() -> Result<NvmlContext, String> {
    let nvml = Nvml::init().map_err(|e| format!("NVML init failed: {e}"))?;
    let count = nvml
        .device_count()
        .map_err(|e| format!("NVML device_count failed: {e}"))?;
    let device_indices = (0..count).collect();
    Ok(NvmlContext {
        nvml,
        device_indices,
    })
}

pub fn snapshot_all_with_ctx(ctx: &NvmlContext) -> Result<Vec<GpuSnapshot>, String> {
    if ctx.device_indices.is_empty() {
        return Err("NVIDIA NVML returned zero devices.".to_string());
    }

    let device_results = collect_device_results(&ctx.nvml, ctx.device_indices.iter().copied());
    let successful = device_results
        .into_iter()
        .filter_map(|sample| sample.result.ok())
        .collect::<Vec<_>>();

    if successful.is_empty() {
        Err("all NVIDIA device samples failed".to_string())
    } else {
        Ok(successful)
    }
}

fn snapshot_report_once_unbounded() -> NvmlSnapshotReport {
    let nvml = match Nvml::init() {
        Ok(nvml) => nvml,
        Err(e) => {
            return NvmlSnapshotReport {
                status: "unavailable",
                reason: Some(format!("NVML init failed: {e}")),
                device_results: Vec::new(),
            };
        }
    };

    let count = match nvml.device_count() {
        Ok(count) => count,
        Err(e) => {
            return NvmlSnapshotReport {
                status: "unavailable",
                reason: Some(format!("NVML device_count failed: {e}")),
                device_results: Vec::new(),
            };
        }
    };

    if count == 0 {
        return NvmlSnapshotReport {
            status: "unavailable",
            reason: Some("NVIDIA NVML returned zero devices.".to_string()),
            device_results: Vec::new(),
        };
    }

    let device_results = collect_device_results(&nvml, 0..count);
    let successful_count = device_results
        .iter()
        .filter(|sample| sample.result.is_ok())
        .count();

    let (status, reason) = if successful_count == 0 {
        (
            "error",
            Some("all NVIDIA device samples failed".to_string()),
        )
    } else {
        ("ok", None)
    };

    NvmlSnapshotReport {
        status,
        reason,
        device_results,
    }
}

/// One-shot NVML snapshot for CLI snapshot paths only.
///
/// This intentionally spawns a worker thread around the full NVML snapshot sequence so
/// `--once` can return even if a vendor FFI call wedges. If that happens, the worker thread may
/// remain blocked until process exit. Do not call this once per tick from watch/stats loops.
pub fn snapshot_report_bounded_once(timeout: Duration) -> NvmlSnapshotReport {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(snapshot_report_once_unbounded());
    });

    match rx.recv_timeout(timeout) {
        Ok(report) => report,
        Err(mpsc::RecvTimeoutError::Timeout) => NvmlSnapshotReport {
            status: "unavailable",
            reason: Some(format!(
                "NVIDIA NVML snapshot did not return within {}ms (possible driver/service hang)",
                timeout.as_millis()
            )),
            device_results: Vec::new(),
        },
        Err(mpsc::RecvTimeoutError::Disconnected) => NvmlSnapshotReport {
            status: "error",
            reason: Some("NVIDIA NVML snapshot worker disconnected.".to_string()),
            device_results: Vec::new(),
        },
    }
}

fn collect_device_results<I>(nvml: &Nvml, indices: I) -> Vec<GpuSampleResult>
where
    I: IntoIterator<Item = u32>,
{
    indices
        .into_iter()
        .map(|i| GpuSampleResult {
            index: i,
            result: collect_device_snapshot(nvml, i),
        })
        .collect()
}

fn collect_device_snapshot(nvml: &Nvml, i: u32) -> Result<GpuSnapshot, String> {
    let dev = nvml
        .device_by_index(i)
        .map_err(|e| format!("device_by_index({i}) failed: {e}"))?;

    let name = dev.name().unwrap_or_else(|_| "<unknown>".to_string());
    let uuid = dev.uuid().unwrap_or_else(|_| "<unknown>".to_string());

    let mem = dev
        .memory_info()
        .map_err(|e| format!("memory_info({i}) failed: {e}"))?;
    let util = dev
        .utilization_rates()
        .map_err(|e| format!("utilization_rates({i}) failed: {e}"))?;
    let temp_c = dev.temperature(TemperatureSensor::Gpu).ok();

    // Power calls can fail on some laptops / policies; treat as optional.
    let power_mw = dev.power_usage().ok();
    let power_limit_mw = dev.enforced_power_limit().ok();

    Ok(GpuSnapshot {
        index: i,
        name,
        uuid,
        mem_used_bytes: mem.used,
        mem_total_bytes: mem.total,
        gpu_util_pct: util.gpu,
        mem_util_pct: util.memory,
        temp_c,
        power_mw,
        power_limit_mw,
    })
}
