//! WTG App — TUI for v0.1 GPU metric validation
//!
//! This is the entry point for the WTG proof-of-concept.
//! Phase 1: TUI-based validation of NVML metrics.

use std::env;
use tracing::info;

fn main() {
    // Initialize logging
    tracing_subscriber::fmt::init();

    info!("WTG v0.1 initializing...");

    // Minimal arg parsing (no deps)
    let args: Vec<String> = env::args().collect();
    let once = args.iter().any(|a| a == "--once");

    if once {
        match wtg_core::nvml::snapshot_all() {
            Ok(snaps) => {
                println!("WTG — WhatTheGPU v0.1");
                println!("Honest GPU compute stats for Windows");
                println!("\nWTG snapshot (NVML)\n");

                for s in snaps {
                    println!("{s}");
                }
            }
            Err(e) => {
                eprintln!("WTG --once failed: {e}");
                std::process::exit(2);
            }
        }
        return;
    }

    // TODO: Initialize backend (wtg-core)
    // TODO: Implement TUI (ratatui) for snapshot visualization
    // TODO: Metrics validation loop

    println!("WTG — WhatTheGPU v0.1");
    println!("Honest GPU compute stats for Windows");
    println!("\nTUI initialization in progress...");
}

