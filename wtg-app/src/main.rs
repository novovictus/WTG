//! WTG App — TUI for GPU metric validation
//!
//! Entry point for the WTG proof-of-concept.
//!
//! Current modes (intentionally minimal, no clap):
//!   --once                     : Take one NVML snapshot, print, exit.
//!   --watch                    : Take repeated NVML snapshots, print each tick.
//!   --watch --interval <ms>    : Same, but set period in milliseconds (default 1000ms).
//!
//! Design intent:
//! - Keep "mode" flags separate from "parameter" flags.
//! - `--interval` only matters when `--watch` is present.
//! - `--interval` without a value is a hard error (avoids ambiguity).
//! - This is a proof path: ground-truth telemetry first; UI later.

use std::env;
use std::process;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tracing::info;

/// Default sampling interval when `--watch` is enabled.
/// 1000ms is conservative and matches NVML’s practical update cadence for many metrics.
const DEFAULT_INTERVAL_MS: u64 = 1000;

/// Returns a simple timestamp like "1707101234.567" (unix seconds.millis).
/// No external deps; good enough for proof and log correlation.
fn now_ts() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => format!("{}.{:03}", d.as_secs(), d.subsec_millis()),
        Err(_) => "0.000".to_string(),
    }
}

/// Minimal argument parser for our small surface area.
///
/// Semantics:
/// - `--once` and `--watch` are mutually exclusive (error if both).
/// - `--interval <ms>`:
///     - requires a value
///     - parsed as u64 milliseconds
///     - only used with `--watch`
/// - Unknown flags are ignored for now (keeps dev friction low during bootstrap).
fn parse_args() -> (bool /*once*/, bool /*watch*/, Option<u64> /*interval_ms*/) {
    let args: Vec<String> = env::args().collect();

    let once = args.iter().any(|a| a == "--once");
    let watch = args.iter().any(|a| a == "--watch");

    // Hard guard: mutually exclusive modes.
    if once && watch {
        eprintln!("WTG usage error: --once and --watch are mutually exclusive.");
        process::exit(2);
    }

    // Parse `--interval <ms>` if present.
    // We intentionally *do not* accept `--interval` without a value.
    let mut interval_ms: Option<u64> = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--interval" {
            // Require a next token.
            if i + 1 >= args.len() {
                eprintln!("WTG usage error: --interval requires a value in milliseconds (e.g., --interval 1000).");
                process::exit(2);
            }

            let v = &args[i + 1];
            let parsed = v.parse::<u64>().unwrap_or_else(|_| {
                eprintln!("WTG usage error: --interval value must be an integer milliseconds value. Got: {v}");
                process::exit(2);
            });

            interval_ms = Some(parsed);
            i += 2;
            continue;
        }
        i += 1;
    }

    (once, watch, interval_ms)
}

fn main() {
    // Initialize logging early. This is safe in all modes and helps diagnostics on Windows.
    tracing_subscriber::fmt::init();

    info!(
        "WTG v{} initializing...",
        env!("CARGO_PKG_VERSION")
        );


    let (once, watch, interval_ms_opt) = parse_args();

    // Print banner once per run (not on every tick).
    println!(
        "WTG — WhatTheGPU v{}",
        env!("CARGO_PKG_VERSION")
        );
    println!("Honest GPU compute stats for Windows");

    // NOTE: `--interval` is a parameter, not a mode.
    // If user provides it without `--watch`, we ignore it (optionally warn).
    if interval_ms_opt.is_some() && !watch {
        eprintln!("WTG note: --interval is only used with --watch; ignoring for this run.");
    }

    // Mode: `--once`
    if once {
        println!("\nWTG snapshot (NVML)\n");
        match wtg_core::nvml::snapshot_all() {
            Ok(snaps) => {
                for s in snaps {
                    println!("{s}");
                }
            }
            Err(e) => {
                eprintln!("WTG --once failed: {e}");
                process::exit(2);
            }
        }
        return;
    }

    // Mode: `--watch`
    if watch {
        // Determine the sampling period.
        let interval_ms = interval_ms_opt.unwrap_or(DEFAULT_INTERVAL_MS);

        // Guardrail: avoid accidental near-zero hot loops that add noise.
        // We still allow low values if you intentionally set them, but we can warn.
        if interval_ms < 100 {
            eprintln!("WTG note: very low interval ({interval_ms}ms). NVML metrics may not update this quickly; expect duplicates.");
        }

        println!("\nWTG watch mode (NVML) — interval {} ms\n", interval_ms);

        let sleep_dur = Duration::from_millis(interval_ms);

        loop {
            // Snapshot at the top of the loop. If NVML fails, error and exit non-zero.
            match wtg_core::nvml::snapshot_all() {
                Ok(snaps) => {
                    // Timestamp each tick for correlation and to prove we are refreshing.
                    println!("--- tick {} ---", now_ts());
                    for s in snaps {
                        println!("{s}");
                    }
                    println!();
                }
                Err(e) => {
                    eprintln!("WTG --watch failed: {e}");
                    process::exit(2);
                }
            }

            // Sleep until next tick. Ctrl+C will terminate the process naturally.
            thread::sleep(sleep_dur);
        }
    }

    // Default behavior (no flags):
    // Keep the placeholder, because TUI is explicitly not built yet.
    println!("\nTUI initialization in progress...");
}
