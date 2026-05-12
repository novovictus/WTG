// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Adam Hooper
//! WTG App - CLI for GPU metric validation.
//!
//! Entry point for the WTG proof-of-concept.
//!
//! Current modes (intentionally minimal, no clap):
//!   --once                     : Take one NVML snapshot, print, exit.
//!   --watch                    : Take repeated NVML snapshots, print each tick.
//!   --watch --interval <ms>    : Same, but set period in milliseconds (default 1000ms).
//!   --probe                    : Take one NVML snapshot, print probe fields, exit.
//!   --probe-fields             : Take one NVML snapshot, print requested NVML field values, exit.
//!   --field-id <u32>           : Repeatable field ID parameter for --probe-fields.
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

mod probe;
mod probe_fields;
mod sink;

use probe::{format_probe_csv_header, format_probe_csv_row, format_probe_record, ProbeRecord};
use probe_fields::{format_field_value, format_probe_fields_snapshot};
use sink::{Sink, SinkKind};

/// Default sampling interval when `--watch` is enabled.
/// 1000ms is conservative and matches NVML’s practical update cadence for many metrics.
const DEFAULT_INTERVAL_MS: u64 = 1000;

/// Stats output schema version.
/// This lets us evolve the key set while remaining explicit in artifacts.
const STATS_SCHEMA: u32 = 0;

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
    println!("util.mem_controller_pct: {}", s.mem_util_pct);

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
/// - `--probe` is mutually exclusive with `--once` and `--watch`.
/// - `--probe-fields` is mutually exclusive with `--once`, `--watch`, and `--probe`.
/// - `--stats` is an output modifier and requires `--once` or `--watch`.
/// - `--interval <ms>`:
///     - requires a value
///     - parsed as u64 milliseconds
///     - only used with `--watch`
/// - `--field-id <u32>`:
///     - repeatable
///     - required by `--probe-fields`
///     - invalid without `--probe-fields`
/// - Unknown flags are ignored for now (keeps dev friction low during bootstrap).
fn parse_args() -> (
    bool,        /*once*/
    bool,        /*watch*/
    bool,        /*probe*/
    bool,        /*probe_fields*/
    bool,        /*stats*/
    Option<u64>, /*interval_ms*/
    Option<SinkKind>,
    Vec<u32>, /*field_ids*/
) {
    let args: Vec<String> = env::args().collect();

    let once = args.iter().any(|a| a == "--once");
    let watch = args.iter().any(|a| a == "--watch");
    let probe = args.iter().any(|a| a == "--probe");
    let probe_fields = args.iter().any(|a| a == "--probe-fields");
    let stats = args.iter().any(|a| a == "--stats");

    // Hard guard: mutually exclusive modes.
    if once && watch {
        eprintln!("WTG usage error: --once and --watch are mutually exclusive.");
        process::exit(2);
    }

    if probe && (once || watch) {
        eprintln!("WTG usage error: --probe is mutually exclusive with --once and --watch.");
        process::exit(2);
    }

    if probe_fields && (once || watch || probe) {
        eprintln!(
            "WTG usage error: --probe-fields is mutually exclusive with --once, --watch, and --probe."
        );
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
    let mut field_ids: Vec<u32> = Vec::new();
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

        if args[i] == "--field-id" {
            // Require a next token.
            if i + 1 >= args.len() {
                eprintln!("WTG usage error: --field-id requires a u32 field ID value.");
                process::exit(2);
            }

            let v = &args[i + 1];
            let parsed = v.parse::<u32>().unwrap_or_else(|_| {
                eprintln!("WTG usage error: --field-id value must be a u32 integer. Got: {v}");
                process::exit(2);
            });

            field_ids.push(parsed);
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

    if !probe_fields && !field_ids.is_empty() {
        eprintln!("WTG usage error: --field-id requires --probe-fields.");
        process::exit(2);
    }

    if probe_fields && field_ids.is_empty() {
        eprintln!("WTG usage error: --probe-fields requires at least one --field-id <u32>.");
        process::exit(2);
    }

    if stats && sink.is_some() {
        eprintln!("WTG usage error: --sink is not supported with --stats in this beta.");
        process::exit(2);
    }

    if probe_fields && sink.is_some() {
        eprintln!("WTG usage error: --sink is not supported with --probe-fields in this beta.");
        process::exit(2);
    }

    (
        once,
        watch,
        probe,
        probe_fields,
        stats,
        interval_ms,
        sink,
        field_ids,
    )
}

fn main() {
    // Initialize logging early. This is safe in all modes and helps diagnostics on Windows.
    tracing_subscriber::fmt::init();

    info!("WTG v{} initializing...", env!("CARGO_PKG_VERSION"));

    let (once, watch, probe, probe_fields, stats, interval_ms_opt, sink_opt, field_ids) =
        parse_args();

    // Mode: `--probe-fields`
    if probe_fields {
        let ctx = match wtg_core::nvml::init_context() {
            Ok(ctx) => ctx,
            Err(e) => {
                eprintln!("WTG --probe-fields init failed: {e}");
                process::exit(2);
            }
        };

        match wtg_core::nvml::snapshot_all_with_ctx(&ctx) {
            Ok(snaps) => {
                for s in snaps.iter() {
                    let context =
                        wtg_core::nvml::probe_context::query_probe_context_for_gpu_with_ctx(
                            &ctx, s.index,
                        );
                    print!("{}", format_probe_fields_snapshot(s, &context));

                    let fields = wtg_core::nvml::field_values::query_field_values_for_gpu(
                        &ctx, s.index, &field_ids,
                    );
                    for field in fields.iter() {
                        print!("{}", format_field_value(s.index, field));
                    }
                }
            }
            Err(e) => {
                eprintln!("WTG --probe-fields failed: {e}");
                process::exit(2);
            }
        }
        return;
    }

    let _sink = match sink_opt {
        Some(kind) => match Sink::new(kind) {
            Ok(sink) => {
                eprintln!("WTG note: sink enabled: {}", sink.filename());
                Some(sink)
            }
            Err(e) => {
                eprintln!("WTG runtime error: failed to create sink output file: {e}");
                process::exit(2);
            }
        },
        None => None,
    };

    // Mode: `--probe`
    if probe {
        let probe_context_ctx = match wtg_core::nvml::init_context() {
            Ok(ctx) => ctx,
            Err(e) => {
                eprintln!("WTG --probe init failed: {e}");
                process::exit(2);
            }
        };
        match wtg_core::nvml::snapshot_all_with_ctx(&probe_context_ctx) {
            Ok(snaps) => {
                let mut wrote_csv_header = false;
                for s in snaps.iter() {
                    let context =
                        wtg_core::nvml::probe_context::query_probe_context_for_gpu_with_ctx(
                            &probe_context_ctx,
                            s.index,
                        );
                    let record = ProbeRecord::from_snapshot(s, context);
                    let block = format_probe_record(&record);
                    print!("{block}");
                    if let Some(sink) = &_sink {
                        match sink.kind() {
                            SinkKind::Jsonl => sink.emit(&block),
                            SinkKind::Csv => {
                                if !wrote_csv_header {
                                    sink.emit_raw_line(format_probe_csv_header());
                                    wrote_csv_header = true;
                                }
                                sink.emit_raw_line(&format_probe_csv_row(&record));
                            }
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("WTG --probe failed: {e}");
                process::exit(2);
            }
        }
        return;
    }

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
    println!("\nRun with --once, --watch, --probe, or --probe-fields. Use wtg-ui.exe for the experimental UI.");
}
