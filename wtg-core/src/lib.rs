//! WTG Core - Backend truth layer for GPU metrics via NVML
//!
//! This crate provides:
//! - NVML access + snapshot types used by the app
//! - `nvml::GpuSnapshot` is the authoritative snapshot model for `--stats`

pub mod nvml;
