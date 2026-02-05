//! WTG App — TUI for v0.1 GPU metric validation
//!
//! This is the entry point for the WTG proof-of-concept.
//! Phase 1: TUI-based validation of NVML metrics.

use tracing::info;

fn main() {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    info!("WTG v0.1 initializing...");
    
    // TODO: Initialize backend (wtg-core)
    // TODO: Implement TUI (ratatui) for snapshot visualization
    // TODO: Metrics validation loop
    
    println!("WTG — WhatTheGPU v0.1");
    println!("Honest GPU compute stats for Windows");
    println!("\nTUI initialization in progress...");
}
