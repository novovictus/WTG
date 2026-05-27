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
//! - CLI remains the validation/capture path; wtg-ui.exe is the visual/demo/operator surface.

use std::env;
use std::process;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tracing::info;

mod probe;
mod probe_fields;
mod sink;

use probe::{format_probe_csv_header, format_probe_csv_row, format_probe_record, ProbeRecord};
use probe_fields::{
    format_field_value, format_probe_fields_csv_header, format_probe_fields_csv_row,
    format_probe_fields_snapshot,
};
use sink::{Sink, SinkKind};

/// Default sampling interval when `--watch` is enabled.
/// 1000ms is conservative and matches NVML’s practical update cadence for many metrics.
const DEFAULT_INTERVAL_MS: u64 = 1000;

/// Stats output schema version.
/// This lets us evolve the key set while remaining explicit in artifacts.
const STATS_SCHEMA: u32 = 0;

struct CliArgs {
    once: bool,
    watch: bool,
    probe: bool,
    probe_fields: bool,
    stats: bool,
    help: bool,
    version: bool,
    interval_ms: Option<u64>,
    sink: Option<SinkKind>,
    field_ids: Vec<u32>,
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
fn format_stats_schema_header() -> String {
    format!("stats.schema: {}\n\n", STATS_SCHEMA)
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

fn optional_u32_string(value: Option<u32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "N/A".to_string())
}

fn optional_w_string(value: Option<f32>) -> String {
    value
        .map(|value| format!("{value:.1}"))
        .unwrap_or_else(|| "N/A".to_string())
}

fn format_snapshot_csv_header() -> &'static str {
    "wtg_version,tick_seq,tick_ts,gpu_index,gpu_name,gpu_uuid,temp_c,util_gpu_pct,util_mem_controller_pct,vram_used_mib,vram_total_mib,power_w,power_limit_w"
}

fn format_snapshot_csv_row(
    s: &wtg_core::nvml::GpuSnapshot,
    tick_seq: u64,
    tick_ts: &str,
) -> String {
    sink::format_csv_row(&[
        env!("CARGO_PKG_VERSION").to_string(),
        tick_seq.to_string(),
        tick_ts.to_string(),
        s.index.to_string(),
        s.name.clone(),
        s.uuid.clone(),
        optional_u32_string(s.temp_c),
        s.gpu_util_pct.to_string(),
        s.mem_util_pct.to_string(),
        bytes_to_mib(s.mem_used_bytes).to_string(),
        bytes_to_mib(s.mem_total_bytes).to_string(),
        optional_w_string(mw_to_w(s.power_mw)),
        optional_w_string(mw_to_w(s.power_limit_mw)),
    ])
}

fn format_stats_csv_header() -> &'static str {
    "wtg_version,stats_schema,tick_seq,tick_ts,gpu_index,gpu_name,gpu_uuid,temp_c,util_gpu_pct,util_mem_controller_pct,vram_used_mib,vram_total_mib,power_w,power_limit_w"
}

fn format_stats_csv_row(s: &wtg_core::nvml::GpuSnapshot, tick_seq: u64, tick_ts: &str) -> String {
    sink::format_csv_row(&[
        env!("CARGO_PKG_VERSION").to_string(),
        STATS_SCHEMA.to_string(),
        tick_seq.to_string(),
        tick_ts.to_string(),
        s.index.to_string(),
        s.name.clone(),
        s.uuid.clone(),
        optional_u32_string(s.temp_c),
        s.gpu_util_pct.to_string(),
        s.mem_util_pct.to_string(),
        bytes_to_mib(s.mem_used_bytes).to_string(),
        bytes_to_mib(s.mem_total_bytes).to_string(),
        optional_w_string(mw_to_w(s.power_mw)),
        optional_w_string(mw_to_w(s.power_limit_mw)),
    ])
}

/// Print one GPU in stable "key: value" form.
/// NOTE: This assumes wtg_core::nvml::GpuSnapshot exposes these fields publicly.
/// If field names differ, adjust the mappings here (only here).
fn format_stats_block(s: &wtg_core::nvml::GpuSnapshot) -> String {
    format!(
        concat!(
            "[stats] gpu={}\n",
            "gpu.index: {}\n",
            "gpu.name: {}\n",
            "gpu.uuid: {}\n",
            "temp.c: {}\n",
            "util.gpu_pct: {}\n",
            "util.mem_controller_pct: {}\n",
            "vram.used_mib: {}\n",
            "vram.total_mib: {}\n",
            "power.w: {}\n",
            "power.limit_w: {}\n",
            "\n"
        ),
        s.index,
        s.index,
        s.name,
        s.uuid,
        optional_u32_string(s.temp_c),
        s.gpu_util_pct,
        s.mem_util_pct,
        bytes_to_mib(s.mem_used_bytes),
        bytes_to_mib(s.mem_total_bytes),
        optional_w_string(mw_to_w(s.power_mw)),
        optional_w_string(mw_to_w(s.power_limit_mw))
    )
}

fn print_and_mirror_jsonl(text: &str, sink: &Option<Sink>) {
    print!("{text}");
    if let Some(sink) = sink {
        if sink.kind() == SinkKind::Jsonl {
            sink.emit_jsonl_lines(text);
        }
    }
}

fn print_help() {
    print!(concat!(
        "WTG - WhatTheGPU v",
        env!("CARGO_PKG_VERSION"),
        "\n",
        "\n",
        "Usage:\n",
        "  wtg.exe --once [--stats] [--sink jsonl|csv]\n",
        "  wtg.exe --watch [--interval <ms>] [--stats] [--sink jsonl|csv]\n",
        "  wtg.exe --probe [--sink jsonl|csv]\n",
        "  wtg.exe --probe-fields --field-id <u32> [--field-id <u32> ...] [--sink jsonl|csv]\n",
        "\n",
        "Options:\n",
        "  --once                  Capture a single GPU snapshot and exit.\n",
        "  --watch                 Continuously poll GPU state.\n",
        "  --interval <ms>         Polling interval in milliseconds for --watch.\n",
        "  --stats                 Print stable key:value stats output for --once or --watch.\n",
        "  --probe                 Capture one context-rich probe block.\n",
        "  --probe-fields          Query explicit NVML field-value IDs.\n",
        "  --field-id <u32>        Repeatable field ID for --probe-fields.\n",
        "  --sink jsonl|csv        Write a timestamped sink file.\n",
        "  --help / -h             Print this help text.\n",
        "  --version / -V          Print version and exit.\n"
    ));
}

fn print_version() {
    println!("WTG - WhatTheGPU v{}", env!("CARGO_PKG_VERSION"));
}

fn usage_error(message: &str) -> ! {
    eprintln!("WTG usage error: {message}");
    process::exit(2);
}

fn parse_args() -> CliArgs {
    let args: Vec<String> = env::args().collect();

    let mut parsed = CliArgs {
        once: false,
        watch: false,
        probe: false,
        probe_fields: false,
        stats: false,
        help: false,
        version: false,
        interval_ms: None,
        sink: None,
        field_ids: Vec::new(),
    };

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--once" => {
                parsed.once = true;
                i += 1;
            }
            "--watch" => {
                parsed.watch = true;
                i += 1;
            }
            "--probe" => {
                parsed.probe = true;
                i += 1;
            }
            "--probe-fields" => {
                parsed.probe_fields = true;
                i += 1;
            }
            "--stats" => {
                parsed.stats = true;
                i += 1;
            }
            "--help" | "-h" => {
                parsed.help = true;
                i += 1;
            }
            "--version" | "-V" => {
                parsed.version = true;
                i += 1;
            }
            "--interval" => {
                if i + 1 >= args.len() {
                    usage_error(
                        "--interval requires a value in milliseconds (e.g., --interval 1000).",
                    );
                }

                let v = &args[i + 1];
                let interval_ms = v.parse::<u64>().unwrap_or_else(|_| {
                    usage_error(&format!(
                        "--interval value must be an integer milliseconds value. Got: {v}"
                    ));
                });

                parsed.interval_ms = Some(interval_ms);
                i += 2;
            }
            "--field-id" => {
                if i + 1 >= args.len() {
                    usage_error("--field-id requires a u32 field ID value.");
                }

                let v = &args[i + 1];
                let field_id = v.parse::<u32>().unwrap_or_else(|_| {
                    usage_error(&format!("--field-id value must be a u32 integer. Got: {v}"));
                });

                parsed.field_ids.push(field_id);
                i += 2;
            }
            "--sink" => {
                if i + 1 >= args.len() {
                    usage_error("--sink requires a value (csv or jsonl).");
                }

                let v = &args[i + 1];
                parsed.sink = Some(match v.as_str() {
                    "csv" => SinkKind::Csv,
                    "jsonl" => SinkKind::Jsonl,
                    _ => usage_error(&format!("--sink value must be csv or jsonl. Got: {v}")),
                });

                i += 2;
            }
            other if other.starts_with('-') => {
                usage_error(&format!("unknown flag: {other}"));
            }
            other => {
                usage_error(&format!("unexpected argument: {other}"));
            }
        }
    }

    let once = parsed.once;
    let watch = parsed.watch;
    let probe = parsed.probe;
    let probe_fields = parsed.probe_fields;
    let stats = parsed.stats;

    if once && watch {
        usage_error("--once and --watch are mutually exclusive.");
    }

    if probe && (once || watch) {
        usage_error("--probe is mutually exclusive with --once and --watch.");
    }

    if probe_fields && (once || watch || probe) {
        usage_error("--probe-fields is mutually exclusive with --once, --watch, and --probe.");
    }

    if stats && !once && !watch {
        usage_error("--stats requires --once or --watch.");
    }

    if parsed.interval_ms.is_some() && !watch {
        usage_error("--interval is valid only with --watch.");
    }

    if !probe_fields && !parsed.field_ids.is_empty() {
        usage_error("--field-id requires --probe-fields.");
    }

    if probe_fields && parsed.field_ids.is_empty() {
        usage_error("--probe-fields requires at least one --field-id <u32>.");
    }

    if parsed.sink.is_some() && !once && !watch && !probe && !probe_fields {
        usage_error("--sink requires --once, --watch, --probe, or --probe-fields.");
    }

    parsed
}

fn main() {
    let args = parse_args();

    if args.help {
        print_help();
        return;
    }

    if args.version {
        print_version();
        return;
    }

    // Initialize logging early. This is safe in all modes and helps diagnostics on Windows.
    tracing_subscriber::fmt::init();

    info!("WTG v{} initializing...", env!("CARGO_PKG_VERSION"));

    let sink = match args.sink {
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

    // Mode: `--probe-fields`
    if args.probe_fields {
        let ctx = match wtg_core::nvml::init_context() {
            Ok(ctx) => ctx,
            Err(e) => {
                eprintln!("WTG --probe-fields init failed: {e}");
                process::exit(2);
            }
        };

        match wtg_core::nvml::snapshot_all_with_ctx(&ctx) {
            Ok(snaps) => {
                let mut wrote_csv_header = false;
                for s in snaps.iter() {
                    let context =
                        wtg_core::nvml::probe_context::query_probe_context_for_gpu_with_ctx(
                            &ctx, s.index,
                        );
                    let block = format_probe_fields_snapshot(s, &context);
                    print_and_mirror_jsonl(&block, &sink);

                    let fields = wtg_core::nvml::field_values::query_field_values_for_gpu(
                        &ctx,
                        s.index,
                        &args.field_ids,
                    );
                    if let Some(sink) = &sink {
                        if sink.kind() == SinkKind::Csv && !wrote_csv_header {
                            sink.emit_raw_line(format_probe_fields_csv_header());
                            wrote_csv_header = true;
                        }
                    }
                    for field in fields.iter() {
                        let field_block = format_field_value(s.index, field);
                        print_and_mirror_jsonl(&field_block, &sink);
                        if let Some(sink) = &sink {
                            if sink.kind() == SinkKind::Csv {
                                sink.emit_raw_line(&format_probe_fields_csv_row(
                                    s, &context, field,
                                ));
                            }
                        }
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

    // Mode: `--probe`
    if args.probe {
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
                    print_and_mirror_jsonl(&block, &sink);
                    if let Some(sink) = &sink {
                        match sink.kind() {
                            SinkKind::Jsonl => {}
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

    // Print banner once per run (not on every tick). This is console-only for
    // snapshot/watch modes; JSONL remains telemetry-oriented on those paths.
    println!("WTG - WhatTheGPU v{}", env!("CARGO_PKG_VERSION"));
    println!("Honest GPU compute stats for Windows");

    // Mode: `--once`
    if args.once {
        match wtg_core::nvml::snapshot_all() {
            Ok(snaps) => {
                let tick_seq = 0;
                let tick_ts = now_ts();
                if args.stats {
                    let header = format_stats_schema_header();
                    print_and_mirror_jsonl(&header, &sink);
                    if let Some(sink) = &sink {
                        if sink.kind() == SinkKind::Csv {
                            sink.emit_raw_line(format_stats_csv_header());
                        }
                    }
                    for s in snaps.iter() {
                        let block = format_stats_block(s);
                        print_and_mirror_jsonl(&block, &sink);
                        if let Some(sink) = &sink {
                            if sink.kind() == SinkKind::Csv {
                                sink.emit_raw_line(&format_stats_csv_row(s, tick_seq, &tick_ts));
                            }
                        }
                    }
                } else {
                    println!("\nWTG snapshot (NVML)\n");
                    if let Some(sink) = &sink {
                        if sink.kind() == SinkKind::Csv {
                            sink.emit_raw_line(format_snapshot_csv_header());
                        }
                    }
                    for s in snaps.iter() {
                        let line = format!("{s}");
                        println!("{line}");
                        if let Some(sink) = &sink {
                            match sink.kind() {
                                SinkKind::Jsonl => sink.emit_jsonl_lines(&line),
                                SinkKind::Csv => {
                                    sink.emit_raw_line(&format_snapshot_csv_row(
                                        s, tick_seq, &tick_ts,
                                    ));
                                }
                            }
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
    if args.watch {
        let interval_ms = args.interval_ms.unwrap_or(DEFAULT_INTERVAL_MS);

        if interval_ms < 100 {
            eprintln!("WTG note: very low interval ({interval_ms}ms). NVML metrics may not update this quickly; expect duplicates.");
        }

        if args.stats {
            let header = format_stats_schema_header();
            print_and_mirror_jsonl(&header, &sink);
            print_and_mirror_jsonl(&format!("watch.interval_ms: {interval_ms}\n\n"), &sink);
            if let Some(sink) = &sink {
                if sink.kind() == SinkKind::Csv {
                    sink.emit_raw_line(format_stats_csv_header());
                }
            }
        } else {
            println!("\nWTG watch mode (NVML) - interval {} ms\n", interval_ms);
            if let Some(sink) = &sink {
                if sink.kind() == SinkKind::Csv {
                    sink.emit_raw_line(format_snapshot_csv_header());
                }
            }
        }

        let sleep_dur = Duration::from_millis(interval_ms);

        if args.stats {
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
                        let tick_ts = now_ts();
                        print_and_mirror_jsonl(&format!("tick.seq: {tick_seq}\n"), &sink);
                        print_and_mirror_jsonl(&format!("tick.ts: {tick_ts}\n"), &sink);
                        for s in snaps.iter() {
                            let block = format_stats_block(s);
                            print_and_mirror_jsonl(&block, &sink);
                            if let Some(sink) = &sink {
                                if sink.kind() == SinkKind::Csv {
                                    sink.emit_raw_line(&format_stats_csv_row(
                                        s, tick_seq, &tick_ts,
                                    ));
                                }
                            }
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

                thread::sleep(sleep_dur);
            }
        } else {
            let mut tick_seq: u64 = 0;
            loop {
                match wtg_core::nvml::snapshot_all() {
                    Ok(snaps) => {
                        let tick_ts = now_ts();
                        let tick_line = format!("--- tick {tick_ts} ---");
                        println!("{tick_line}");
                        for s in snaps.iter() {
                            let line = format!("{s}");
                            println!("{line}");
                            if let Some(sink) = &sink {
                                match sink.kind() {
                                    SinkKind::Jsonl => sink.emit_jsonl_lines(&line),
                                    SinkKind::Csv => {
                                        sink.emit_raw_line(&format_snapshot_csv_row(
                                            s, tick_seq, &tick_ts,
                                        ));
                                    }
                                }
                            }
                        }
                        println!();
                        tick_seq += 1;
                    }
                    Err(e) => {
                        eprintln!("WTG --watch failed: {e}");
                        process::exit(2);
                    }
                }

                thread::sleep(sleep_dur);
            }
        }
    }

    // Default behavior (no flags):
    // Keep the placeholder, because TUI is explicitly not built yet.
    println!("\nRun with --once, --watch, --probe, or --probe-fields. Use wtg-ui.exe for the experimental UI.");
}
