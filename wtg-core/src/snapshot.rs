//! Immutable GPU metric snapshots — the source of truth
//!
//! Snapshots capture an instantaneous GPU state without smoothing,
//! allowing UI layers to consume and render independently.

use serde::{Deserialize, Serialize};
use std::time::Instant;

/// GPU-level statistics
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GpuStats {
    pub gpu_index: usize,
    pub gpu_name: String,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
    pub power_draw_w: f32,
    pub sm_clock_mhz: u32,
    pub mem_clock_mhz: u32,
}

/// Per-process GPU statistics
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProcessStats {
    pub pid: u32,
    pub process_name: String,
    pub gpu_index: usize,
    pub sm_utilization_pct: f32,
    pub memory_used_mb: u64,
}

/// Immutable snapshot of GPU state at a point in time
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub timestamp: u64, // Unix timestamp in milliseconds
    pub gpus: Vec<GpuStats>,
    pub processes: Vec<ProcessStats>,
}

impl Snapshot {
    /// Create a new snapshot
    pub fn new(gpus: Vec<GpuStats>, processes: Vec<ProcessStats>) -> Self {
        Self {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            gpus,
            processes,
        }
    }
}
