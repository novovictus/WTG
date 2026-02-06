//! WTG Core - Backend truth layer for GPU metrics via NVML
//!
//! This crate provides:
//! - NVML integration
//! - Metric providers (SM utilization, VRAM, power, clocks)
//! - Refresh loop with fixed timestep
//! - Immutable snapshot structures
//! - Per-process GPU metric attribution

pub mod nvml;
pub mod snapshot;

// TODO: Implement NVML bindings and bootstrap
