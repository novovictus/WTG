//! WTG View - Shared view helpers for UI layers
//!
//! This crate provides:
//! - Sorting and filtering helpers
//! - Column formatting
//! - View transformations
//! - Reusable components for TUI and egui

use wtg_core::nvml::GpuSnapshot;

/// Format a snapshot for display
pub fn format_snapshot(_snapshot: &GpuSnapshot) -> String {
    // TODO: Implement formatting helpers
    "TODO: format_snapshot".to_string()
}
