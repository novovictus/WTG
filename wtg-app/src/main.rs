// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Adam Hooper
// //! WTG App - TUI for GPU metric validation
//!
//! Entry point for the WTG proof-of-concept.
//!
//! Current modes (intentionally minimal, no clap):
//!   --once                     : Take one NVML snapshot, print, exit.
//!   --watch                    : Take repeated NVML snapshots, print each tick.
//!   --watch --interval <ms>    : Same, but set period in milliseconds (default 1000ms).
//!
//! Optional output mode:
//!   --stats                    : Print a stable key:value "stats block" format (schema 0).
//!                               Requires --once or --watch. Does not change default output.
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

/// Stats output schema version.
/// This lets us evolve the key set while remaining explicit in artifacts.
const STATS_SCHEMA: u32 = 0;

#[derive(Debug, Clone, Copy)]
enum SinkKind {
    Csv,
    Jsonl,
}

#[allow(dead_code)]
struct Sink {
    kind: SinkKind,
}

impl Sink {
    fn new(kind: SinkKind) -> Self {
        Self { kind }
    }

    #[allow(dead_code)]
    fn emit(&self, _line: &str) {
        // no-op for now
    }
}

/// Returns a simple timestamp like "1707101234.567" (unix seconds.millis).
/// No external deps; good enough for proof and log correlation.
fn now_ts() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => format!("{}.{:03}", d.as_secs(), d.subsec_millis()),
        Err(_) => "N/A".to_string(),
    }
}

/// Print the stats schema once per run when `--stats` is enabled.
fn print_stats_schema_header() {
    println!("stats.schema: {}", STATS_SCHEMA);
    println!();
}

/// --- Unit conversion helpers ------------------------------------------------
/// Convert raw NVML memory values (bytes) into mebibytes (MiB).
/// NVML reports memory in bytes; MiB keeps output human-readable and
/// consistent with tools like nvidia-smi.
fn bytes_to_mib(b: u64) -> u64 {
    b / (1024 * 1024)
}

/// Convert raw NVML power values from milliwatts to watts.
/// NVML reports power in milliwatts; some platforms may not report power at all,
/// so we accept Option and propagate None rather than forcing an unwrap.
fn mw_to_w(mw: Option<u32>) -> Option<f32> {
    mw.map(|x| (x as f32) / 1000.0)
}

/// Print one GPU in stable "key: value" form.
/// NOTE: This assumes wtg_core::nvml::GpuSnapshot exposes these fields publicly.
/// If field names differ, adjust the mappings here (only here).
fn print_stats_block(s: &wtg_core::nvml::GpuSnapshot) {
    println!("[stats] gpu={}", s.index);

    // Identity
    println!("gpu.index: {}", s.index);
    println!("gpu.name: {}", s.name);
    println!("gpu.uuid: {}", s.uuid);

    // Core telemetry (Basic tier)
    println!(
        "temp.c: {}",
        s.temp_c
            .map(|t| t.to_string())
            .unwrap_or_else(|| "N/A".to_string())
    );
    println!("util.gpu_pct: {}", s.gpu_util_pct);
    println!("util.mem_pct: {}", s.mem_util_pct);

    println!("vram.used_mib: {}", bytes_to_mib(s.mem_used_bytes));
    println!("vram.total_mib: {}", bytes_to_mib(s.mem_total_bytes));

    println!(
        "power.w: {}",
        mw_to_w(s.power_mw)
            .map(|w| format!("{w:.1}"))
            .unwrap_or_else(|| "N/A".to_string())
    );

    println!(
        "power.limit_w: {}",
        mw_to_w(s.power_limit_mw)
            .map(|w| format!("{w:.1}"))
            .unwrap_or_else(|| "N/A".to_string())
    );

    println!();
}

/// Minimal argument parser for our small surface area.
///
/// Semantics:
/// - `--once` and `--watch` are mutually exclusive (error if both).
/// - `--stats` is an output modifier and requires `--once` or `--watch`.
/// - `--interval <ms>`:
///     - requires a value
///     - parsed as u64 milliseconds
///     - only used with `--watch`
/// - Unknown flags are ignored for now (keeps dev friction low during bootstrap).
fn parse_args() -> (
    bool,        /*once*/
    bool,        /*watch*/
    bool,        /*stats*/
    Option<u64>, /*interval_ms*/
    Option<SinkKind>,
) {
    let args: Vec<String> = env::args().collect();

    let once = args.iter().any(|a| a == "--once");
    let watch = args.iter().any(|a| a == "--watch");
    let stats = args.iter().any(|a| a == "--stats");

    // Hard guard: mutually exclusive modes.
    if once && watch {
        eprintln!("WTG usage error: --once and --watch are mutually exclusive.");
        process::exit(2);
    }

    // `--stats` is a modifier; do not change default behavior.
    // Require an explicit mode so "wtg.exe --stats" doesn't unexpectedly change output.
    if stats && !once && !watch {
        eprintln!("WTG usage error: --stats requires --once or --watch.");
        process::exit(2);
    }

    // Parse `--interval <ms>` if present.
    // We intentionally *do not* accept `--interval` without a value.
    let mut interval_ms: Option<u64> = None;
    let mut sink: Option<SinkKind> = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--interval" {
            // Require a next token.
            if i + 1 >= args.len() {
                eprintln!(
                    "WTG usage error: --interval requires a value in milliseconds (e.g., --interval 1000)."
                );
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

        if args[i] == "--sink" {
            // Require a next token.
            if i + 1 >= args.len() {
                eprintln!("WTG usage error: --sink requires a value (csv or jsonl).");
                process::exit(2);
            }

            let v = &args[i + 1];
            sink = Some(match v.as_str() {
                "csv" => SinkKind::Csv,
                "jsonl" => SinkKind::Jsonl,
                _ => {
                    eprintln!("WTG usage error: --sink value must be csv or jsonl. Got: {v}");
                    process::exit(2);
                }
            });

            i += 2;
            continue;
        }
        i += 1;
    }

    (once, watch, stats, interval_ms, sink)
}

fn main() {
    // Initialize logging early. This is safe in all modes and helps diagnostics on Windows.
    tracing_subscriber::fmt::init();

    info!("WTG v{} initializing...", env!("CARGO_PKG_VERSION"));

    let (once, watch, stats, interval_ms_opt, sink_opt) = parse_args();

    let _sink = sink_opt.map(Sink::new);

    // Print banner once per run (not on every tick).
    println!("WTG - WhatTheGPU v{}", env!("CARGO_PKG_VERSION"));
    println!("Honest GPU compute stats for Windows");

    // NOTE: `--interval` is a parameter, not a mode.
    // If user provides it without `--watch`, we ignore it (optionally warn).
    if interval_ms_opt.is_some() && !watch {
        eprintln!("WTG note: --interval is only used with --watch; ignoring for this run.");
    }

    // Mode: `--once`
    if once {
        match wtg_core::nvml::snapshot_all() {
            Ok(snaps) => {
                if stats {
                    print_stats_schema_header();
                    for s in snaps.iter() {
                        print_stats_block(s);
                    }
                } else {
                    println!("\nWTG snapshot (NVML)\n");
                    for s in snaps {
                        let line = format!("{s}");
                        println!("{line}");
                        if let Some(sink) = &_sink {
                            sink.emit(&line);
                        }
                    }
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

        if stats {
            print_stats_schema_header();
            println!("watch.interval_ms: {interval_ms}");
            println!();
        } else {
            println!("\nWTG watch mode (NVML) - interval {} ms\n", interval_ms);
        }

        let sleep_dur = Duration::from_millis(interval_ms);

        if stats {
            let mut ctx = loop {
                match wtg_core::nvml::init_context() {
                    Ok(ctx) => break ctx,
                    Err(e) => {
                        eprintln!("WTG --watch init failed: {e}");
                        thread::sleep(sleep_dur);
                    }
                }
            };

            let mut tick_seq: u64 = 0;
            loop {
                match wtg_core::nvml::snapshot_all_with_ctx(&ctx) {
                    Ok(snaps) => {
                        println!("tick.seq: {tick_seq}");
                        println!("tick.ts: {}", now_ts());
                        for s in snaps.iter() {
                            print_stats_block(s);
                        }
                        tick_seq += 1;
                    }
                    Err(e) => {
                        eprintln!("WTG --watch failed: {e}");
                        match wtg_core::nvml::init_context() {
                            Ok(new_ctx) => {
                                ctx = new_ctx;
                            }
                            Err(e2) => {
                                eprintln!("WTG --watch re-init failed: {e2}");
                            }
                        }
                    }
                }

                // Sleep until next tick. Ctrl+C will terminate the process naturally.
                thread::sleep(sleep_dur);
            }
        } else {
            loop {
                match wtg_core::nvml::snapshot_all() {
                    Ok(snaps) => {
                        // Timestamp each tick for correlation and to prove we are refreshing.
                        println!("--- tick {} ---", now_ts());
                        for s in snaps {
                            let line = format!("{s}");
                            println!("{line}");
                            if let Some(sink) = &_sink {
                                sink.emit(&line);
                            }
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
    }

    // Default behavior (no flags):
    // Keep the placeholder, because TUI is explicitly not built yet.
    println!("\nTUI initialization in progress...");
}
