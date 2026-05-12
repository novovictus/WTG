// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Adam Hooper

//! WTG View - Reserved shared view helpers for future UI layers.
//!
//! This crate is retained as a workspace placeholder for shared formatting,
//! sorting, filtering, and view-model helpers. The current beta keeps active
//! UI logic in `wtg-app`.

use wtg_core::nvml::GpuSnapshot;

/// Reserved formatting hook for future shared UI code.
#[doc(hidden)]
pub fn format_snapshot(_snapshot: &GpuSnapshot) -> String {
    String::new()
}
