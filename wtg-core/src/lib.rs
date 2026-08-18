// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Adam Hooper
//! WTG Core - Backend truth layer for GPU metrics via NVML.
//!
//! This crate provides:
//! - NVML access + snapshot types used by the app
//! - `nvml::GpuSnapshot` is the authoritative snapshot model for `--stats`

pub mod exit_code;
pub mod nvml;

pub use exit_code::exit_code_for_status;
