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
//!   --sink mqtt                : Publish experimental MQTT state payloads during --watch.
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

mod mqtt;
mod probe;
mod probe_fields;
mod sink;

use mqtt::{
    MqttAuthOptions, MqttHaDiscoveryOptions, MqttOptions, MqttSink, DEFAULT_HA_DISCOVERY_PREFIX,
    DEFAULT_MQTT_PORT, DEFAULT_TOPIC_PREFIX,
};
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
    mqtt_host: Option<String>,
    mqtt_port: Option<String>,
    mqtt_topic_prefix: Option<String>,
    mqtt_node_id: Option<String>,
    mqtt_username: Option<String>,
    mqtt_password_env: Option<String>,
    mqtt_ha_discovery: bool,
    mqtt_ha_prefix: Option<String>,
    mqtt_retain_discovery: bool,
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
        "  wtg.exe --watch [--interval <ms>] [--stats] [--sink jsonl|csv|mqtt]\n",
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
        "  --sink jsonl|csv|mqtt   Select an output sink. jsonl/csv write timestamped files; mqtt publishes during --watch.\n",
        "  --mqtt-host <host>      MQTT broker host. Required with --sink mqtt.\n",
        "  --mqtt-port <port>      MQTT broker port. Default: 1883.\n",
        "  --mqtt-topic-prefix <p> MQTT topic prefix. Default: wtg.\n",
        "  --mqtt-node-id <id>     MQTT node ID. Required with --sink mqtt.\n",
        "  --mqtt-username <user>  MQTT username. Requires --mqtt-password-env.\n",
        "  --mqtt-password-env <v> Read MQTT password from environment variable. Requires --mqtt-username.\n",
        "  --mqtt-ha-discovery     Publish Home Assistant MQTT discovery configs with --sink mqtt.\n",
        "  --mqtt-ha-prefix <p>    Home Assistant discovery prefix. Default: homeassistant.\n",
        "  --mqtt-retain-discovery Retain Home Assistant discovery configs. State messages remain non-retained.\n",
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
        mqtt_host: None,
        mqtt_port: None,
        mqtt_topic_prefix: None,
        mqtt_node_id: None,
        mqtt_username: None,
        mqtt_password_env: None,
        mqtt_ha_discovery: false,
        mqtt_ha_prefix: None,
        mqtt_retain_discovery: false,
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
                    usage_error("--sink requires a value (csv, jsonl, or mqtt).");
                }

                let v = &args[i + 1];
                parsed.sink = Some(match v.as_str() {
                    "csv" => SinkKind::Csv,
                    "jsonl" => SinkKind::Jsonl,
                    "mqtt" => SinkKind::Mqtt,
                    _ => usage_error(&format!(
                        "--sink value must be csv, jsonl, or mqtt. Got: {v}"
                    )),
                });

                i += 2;
            }
            "--mqtt-host" => {
                if i + 1 >= args.len() {
                    usage_error("--mqtt-host requires a host value.");
                }

                parsed.mqtt_host = Some(args[i + 1].clone());
                i += 2;
            }
            "--mqtt-port" => {
                if i + 1 >= args.len() {
                    usage_error("--mqtt-port requires a port value.");
                }

                parsed.mqtt_port = Some(args[i + 1].clone());
                i += 2;
            }
            "--mqtt-topic-prefix" => {
                if i + 1 >= args.len() {
                    usage_error("--mqtt-topic-prefix requires a topic prefix value.");
                }

                parsed.mqtt_topic_prefix = Some(args[i + 1].clone());
                i += 2;
            }
            "--mqtt-node-id" => {
                if i + 1 >= args.len() {
                    usage_error("--mqtt-node-id requires a node ID value.");
                }

                parsed.mqtt_node_id = Some(args[i + 1].clone());
                i += 2;
            }
            "--mqtt-username" => {
                if i + 1 >= args.len() {
                    usage_error("--mqtt-username requires a username value.");
                }

                parsed.mqtt_username = Some(args[i + 1].clone());
                i += 2;
            }
            "--mqtt-password-env" => {
                if i + 1 >= args.len() {
                    usage_error("--mqtt-password-env requires an environment variable name.");
                }

                parsed.mqtt_password_env = Some(args[i + 1].clone());
                i += 2;
            }
            "--mqtt-ha-discovery" => {
                parsed.mqtt_ha_discovery = true;
                i += 1;
            }
            "--mqtt-ha-prefix" => {
                if i + 1 >= args.len() {
                    usage_error("--mqtt-ha-prefix requires a discovery prefix value.");
                }

                parsed.mqtt_ha_prefix = Some(args[i + 1].clone());
                i += 2;
            }
            "--mqtt-retain-discovery" => {
                parsed.mqtt_retain_discovery = true;
                i += 1;
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

    if parsed.sink == Some(SinkKind::Mqtt) {
        if !watch {
            usage_error("--sink mqtt is valid only with --watch for this experimental spike.");
        }
        if parsed
            .mqtt_host
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
        {
            usage_error("--sink mqtt requires --mqtt-host <host>.");
        }
        if parsed
            .mqtt_node_id
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
        {
            usage_error("--sink mqtt requires --mqtt-node-id <id>.");
        }
    }

    if parsed.mqtt_ha_discovery && parsed.sink != Some(SinkKind::Mqtt) {
        usage_error("--mqtt-ha-discovery is valid only with --sink mqtt.");
    }

    if let Err(e) = validate_mqtt_auth_flags(
        parsed.sink,
        parsed.mqtt_username.as_deref(),
        parsed.mqtt_password_env.as_deref(),
    ) {
        usage_error(&e);
    }

    if parsed.mqtt_ha_prefix.is_some() && !parsed.mqtt_ha_discovery {
        usage_error("--mqtt-ha-prefix requires --mqtt-ha-discovery.");
    }

    if parsed.mqtt_retain_discovery && !parsed.mqtt_ha_discovery {
        usage_error("--mqtt-retain-discovery requires --mqtt-ha-discovery.");
    }

    parsed
}

fn validate_mqtt_auth_flags(
    sink: Option<SinkKind>,
    username: Option<&str>,
    password_env: Option<&str>,
) -> Result<(), String> {
    if (username.is_some() || password_env.is_some()) && sink != Some(SinkKind::Mqtt) {
        return Err(
            "--mqtt-username and --mqtt-password-env are valid only with --sink mqtt.".to_string(),
        );
    }

    if username.is_some() != password_env.is_some() {
        return Err(
            "--mqtt-username and --mqtt-password-env must be provided together.".to_string(),
        );
    }

    Ok(())
}

fn mqtt_options_from_args(args: &CliArgs) -> Option<MqttOptions> {
    if args.sink != Some(SinkKind::Mqtt) {
        return None;
    }

    let port = match args.mqtt_port.as_deref() {
        Some(value) => value.parse::<u16>().unwrap_or_else(|_| {
            usage_error(&format!(
                "--mqtt-port must be a TCP port number. Got: {value}"
            ));
        }),
        None => DEFAULT_MQTT_PORT,
    };

    let topic_prefix = args
        .mqtt_topic_prefix
        .clone()
        .unwrap_or_else(|| DEFAULT_TOPIC_PREFIX.to_string());
    let auth = mqtt_auth_from_args(args);
    let ha_discovery = if args.mqtt_ha_discovery {
        let prefix = args
            .mqtt_ha_prefix
            .clone()
            .unwrap_or_else(|| DEFAULT_HA_DISCOVERY_PREFIX.to_string());
        Some(
            MqttHaDiscoveryOptions::new(prefix, args.mqtt_retain_discovery)
                .unwrap_or_else(|e| usage_error(&e)),
        )
    } else {
        None
    };

    Some(
        MqttOptions::new(
            args.mqtt_host.clone().unwrap_or_default(),
            port,
            topic_prefix,
            args.mqtt_node_id.clone().unwrap_or_default(),
            auth,
            ha_discovery,
        )
        .unwrap_or_else(|e| usage_error(&e)),
    )
}

fn mqtt_auth_from_args(args: &CliArgs) -> Option<MqttAuthOptions> {
    match (
        args.mqtt_username.as_deref(),
        args.mqtt_password_env.as_deref(),
    ) {
        (Some(username), Some(password_env)) => Some(
            resolve_mqtt_auth(username, password_env, |name| env::var(name).ok())
                .unwrap_or_else(|e| usage_error(&e)),
        ),
        _ => None,
    }
}

fn resolve_mqtt_auth<F>(
    username: &str,
    password_env: &str,
    get_env: F,
) -> Result<MqttAuthOptions, String>
where
    F: FnOnce(&str) -> Option<String>,
{
    let password_env = password_env.trim();
    if password_env.is_empty() {
        return Err("--mqtt-password-env must not be empty.".to_string());
    }

    let password = get_env(password_env).ok_or_else(|| {
        format!("--mqtt-password-env variable {password_env} is not set or is not valid Unicode.")
    })?;
    if password.is_empty() {
        return Err(format!(
            "--mqtt-password-env variable {password_env} must not be empty."
        ));
    }

    MqttAuthOptions::new(username.to_string(), password)
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
        Some(kind @ (SinkKind::Csv | SinkKind::Jsonl)) => match Sink::new(kind) {
            Ok(sink) => {
                eprintln!("WTG note: sink enabled: {}", sink.filename());
                Some(sink)
            }
            Err(e) => {
                eprintln!("WTG runtime error: failed to create sink output file: {e}");
                process::exit(2);
            }
        },
        Some(SinkKind::Mqtt) | None => None,
    };

    let mut mqtt_sink = match mqtt_options_from_args(&args) {
        Some(options) => match MqttSink::connect(options) {
            Ok(sink) => {
                eprintln!("WTG note: MQTT sink enabled.");
                Some(sink)
            }
            Err(e) => {
                eprintln!("WTG MQTT error: {e}");
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
                            SinkKind::Mqtt => {}
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
                                SinkKind::Mqtt => {}
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
            let mut mqtt_ha_discovery_published = false;
            loop {
                match wtg_core::nvml::snapshot_all_with_ctx(&ctx) {
                    Ok(snapshots) => {
                        let tick_ts = now_ts();
                        print_and_mirror_jsonl(&format!("tick.seq: {tick_seq}\n"), &sink);
                        print_and_mirror_jsonl(&format!("tick.ts: {tick_ts}\n"), &sink);
                        for s in snapshots.iter() {
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
                        if let Some(mqtt_sink) = mqtt_sink.as_mut() {
                            if !mqtt_ha_discovery_published {
                                if let Err(e) =
                                    mqtt_sink.publish_ha_discovery_for_snapshots(&snapshots)
                                {
                                    eprintln!("WTG MQTT error: {e}");
                                    process::exit(2);
                                }
                                mqtt_ha_discovery_published = true;
                            }
                            if let Err(e) =
                                mqtt_sink.publish_snapshots(&ctx, &snapshots, tick_seq, &tick_ts)
                            {
                                eprintln!("WTG MQTT error: {e}");
                                process::exit(2);
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
            let mqtt_ctx = if mqtt_sink.is_some() {
                Some(match wtg_core::nvml::init_context() {
                    Ok(ctx) => ctx,
                    Err(e) => {
                        eprintln!("WTG --watch init failed: {e}");
                        process::exit(2);
                    }
                })
            } else {
                None
            };

            let mut tick_seq: u64 = 0;
            let mut mqtt_ha_discovery_published = false;
            loop {
                let snapshot_result = match mqtt_ctx.as_ref() {
                    Some(ctx) => wtg_core::nvml::snapshot_all_with_ctx(ctx),
                    None => wtg_core::nvml::snapshot_all(),
                };

                match snapshot_result {
                    Ok(snapshots) => {
                        let tick_ts = now_ts();
                        let tick_line = format!("--- tick {tick_ts} ---");
                        println!("{tick_line}");
                        for s in snapshots.iter() {
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
                                    SinkKind::Mqtt => {}
                                }
                            }
                        }
                        if let (Some(mqtt_sink), Some(ctx)) =
                            (mqtt_sink.as_mut(), mqtt_ctx.as_ref())
                        {
                            if !mqtt_ha_discovery_published {
                                if let Err(e) =
                                    mqtt_sink.publish_ha_discovery_for_snapshots(&snapshots)
                                {
                                    eprintln!("WTG MQTT error: {e}");
                                    process::exit(2);
                                }
                                mqtt_ha_discovery_published = true;
                            }
                            if let Err(e) =
                                mqtt_sink.publish_snapshots(ctx, &snapshots, tick_seq, &tick_ts)
                            {
                                eprintln!("WTG MQTT error: {e}");
                                process::exit(2);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mqtt_auth_flags_require_mqtt_sink() {
        let err =
            validate_mqtt_auth_flags(None, Some("user"), Some("WTG_MQTT_PASSWORD")).unwrap_err();

        assert!(err.contains("valid only with --sink mqtt"));
    }

    #[test]
    fn mqtt_auth_flags_reject_password_without_username() {
        let err = validate_mqtt_auth_flags(Some(SinkKind::Mqtt), None, Some("WTG_MQTT_PASSWORD"))
            .unwrap_err();

        assert!(err.contains("must be provided together"));
    }

    #[test]
    fn mqtt_auth_flags_reject_username_without_password_env() {
        let err = validate_mqtt_auth_flags(Some(SinkKind::Mqtt), Some("user"), None).unwrap_err();

        assert!(err.contains("must be provided together"));
    }

    #[test]
    fn mqtt_auth_resolution_rejects_missing_password_env_var() {
        let err = resolve_mqtt_auth("user", "WTG_MQTT_PASSWORD", |_| None).unwrap_err();

        assert!(err.contains("WTG_MQTT_PASSWORD"));
        assert!(err.contains("not set"));
    }

    #[test]
    fn mqtt_auth_resolution_rejects_empty_password_env_var() {
        let err =
            resolve_mqtt_auth("user", "WTG_MQTT_PASSWORD", |_| Some(String::new())).unwrap_err();

        assert!(err.contains("WTG_MQTT_PASSWORD"));
        assert!(err.contains("must not be empty"));
    }

    #[test]
    fn mqtt_auth_resolution_accepts_username_and_password_env_var() {
        let password = String::from_utf8(vec![115, 101, 99, 114, 101, 116]).unwrap();

        assert!(resolve_mqtt_auth("user", "WTG_MQTT_PASSWORD", |_| Some(password)).is_ok());
    }
}
