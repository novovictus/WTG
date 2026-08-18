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
use std::path::Path;
use std::process;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tracing::info;

mod cli_mqtt;
mod config;
mod mqtt;
mod mqtt_settings;
mod nvml_provenance;
mod probe;
mod probe_fields;
mod sink;

use mqtt::{MqttOptions, MqttSink};
use probe::{format_probe_csv_header, format_probe_csv_row, format_probe_record, ProbeRecord};
use probe_fields::{
    format_field_value, format_probe_fields_csv_header, format_probe_fields_csv_row,
    format_probe_fields_snapshot,
};
use sink::{Sink, SinkKind};
use wtg_providers::{amd_adl, amd_adlx, intel_level_zero};

/// Default sampling interval when `--watch` is enabled.
/// 1000ms is conservative and matches NVML’s practical update cadence for many metrics.
const DEFAULT_INTERVAL_MS: u64 = 1000;
const NVML_ONCE_TIMEOUT_MS: u64 = 5000;

/// Stats output schema version.
/// This lets us evolve the key set while remaining explicit in artifacts.
const STATS_SCHEMA: u32 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProviderKind {
    Amd,
    Intel,
}

struct CliArgs {
    once: bool,
    watch: bool,
    probe: bool,
    probe_fields: bool,
    stats: bool,
    help: bool,
    version: bool,
    provider: Option<ProviderKind>,
    config_path: Option<String>,
    interval_ms: Option<u64>,
    sink: Option<SinkKind>,
    mqtt_host: Option<String>,
    mqtt_port: Option<String>,
    mqtt_topic_prefix: Option<String>,
    mqtt_node_id: Option<String>,
    mqtt_username: Option<String>,
    mqtt_password: Option<String>,
    mqtt_password_env: Option<String>,
    mqtt_ha_discovery: bool,
    mqtt_ha_discovery_from_cli: bool,
    mqtt_ha_prefix: Option<String>,
    mqtt_ha_remove_discovery: bool,
    mqtt_init_config: bool,
    mqtt_save_config: bool,
    force_config: bool,
    mqtt_enabled_from_config: bool,
    mqtt_retain_discovery: bool,
    field_ids: Vec<u32>,
}

impl Default for CliArgs {
    fn default() -> Self {
        Self {
            once: false,
            watch: false,
            probe: false,
            probe_fields: false,
            stats: false,
            help: false,
            version: false,
            provider: None,
            config_path: None,
            interval_ms: None,
            sink: None,
            mqtt_host: None,
            mqtt_port: None,
            mqtt_topic_prefix: None,
            mqtt_node_id: None,
            mqtt_username: None,
            mqtt_password: None,
            mqtt_password_env: None,
            mqtt_ha_discovery: false,
            mqtt_ha_discovery_from_cli: false,
            mqtt_ha_prefix: None,
            mqtt_ha_remove_discovery: false,
            mqtt_init_config: false,
            mqtt_save_config: false,
            force_config: false,
            mqtt_enabled_from_config: false,
            mqtt_retain_discovery: false,
            field_ids: Vec::new(),
        }
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

fn print_provider_status_block(status: &str, reason: &str) {
    println!("Provider status: {status}");
    println!("Reason: {reason}");
}

fn print_nvml_failed_device_blocks(report: &wtg_core::nvml::NvmlSnapshotReport) {
    for sample in report.device_results.iter() {
        if let Err(reason) = &sample.result {
            println!();
            println!("NVML device {}: unavailable", sample.index);
            println!("  Reason: {reason}");
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
        "  wtg.exe --once [--stats] [--provider amd|intel] [--sink jsonl|csv]\n",
        "  wtg.exe --watch [--interval <ms>] [--stats] [--provider amd|intel] [--sink jsonl|csv|mqtt]\n",
        "  wtg.exe --probe [--provider amd|intel] [--sink jsonl|csv]\n",
        "  wtg.exe --probe-fields --field-id <u32> [--field-id <u32> ...] [--sink jsonl|csv]\n",
        "  wtg.exe --sink mqtt --mqtt-ha-remove-discovery --mqtt-host <host> --mqtt-node-id <id>\n",
        "\n",
        "Options:\n",
        "  --once                  Capture a single GPU snapshot and exit.\n",
        "  --watch                 Continuously poll GPU state.\n",
        "  --config <path>         Load explicit WTG TOML config.\n",
        "  --interval <ms>         Polling interval in milliseconds for --watch.\n",
        "  --stats                 Print stable key:value stats output for --once or --watch.\n",
        "  --provider amd|intel    Use an experimental provider for --once, --watch, or --probe.\n",
        "  --probe                 Capture one context-rich probe block.\n",
        "  --probe-fields          Query explicit NVML field-value IDs.\n",
        "  --field-id <u32>        Repeatable field ID for --probe-fields.\n",
        "  --sink jsonl|csv|mqtt   Select an output sink. jsonl/csv write timestamped files; mqtt publishes during --watch.\n",
        "  --mqtt-host <host>      MQTT broker host. Required with --sink mqtt.\n",
        "  --mqtt-port <port>      MQTT broker port. Default: 1883.\n",
        "  --mqtt-topic-prefix <p> MQTT topic prefix. Default: wtg.\n",
        "  --mqtt-node-id <id>     MQTT node ID. Required with --sink mqtt.\n",
        "  --mqtt-username <user>  MQTT username. Requires --mqtt-password or --mqtt-password-env.\n",
        "  --mqtt-password <pwd>   MQTT password. Requires --mqtt-username. Convenient for trusted local use.\n",
        "  --mqtt-password-env <v> Read MQTT password from environment variable. Requires --mqtt-username.\n",
        "  --mqtt-ha-discovery     Publish Home Assistant MQTT discovery configs with --sink mqtt.\n",
        "  --mqtt-ha-prefix <p>    Home Assistant discovery prefix. Default: homeassistant.\n",
        "  --mqtt-ha-remove-discovery Remove retained Home Assistant discovery configs and availability.\n",
        "  --mqtt-init-config      Create a template wtg.toml in the current directory.\n",
        "  --mqtt-save-config      Write wtg.toml from explicit MQTT CLI flags and exit.\n",
        "  --force-config          Overwrite existing wtg.toml when used with --mqtt-save-config.\n",
        "  --mqtt-retain-discovery Retain Home Assistant discovery configs; accepted with cleanup.\n",
        "  --help / -h             Print this help text.\n",
        "  --version / -V          Print version and exit.\n"
    ));
}

fn print_version() {
    println!("WTG - WhatTheGPU v{}", env!("CARGO_PKG_VERSION"));
}

fn print_product_header() {
    println!("WTG - WhatTheGPU v{}", env!("CARGO_PKG_VERSION"));
    println!("Honest GPU compute stats for Windows");
    println!();
}

fn provider_display_name(provider: Option<ProviderKind>) -> &'static str {
    match provider {
        Some(ProviderKind::Amd) => "AMD ADL",
        Some(ProviderKind::Intel) => "Intel Level Zero",
        None => "NVIDIA NVML",
    }
}

fn aggregate_provider_status(left: &str, right: &str) -> &'static str {
    if left == "ok" || right == "ok" {
        "ok"
    } else if left == "error" || right == "error" {
        "error"
    } else if left == "unavailable" && right == "unavailable" {
        "unavailable"
    } else {
        "error"
    }
}

fn print_mode_header(mode: &str, provider: Option<ProviderKind>, interval_ms: Option<u64>) {
    match interval_ms {
        Some(interval_ms) => println!(
            "WTG {mode} mode (provider: {}) - interval {interval_ms} ms",
            provider_display_name(provider)
        ),
        None => println!(
            "WTG {mode} mode (provider: {})",
            provider_display_name(provider)
        ),
    }
}

fn run_amd_provider(args: &CliArgs) -> ! {
    if args.probe {
        let sample = amd_adl::collect_once(0);
        println!("{}", amd_adl::format_probe_snapshot(&sample));
        process::exit(wtg_core::exit_code_for_status(amd_adl::sample_status(
            &sample,
        )));
    }

    if args.once && args.stats {
        let sample = amd_adl::collect_once(0);
        let tick_ts = now_ts();
        println!(
            "{}",
            amd_adl::format_stats_snapshot_json(&sample, 0, &tick_ts)
        );
        process::exit(wtg_core::exit_code_for_status(amd_adl::sample_status(
            &sample,
        )));
    }

    if args.watch && args.stats {
        let interval_ms = args.interval_ms.unwrap_or(DEFAULT_INTERVAL_MS);
        if interval_ms < 100 {
            eprintln!("WTG note: very low interval ({interval_ms}ms). ADL metrics may not update this quickly; expect duplicates.");
        }

        let sleep_dur = Duration::from_millis(interval_ms);
        let mut sample_seq = 0u64;
        loop {
            let sample = amd_adl::collect_once(sample_seq);
            let tick_ts = now_ts();
            println!(
                "{}",
                amd_adl::format_stats_snapshot_json(&sample, sample_seq, &tick_ts)
            );
            println!();
            sample_seq = sample_seq.saturating_add(1);
            thread::sleep(sleep_dur);
        }
    }

    print_product_header();
    if args.once {
        let sample = amd_adl::collect_once(0);
        let adlx_sample = amd_adlx::collect_once(0);
        print_mode_header("snapshot", args.provider, None);
        println!("Provider source: {}", amd_adl::provider_source());
        println!("Telemetry class: {}", amd_adl::telemetry_class());
        println!();
        println!("{}", amd_adl::format_snapshot(&sample));
        println!();
        println!("Provider source: {}", amd_adlx::provider_source());
        println!("Telemetry class: {}", amd_adlx::telemetry_class());
        println!();
        println!("{}", amd_adlx::format_snapshot(&adlx_sample));
        process::exit(wtg_core::exit_code_for_status(aggregate_provider_status(
            amd_adl::sample_status(&sample),
            amd_adlx::sample_status(&adlx_sample),
        )));
    }

    let interval_ms = args.interval_ms.unwrap_or(DEFAULT_INTERVAL_MS);
    if interval_ms < 100 {
        eprintln!("WTG note: very low interval ({interval_ms}ms). ADL metrics may not update this quickly; expect duplicates.");
    }

    print_mode_header("watch", args.provider, Some(interval_ms));
    println!("Provider source: {}", amd_adl::provider_source());
    println!("Telemetry class: {}", amd_adl::telemetry_class());
    println!();

    let sleep_dur = Duration::from_millis(interval_ms);
    let mut sample_seq = 0u64;
    loop {
        let sample = amd_adl::collect_once(sample_seq);
        println!("--- tick {} ---", now_ts());
        println!("{}", amd_adl::format_watch_sample(&sample));
        println!();
        sample_seq = sample_seq.saturating_add(1);
        thread::sleep(sleep_dur);
    }
}

fn run_intel_provider(args: &CliArgs) -> ! {
    if args.probe {
        let sample = intel_level_zero::collect_once(0);
        println!("{}", intel_level_zero::format_probe_snapshot(&sample));
        process::exit(wtg_core::exit_code_for_status(
            intel_level_zero::sample_status(&sample),
        ));
    }

    if args.once && args.stats {
        let sample = intel_level_zero::collect_visible_sample(0);
        let tick_ts = now_ts();
        println!(
            "{}",
            intel_level_zero::format_stats_snapshot_json(&sample, 0, &tick_ts)
        );
        process::exit(wtg_core::exit_code_for_status(
            intel_level_zero::sample_status(&sample),
        ));
    }

    if args.watch && args.stats {
        let interval_ms = args.interval_ms.unwrap_or(DEFAULT_INTERVAL_MS);
        if interval_ms < 100 {
            eprintln!("WTG note: very low interval ({interval_ms}ms). Intel Level Zero metrics may not update this quickly; expect duplicates.");
        }

        let sleep_dur = Duration::from_millis(interval_ms);
        let mut sample_seq = 0u64;
        let first_sample = intel_level_zero::collect_visible_sample(sample_seq);
        let first_tick_ts = now_ts();
        println!(
            "{}",
            intel_level_zero::format_stats_snapshot_json(&first_sample, sample_seq, &first_tick_ts)
        );
        println!();
        sample_seq = sample_seq.saturating_add(1);
        thread::sleep(sleep_dur);

        loop {
            let sample = intel_level_zero::collect_once(sample_seq);
            let tick_ts = now_ts();
            println!(
                "{}",
                intel_level_zero::format_stats_snapshot_json(&sample, sample_seq, &tick_ts)
            );
            println!();
            sample_seq = sample_seq.saturating_add(1);
            thread::sleep(sleep_dur);
        }
    }

    print_product_header();
    if args.once {
        let sample = intel_level_zero::collect_visible_sample(0);
        print_mode_header("snapshot", args.provider, None);
        println!("Provider source: {}", intel_level_zero::provider_source());
        println!("Telemetry class: {}", intel_level_zero::telemetry_class());
        println!();
        println!("{}", intel_level_zero::format_snapshot(&sample));
        process::exit(wtg_core::exit_code_for_status(
            intel_level_zero::sample_status(&sample),
        ));
    }

    let interval_ms = args.interval_ms.unwrap_or(DEFAULT_INTERVAL_MS);
    if interval_ms < 100 {
        eprintln!("WTG note: very low interval ({interval_ms}ms). Intel Level Zero metrics may not update this quickly; expect duplicates.");
    }

    print_mode_header("watch", args.provider, Some(interval_ms));
    println!("Provider source: {}", intel_level_zero::provider_source());
    println!("Telemetry class: {}", intel_level_zero::telemetry_class());
    println!();

    let sleep_dur = Duration::from_millis(interval_ms);
    let mut sample_seq = 0u64;
    let first_sample = intel_level_zero::collect_visible_sample(sample_seq);
    println!("--- tick {} ---", now_ts());
    println!("{}", intel_level_zero::format_watch_sample(&first_sample));
    println!();
    sample_seq = sample_seq.saturating_add(1);
    thread::sleep(sleep_dur);

    loop {
        let sample = intel_level_zero::collect_once(sample_seq);
        println!("--- tick {} ---", now_ts());
        println!("{}", intel_level_zero::format_watch_sample(&sample));
        println!();
        sample_seq = sample_seq.saturating_add(1);
        thread::sleep(sleep_dur);
    }
}

fn usage_error(message: &str) -> ! {
    eprintln!("WTG usage error: {message}");
    process::exit(1);
}

fn parse_args() -> CliArgs {
    let args: Vec<String> = env::args().collect();

    let mut parsed = CliArgs::default();

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
            "--provider" => {
                if i + 1 >= args.len() {
                    usage_error("--provider requires a value. Supported: amd, intel.");
                }

                parsed.provider = Some(match args[i + 1].as_str() {
                    "amd" => ProviderKind::Amd,
                    "intel" => ProviderKind::Intel,
                    other => usage_error(&format!(
                        "--provider value must be amd or intel. Got: {other}"
                    )),
                });
                i += 2;
            }
            "--config" => {
                if i + 1 >= args.len() {
                    usage_error("--config requires a TOML file path.");
                }

                parsed.config_path = Some(args[i + 1].clone());
                i += 2;
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
            "--mqtt-password" => {
                if i + 1 >= args.len() {
                    usage_error("--mqtt-password requires a password value.");
                }

                parsed.mqtt_password = Some(args[i + 1].clone());
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
                parsed.mqtt_ha_discovery_from_cli = true;
                i += 1;
            }
            "--mqtt-ha-prefix" => {
                if i + 1 >= args.len() {
                    usage_error("--mqtt-ha-prefix requires a discovery prefix value.");
                }

                parsed.mqtt_ha_prefix = Some(args[i + 1].clone());
                i += 2;
            }
            "--mqtt-ha-remove-discovery" => {
                parsed.mqtt_ha_remove_discovery = true;
                i += 1;
            }
            "--mqtt-init-config" => {
                parsed.mqtt_init_config = true;
                i += 1;
            }
            "--mqtt-save-config" => {
                parsed.mqtt_save_config = true;
                i += 1;
            }
            "--force-config" => {
                parsed.force_config = true;
                i += 1;
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

    if parsed.force_config && !parsed.mqtt_save_config {
        usage_error("--force-config is valid only with --mqtt-save-config.");
    }

    if parsed.mqtt_init_config || parsed.mqtt_save_config {
        if parsed.mqtt_save_config {
            if let Err(e) = validate_save_config_args(&parsed) {
                usage_error(&e);
            }
        }
        return parsed;
    }

    if let Some(config_path) = parsed.config_path.clone() {
        let config = config::load_config_file(Path::new(&config_path)).unwrap_or_else(|e| {
            usage_error(&e);
        });
        apply_config(&mut parsed, &config);
    }

    validate_args(&parsed);

    parsed
}

fn apply_config(parsed: &mut CliArgs, config: &config::WtgConfig) {
    let Some(mqtt) = config.mqtt.as_ref() else {
        return;
    };

    let mqtt_config_enabled = mqtt.enabled();
    if mqtt_config_enabled && parsed.sink.is_none() {
        parsed.mqtt_enabled_from_config = true;
    }

    let apply_mqtt_values = mqtt_config_enabled
        || parsed.sink == Some(SinkKind::Mqtt)
        || parsed.mqtt_ha_remove_discovery;
    if !apply_mqtt_values {
        return;
    }

    if parsed.mqtt_host.is_none() {
        if let Some(value) = mqtt.host() {
            parsed.mqtt_host = Some(value.to_string());
        }
    }
    if parsed.mqtt_port.is_none() {
        if let Some(value) = mqtt.port {
            parsed.mqtt_port = Some(value.to_string());
        }
    }
    if parsed.mqtt_username.is_none() {
        if let Some(value) = mqtt.username() {
            parsed.mqtt_username = Some(value.to_string());
        }
    }
    if parsed.mqtt_password.is_none() {
        if let Some(value) = mqtt.password() {
            parsed.mqtt_password = Some(value.to_string());
        }
    }
    if parsed.mqtt_password_env.is_none() {
        if let Some(value) = mqtt.password_env() {
            parsed.mqtt_password_env = Some(value.to_string());
        }
    }
    if parsed.mqtt_topic_prefix.is_none() {
        if let Some(value) = mqtt.topic_prefix() {
            parsed.mqtt_topic_prefix = Some(value.to_string());
        }
    }
    if parsed.mqtt_node_id.is_none() {
        if let Some(value) = mqtt.node_id() {
            parsed.mqtt_node_id = Some(value.to_string());
        }
    }

    if let Some(ha) = mqtt.home_assistant.as_ref() {
        let config_ha_discovery = ha.discovery.unwrap_or(false);
        if config_ha_discovery && !parsed.mqtt_ha_remove_discovery {
            parsed.mqtt_ha_discovery = true;
        }
        if parsed.mqtt_ha_prefix.is_none()
            && (parsed.mqtt_ha_discovery || parsed.mqtt_ha_remove_discovery || config_ha_discovery)
        {
            if let Some(value) = ha.discovery_prefix() {
                parsed.mqtt_ha_prefix = Some(value.to_string());
            }
        }
        if (parsed.mqtt_ha_discovery || config_ha_discovery) && ha.retain_discovery.unwrap_or(false)
        {
            parsed.mqtt_retain_discovery = true;
        }
    }
}

fn mqtt_is_active(args: &CliArgs) -> bool {
    args.sink == Some(SinkKind::Mqtt) || args.mqtt_enabled_from_config
}

fn validate_args(parsed: &CliArgs) {
    if let Err(e) = validate_args_result(parsed) {
        usage_error(&e);
    }
}

fn validate_args_result(parsed: &CliArgs) -> Result<(), String> {
    let once = parsed.once;
    let watch = parsed.watch;
    let probe = parsed.probe;
    let probe_fields = parsed.probe_fields;
    let stats = parsed.stats;
    let mqtt_active = mqtt_is_active(parsed);

    if once && watch {
        return Err("--once and --watch are mutually exclusive.".to_string());
    }

    if probe && (once || watch) {
        return Err("--probe is mutually exclusive with --once and --watch.".to_string());
    }

    if probe_fields && (once || watch || probe) {
        return Err(
            "--probe-fields is mutually exclusive with --once, --watch, and --probe.".to_string(),
        );
    }

    if stats && !once && !watch {
        return Err("--stats requires --once or --watch.".to_string());
    }

    if parsed.provider == Some(ProviderKind::Amd) {
        if probe_fields {
            return Err("--provider amd does not support --probe-fields.".to_string());
        }
        if parsed.sink.is_some() {
            return Err("--provider amd does not support --sink.".to_string());
        }
        if parsed.mqtt_ha_remove_discovery {
            return Err("--provider amd does not support --mqtt-ha-remove-discovery.".to_string());
        }
    }

    if parsed.provider == Some(ProviderKind::Intel) {
        if probe_fields {
            return Err("--provider intel does not support --probe-fields.".to_string());
        }
        if parsed.sink.is_some() {
            return Err("--provider intel does not support --sink.".to_string());
        }
        if parsed.mqtt_ha_remove_discovery {
            return Err(
                "--provider intel does not support --mqtt-ha-remove-discovery.".to_string(),
            );
        }
    }

    if parsed.provider.is_some() && !once && !watch && !probe {
        return Err("--provider is valid only with --once, --watch, or --probe.".to_string());
    }

    if parsed.mqtt_ha_remove_discovery && (once || watch || probe || probe_fields || stats) {
        return Err("--mqtt-ha-remove-discovery cannot be combined with --once, --watch, --stats, --probe, or --probe-fields.".to_string());
    }

    if parsed.interval_ms.is_some() && !watch {
        return Err("--interval is valid only with --watch.".to_string());
    }

    if !probe_fields && !parsed.field_ids.is_empty() {
        return Err("--field-id requires --probe-fields.".to_string());
    }

    if probe_fields && parsed.field_ids.is_empty() {
        return Err("--probe-fields requires at least one --field-id <u32>.".to_string());
    }

    if parsed.sink.is_some()
        && !once
        && !watch
        && !probe
        && !probe_fields
        && !parsed.mqtt_ha_remove_discovery
    {
        return Err("--sink requires --once, --watch, --probe, or --probe-fields.".to_string());
    }

    if parsed.mqtt_enabled_from_config && !watch {
        return Err("[mqtt].enabled = true in --config requires --watch.".to_string());
    }

    if parsed.mqtt_ha_remove_discovery && parsed.mqtt_ha_discovery_from_cli {
        return Err(
            "--mqtt-ha-remove-discovery cannot be combined with --mqtt-ha-discovery.".to_string(),
        );
    }

    if mqtt_active {
        if !watch && !parsed.mqtt_ha_remove_discovery {
            return Err(
                "--sink mqtt is valid only with --watch, except for --mqtt-ha-remove-discovery."
                    .to_string(),
            );
        }
        if parsed
            .mqtt_host
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
        {
            return Err("--sink mqtt requires --mqtt-host <host>.".to_string());
        }
        if parsed
            .mqtt_node_id
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
        {
            return Err("--sink mqtt requires --mqtt-node-id <id>.".to_string());
        }
    }

    if parsed.mqtt_ha_discovery && !mqtt_active {
        return Err("--mqtt-ha-discovery is valid only with active MQTT.".to_string());
    }

    if parsed.mqtt_ha_remove_discovery && parsed.sink != Some(SinkKind::Mqtt) {
        return Err("--mqtt-ha-remove-discovery is valid only with --sink mqtt.".to_string());
    }

    if let Err(e) = validate_mqtt_auth_combination(
        parsed.mqtt_username.as_deref(),
        parsed.mqtt_password.as_deref(),
        parsed.mqtt_password_env.as_deref(),
    ) {
        return Err(e);
    }

    let has_auth = parsed.mqtt_username.is_some()
        || parsed.mqtt_password.is_some()
        || parsed.mqtt_password_env.is_some();
    if has_auth && !mqtt_active {
        return Err("--mqtt-username, --mqtt-password, and --mqtt-password-env are valid only with active MQTT.".to_string());
    }

    if parsed.mqtt_ha_prefix.is_some()
        && !parsed.mqtt_ha_discovery
        && !parsed.mqtt_ha_remove_discovery
    {
        return Err(
            "--mqtt-ha-prefix requires --mqtt-ha-discovery or --mqtt-ha-remove-discovery."
                .to_string(),
        );
    }

    if parsed.mqtt_retain_discovery && !parsed.mqtt_ha_discovery && !parsed.mqtt_ha_remove_discovery
    {
        return Err(
            "--mqtt-retain-discovery requires --mqtt-ha-discovery or --mqtt-ha-remove-discovery."
                .to_string(),
        );
    }

    Ok(())
}

fn validate_save_config_args(parsed: &CliArgs) -> Result<(), String> {
    if parsed.force_config && !parsed.mqtt_save_config {
        return Err("--force-config is valid only with --mqtt-save-config.".to_string());
    }

    if parsed.mqtt_init_config {
        return Err("--mqtt-save-config cannot be combined with --mqtt-init-config.".to_string());
    }

    if parsed.once || parsed.watch || parsed.probe || parsed.probe_fields || parsed.stats {
        return Err("--mqtt-save-config cannot be combined with runtime modes.".to_string());
    }

    if parsed.sink.is_some() || parsed.mqtt_ha_remove_discovery {
        return Err(
            "--mqtt-save-config cannot be combined with --sink or --mqtt-ha-remove-discovery."
                .to_string(),
        );
    }

    if parsed.config_path.is_some() {
        return Err("--mqtt-save-config does not load an existing config file.".to_string());
    }

    if parsed
        .mqtt_host
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        return Err("--mqtt-save-config requires --mqtt-host <host>.".to_string());
    }

    if parsed
        .mqtt_node_id
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        return Err("--mqtt-save-config requires --mqtt-node-id <id>.".to_string());
    }

    validate_mqtt_auth_combination(
        parsed.mqtt_username.as_deref(),
        parsed.mqtt_password.as_deref(),
        parsed.mqtt_password_env.as_deref(),
    )?;

    if parsed.mqtt_ha_prefix.is_some() && !parsed.mqtt_ha_discovery {
        return Err("--mqtt-ha-prefix requires --mqtt-ha-discovery.".to_string());
    }

    if parsed.mqtt_retain_discovery && !parsed.mqtt_ha_discovery {
        return Err("--mqtt-retain-discovery requires --mqtt-ha-discovery.".to_string());
    }

    Ok(())
}

fn saved_mqtt_config_from_args(args: &CliArgs) -> config::SavedMqttConfig {
    mqtt_settings::saved_mqtt_config_from_values(
        args.mqtt_host.as_deref(),
        args.mqtt_port.as_deref(),
        args.mqtt_username.as_deref(),
        args.mqtt_password.as_deref(),
        args.mqtt_password_env.as_deref(),
        args.mqtt_topic_prefix.as_deref(),
        args.mqtt_node_id.as_deref(),
        args.mqtt_ha_discovery,
        args.mqtt_ha_prefix.as_deref(),
        args.mqtt_retain_discovery,
    )
    .unwrap_or_else(|e| usage_error(&e))
}

fn validate_mqtt_auth_combination(
    username: Option<&str>,
    password: Option<&str>,
    password_env: Option<&str>,
) -> Result<(), String> {
    mqtt_settings::validate_mqtt_auth_combination(username, password, password_env)
}

fn mqtt_options_from_args(args: &CliArgs) -> Option<MqttOptions> {
    mqtt_settings::mqtt_options_from_values(
        mqtt_is_active(args),
        args.mqtt_host.as_deref(),
        args.mqtt_port.as_deref(),
        args.mqtt_topic_prefix.as_deref(),
        args.mqtt_node_id.as_deref(),
        args.mqtt_username.as_deref(),
        args.mqtt_password.as_deref(),
        args.mqtt_password_env.as_deref(),
        args.mqtt_ha_discovery,
        args.mqtt_ha_remove_discovery,
        args.mqtt_ha_prefix.as_deref(),
        args.mqtt_retain_discovery,
    )
    .unwrap_or_else(|e| usage_error(&e))
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

    if args.mqtt_init_config {
        match config::create_default_config_file() {
            Ok(path) => {
                eprintln!("WTG note: created {}", path.display());
            }
            Err(e) => {
                eprintln!("WTG config error: {e}");
                process::exit(1);
            }
        }
        return;
    }

    if args.mqtt_save_config {
        let saved = saved_mqtt_config_from_args(&args);
        match config::write_config_file(
            &saved,
            Path::new(config::DEFAULT_CONFIG_FILE_NAME),
            args.force_config,
        ) {
            Ok(path) => {
                eprintln!("WTG note: saved {}", path.display());
            }
            Err(e) => {
                eprintln!("WTG config error: {e}");
                process::exit(1);
            }
        }
        return;
    }

    // Initialize logging early. This is safe in all modes and helps diagnostics on Windows.
    tracing_subscriber::fmt::init();

    if args.provider == Some(ProviderKind::Amd) {
        run_amd_provider(&args);
    }
    if args.provider == Some(ProviderKind::Intel) {
        run_intel_provider(&args);
    }

    let sink = match args.sink {
        Some(kind @ (SinkKind::Csv | SinkKind::Jsonl)) => match Sink::new(kind) {
            Ok(sink) => {
                eprintln!("WTG note: sink enabled: {}", sink.filename());
                Some(sink)
            }
            Err(e) => {
                eprintln!("WTG runtime error: failed to create sink output file: {e}");
                process::exit(wtg_core::exit_code_for_status("error"));
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
                process::exit(wtg_core::exit_code_for_status("error"));
            }
        },
        None => None,
    };

    if args.mqtt_ha_remove_discovery {
        let ctx = match wtg_core::nvml::init_context() {
            Ok(ctx) => ctx,
            Err(e) => {
                eprintln!("WTG MQTT cleanup init failed: {e}");
                process::exit(wtg_core::exit_code_for_status("unavailable"));
            }
        };
        let snapshots = match wtg_core::nvml::snapshot_all_with_ctx(&ctx) {
            Ok(snapshots) => snapshots,
            Err(e) => {
                eprintln!("WTG MQTT cleanup snapshot failed: {e}");
                let status = if e == "all NVIDIA device samples failed" {
                    "error"
                } else {
                    "unavailable"
                };
                process::exit(wtg_core::exit_code_for_status(status));
            }
        };
        let Some(mqtt_sink) = mqtt_sink.as_mut() else {
            eprintln!("WTG MQTT cleanup error: --mqtt-ha-remove-discovery requires --sink mqtt.");
            process::exit(1);
        };
        if let Err(e) = mqtt_sink.publish_ha_discovery_cleanup_for_snapshots(&snapshots) {
            eprintln!("WTG MQTT cleanup error: {e}");
            process::exit(wtg_core::exit_code_for_status("error"));
        }

        eprintln!("WTG note: MQTT Home Assistant discovery cleanup published.");
        return;
    }

    if args.once && args.stats {
        let probe_context_ctx = match wtg_core::nvml::init_context() {
            Ok(ctx) => ctx,
            Err(e) => {
                eprintln!("WTG --once --stats init failed: {e}");
                process::exit(wtg_core::exit_code_for_status("unavailable"));
            }
        };
        let snaps = match wtg_core::nvml::snapshot_all_with_ctx(&probe_context_ctx) {
            Ok(snaps) => snaps,
            Err(e) => {
                eprintln!("WTG --once --stats failed: {e}");
                let status = if e == "all NVIDIA device samples failed" {
                    "error"
                } else {
                    "unavailable"
                };
                process::exit(wtg_core::exit_code_for_status(status));
            }
        };
        let tick_seq = 0;
        let tick_ts = now_ts();
        let contexts = snaps
            .iter()
            .map(|snapshot| {
                wtg_core::nvml::probe_context::query_probe_context_for_gpu_with_ctx(
                    &probe_context_ctx,
                    snapshot.index,
                )
            })
            .collect::<Vec<_>>();
        let provenance_pretty = nvml_provenance::format_nvml_provenance_stats_pretty(
            &snaps,
            &contexts,
            &probe_context_ctx,
            tick_seq,
            &tick_ts,
        );

        // 0.2.7: console/jsonl stats use NVML provenance v1; CSV/watch stats remain legacy for now.
        println!("{provenance_pretty}");

        if let Some(sink) = &sink {
            match sink.kind() {
                SinkKind::Jsonl => {
                    sink.emit_raw_line(&nvml_provenance::format_nvml_provenance_stats_jsonl(
                        &snaps,
                        &contexts,
                        &probe_context_ctx,
                        tick_seq,
                        &tick_ts,
                    ))
                }
                SinkKind::Csv => {
                    sink.emit_raw_line(format_stats_csv_header());
                    for s in snaps.iter() {
                        sink.emit_raw_line(&format_stats_csv_row(s, tick_seq, &tick_ts));
                    }
                }
                SinkKind::Mqtt => {}
            }
        }
        return;
    }

    info!("WTG v{} initializing...", env!("CARGO_PKG_VERSION"));

    // Mode: `--probe-fields`
    if args.probe_fields {
        let ctx = match wtg_core::nvml::init_context() {
            Ok(ctx) => ctx,
            Err(e) => {
                eprintln!("WTG --probe-fields init failed: {e}");
                process::exit(wtg_core::exit_code_for_status("unavailable"));
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
                let status = if e == "all NVIDIA device samples failed" {
                    "error"
                } else {
                    "unavailable"
                };
                process::exit(wtg_core::exit_code_for_status(status));
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
                process::exit(wtg_core::exit_code_for_status("unavailable"));
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
                let status = if e == "all NVIDIA device samples failed" {
                    "error"
                } else {
                    "unavailable"
                };
                process::exit(wtg_core::exit_code_for_status(status));
            }
        }
        return;
    }

    // Mode: `--once`
    if args.once {
        print_product_header();
        let report = wtg_core::nvml::snapshot_report_bounded_once(Duration::from_millis(
            NVML_ONCE_TIMEOUT_MS,
        ));
        let snaps = report.successful_snapshots();
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
            print_mode_header("snapshot", args.provider, None);
            println!();
            if report.status != "ok" {
                print_provider_status_block(
                    report.status,
                    report
                        .reason
                        .as_deref()
                        .unwrap_or("provider returned no additional details"),
                );
                print_nvml_failed_device_blocks(&report);
                process::exit(wtg_core::exit_code_for_status(report.status));
            }

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
                            sink.emit_raw_line(&format_snapshot_csv_row(s, tick_seq, &tick_ts));
                        }
                        SinkKind::Mqtt => {}
                    }
                }
            }
            print_nvml_failed_device_blocks(&report);
        }
        return;
    }

    // Mode: `--watch`
    if args.watch {
        let interval_ms = args.interval_ms.unwrap_or(DEFAULT_INTERVAL_MS);

        if interval_ms < 100 {
            eprintln!("WTG note: very low interval ({interval_ms}ms). NVML metrics may not update this quickly; expect duplicates.");
        }

        print_product_header();

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
            print_mode_header("watch", args.provider, Some(interval_ms));
            println!();
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
                                if let Err(e) = cli_mqtt::publish_ha_discovery_for_snapshots(
                                    mqtt_sink, &args, &snapshots,
                                ) {
                                    eprintln!("WTG MQTT error: {e}");
                                    process::exit(wtg_core::exit_code_for_status("error"));
                                }
                                mqtt_ha_discovery_published = true;
                            }
                            if let Err(e) = cli_mqtt::publish_snapshots(
                                mqtt_sink, &args, &ctx, &snapshots, tick_seq, &tick_ts,
                            ) {
                                eprintln!("WTG MQTT error: {e}");
                                process::exit(wtg_core::exit_code_for_status("error"));
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
                        process::exit(wtg_core::exit_code_for_status("unavailable"));
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
                                if let Err(e) = cli_mqtt::publish_ha_discovery_for_snapshots(
                                    mqtt_sink, &args, &snapshots,
                                ) {
                                    eprintln!("WTG MQTT error: {e}");
                                    process::exit(wtg_core::exit_code_for_status("error"));
                                }
                                mqtt_ha_discovery_published = true;
                            }
                            if let Err(e) = cli_mqtt::publish_snapshots(
                                mqtt_sink, &args, ctx, &snapshots, tick_seq, &tick_ts,
                            ) {
                                eprintln!("WTG MQTT error: {e}");
                                process::exit(wtg_core::exit_code_for_status("error"));
                            }
                        }
                        println!();
                        tick_seq += 1;
                    }
                    Err(e) => {
                        eprintln!("WTG --watch failed: {e}");
                        let status = if e == "all NVIDIA device samples failed" {
                            "error"
                        } else {
                            "unavailable"
                        };
                        process::exit(wtg_core::exit_code_for_status(status));
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
    use std::fs;

    #[test]
    fn mqtt_auth_combination_accepts_no_auth() {
        assert!(validate_mqtt_auth_combination(None, None, None).is_ok());
    }

    #[test]
    fn provider_amd_accepts_once_stats() {
        let mut args = CliArgs::default();
        args.once = true;
        args.stats = true;
        args.provider = Some(ProviderKind::Amd);

        validate_args_result(&args).unwrap();
    }

    #[test]
    fn provider_amd_accepts_once() {
        let mut args = CliArgs::default();
        args.once = true;
        args.provider = Some(ProviderKind::Amd);

        validate_args_result(&args).unwrap();
    }

    #[test]
    fn provider_amd_accepts_watch() {
        let mut args = CliArgs::default();
        args.watch = true;
        args.provider = Some(ProviderKind::Amd);

        validate_args_result(&args).unwrap();
    }

    #[test]
    fn provider_amd_accepts_watch_stats() {
        let mut args = CliArgs::default();
        args.watch = true;
        args.stats = true;
        args.provider = Some(ProviderKind::Amd);

        validate_args_result(&args).unwrap();
    }

    #[test]
    fn provider_amd_accepts_probe() {
        let mut args = CliArgs::default();
        args.probe = true;
        args.provider = Some(ProviderKind::Amd);

        validate_args_result(&args).unwrap();
    }

    #[test]
    fn provider_amd_rejects_once_and_probe() {
        let mut args = CliArgs::default();
        args.once = true;
        args.probe = true;
        args.provider = Some(ProviderKind::Amd);

        let err = validate_args_result(&args).unwrap_err();

        assert_eq!(
            err,
            "--probe is mutually exclusive with --once and --watch."
        );
    }

    #[test]
    fn provider_amd_rejects_probe_fields() {
        let mut args = CliArgs::default();
        args.probe_fields = true;
        args.provider = Some(ProviderKind::Amd);
        args.field_ids.push(1);

        let err = validate_args_result(&args).unwrap_err();

        assert_eq!(err, "--provider amd does not support --probe-fields.");
    }

    #[test]
    fn provider_amd_rejects_sink() {
        let mut args = CliArgs::default();
        args.once = true;
        args.provider = Some(ProviderKind::Amd);
        args.sink = Some(SinkKind::Jsonl);

        let err = validate_args_result(&args).unwrap_err();

        assert_eq!(err, "--provider amd does not support --sink.");
    }

    #[test]
    fn provider_intel_accepts_once() {
        let mut args = CliArgs::default();
        args.once = true;
        args.provider = Some(ProviderKind::Intel);

        validate_args_result(&args).unwrap();
    }

    #[test]
    fn provider_intel_accepts_watch() {
        let mut args = CliArgs::default();
        args.watch = true;
        args.provider = Some(ProviderKind::Intel);

        validate_args_result(&args).unwrap();
    }

    #[test]
    fn provider_intel_accepts_once_stats() {
        let mut args = CliArgs::default();
        args.once = true;
        args.stats = true;
        args.provider = Some(ProviderKind::Intel);

        validate_args_result(&args).unwrap();
    }

    #[test]
    fn provider_intel_accepts_watch_stats() {
        let mut args = CliArgs::default();
        args.watch = true;
        args.stats = true;
        args.provider = Some(ProviderKind::Intel);

        validate_args_result(&args).unwrap();
    }

    #[test]
    fn provider_intel_accepts_probe() {
        let mut args = CliArgs::default();
        args.probe = true;
        args.provider = Some(ProviderKind::Intel);

        validate_args_result(&args).unwrap();
    }

    #[test]
    fn provider_intel_rejects_once_and_probe() {
        let mut args = CliArgs::default();
        args.once = true;
        args.probe = true;
        args.provider = Some(ProviderKind::Intel);

        let err = validate_args_result(&args).unwrap_err();

        assert_eq!(
            err,
            "--probe is mutually exclusive with --once and --watch."
        );
    }

    #[test]
    fn provider_intel_rejects_probe_fields() {
        let mut args = CliArgs::default();
        args.probe_fields = true;
        args.provider = Some(ProviderKind::Intel);
        args.field_ids.push(1);

        let err = validate_args_result(&args).unwrap_err();

        assert_eq!(err, "--provider intel does not support --probe-fields.");
    }

    #[test]
    fn provider_intel_rejects_sink() {
        let mut args = CliArgs::default();
        args.once = true;
        args.provider = Some(ProviderKind::Intel);
        args.sink = Some(SinkKind::Jsonl);

        let err = validate_args_result(&args).unwrap_err();

        assert_eq!(err, "--provider intel does not support --sink.");
    }

    #[test]
    fn mqtt_auth_combination_accepts_username_and_password() {
        assert!(validate_mqtt_auth_combination(Some("user"), Some("secret"), None).is_ok());
    }

    #[test]
    fn mqtt_auth_combination_accepts_username_and_password_env() {
        assert!(
            validate_mqtt_auth_combination(Some("user"), None, Some("WTG_MQTT_PASSWORD")).is_ok()
        );
    }

    #[test]
    fn mqtt_auth_combination_rejects_username_alone() {
        let err = validate_mqtt_auth_combination(Some("user"), None, None).unwrap_err();

        assert!(err.contains("--mqtt-username requires"));
    }

    #[test]
    fn mqtt_auth_combination_rejects_password_alone() {
        let err = validate_mqtt_auth_combination(None, Some("secret"), None).unwrap_err();

        assert!(err.contains("--mqtt-password requires"));
    }

    #[test]
    fn mqtt_auth_combination_rejects_password_env_alone() {
        let err =
            validate_mqtt_auth_combination(None, None, Some("WTG_MQTT_PASSWORD")).unwrap_err();

        assert!(err.contains("--mqtt-password-env requires"));
    }

    #[test]
    fn mqtt_auth_combination_rejects_password_and_password_env_together() {
        let err =
            validate_mqtt_auth_combination(Some("user"), Some("secret"), Some("WTG_MQTT_PASSWORD"))
                .unwrap_err();

        assert!(err.contains("cannot be used together"));
    }

    #[test]
    fn save_config_rejects_missing_host() {
        let args = CliArgs {
            mqtt_save_config: true,
            mqtt_node_id: Some("bench".to_string()),
            ..CliArgs::default()
        };

        let err = validate_save_config_args(&args).unwrap_err();
        assert!(err.contains("requires --mqtt-host"));
    }

    #[test]
    fn save_config_rejects_missing_node_id() {
        let args = CliArgs {
            mqtt_save_config: true,
            mqtt_host: Some("broker.local".to_string()),
            ..CliArgs::default()
        };

        let err = validate_save_config_args(&args).unwrap_err();
        assert!(err.contains("requires --mqtt-node-id"));
    }

    #[test]
    fn save_config_rejects_runtime_modes() {
        let args = CliArgs {
            mqtt_save_config: true,
            mqtt_host: Some("broker.local".to_string()),
            mqtt_node_id: Some("bench".to_string()),
            watch: true,
            ..CliArgs::default()
        };

        let err = validate_save_config_args(&args).unwrap_err();
        assert!(err.contains("runtime modes"));
    }

    #[test]
    fn force_config_requires_save_config() {
        let args = CliArgs {
            force_config: true,
            ..CliArgs::default()
        };

        let err = validate_save_config_args(&args).unwrap_err();
        assert!(err.contains("--force-config is valid only"));
    }

    #[test]
    fn save_config_writes_valid_toml_without_auth() {
        let args = CliArgs {
            mqtt_save_config: true,
            mqtt_host: Some("broker.local".to_string()),
            mqtt_node_id: Some("bench".to_string()),
            ..CliArgs::default()
        };
        validate_save_config_args(&args).unwrap();

        let saved = saved_mqtt_config_from_args(&args);
        let temp_path =
            std::env::temp_dir().join(format!("wtg_save_test_{}.toml", std::process::id()));
        let _ = fs::remove_file(&temp_path);

        let result = config::write_config_file(&saved, &temp_path, false);
        assert!(result.is_ok());

        let parsed = config::parse_config_toml(&fs::read_to_string(&temp_path).unwrap()).unwrap();
        let mqtt = parsed.mqtt.unwrap();
        assert!(mqtt.enabled());
        assert_eq!(mqtt.host(), Some("broker.local"));
        assert_eq!(mqtt.node_id(), Some("bench"));

        let _ = fs::remove_file(&temp_path);
    }

    #[test]
    fn save_config_writes_direct_password() {
        let args = CliArgs {
            mqtt_save_config: true,
            mqtt_host: Some("broker.local".to_string()),
            mqtt_node_id: Some("bench".to_string()),
            mqtt_username: Some("wtg".to_string()),
            mqtt_password: Some("test123".to_string()),
            ..CliArgs::default()
        };
        validate_save_config_args(&args).unwrap();

        let saved = saved_mqtt_config_from_args(&args);
        let temp_path =
            std::env::temp_dir().join(format!("wtg_save_pwd_test_{}.toml", std::process::id()));
        let _ = fs::remove_file(&temp_path);

        config::write_config_file(&saved, &temp_path, false).unwrap();
        let content = fs::read_to_string(&temp_path).unwrap();
        assert!(content.contains("password = \"test123\""));
        assert!(content.contains("password_env = \"\""));

        let _ = fs::remove_file(&temp_path);
    }

    #[test]
    fn save_config_writes_password_env() {
        let args = CliArgs {
            mqtt_save_config: true,
            mqtt_host: Some("broker.local".to_string()),
            mqtt_node_id: Some("bench".to_string()),
            mqtt_username: Some("wtg".to_string()),
            mqtt_password_env: Some("WTG_MQTT_PASSWORD".to_string()),
            ..CliArgs::default()
        };
        validate_save_config_args(&args).unwrap();

        let saved = saved_mqtt_config_from_args(&args);
        let temp_path =
            std::env::temp_dir().join(format!("wtg_save_env_test_{}.toml", std::process::id()));
        let _ = fs::remove_file(&temp_path);

        config::write_config_file(&saved, &temp_path, false).unwrap();
        let content = fs::read_to_string(&temp_path).unwrap();
        assert!(content.contains("password = \"\""));
        assert!(content.contains("password_env = \"WTG_MQTT_PASSWORD\""));

        let _ = fs::remove_file(&temp_path);
    }

    #[test]
    fn config_merge_rejects_both_password_sources() {
        let config = config::parse_config_toml(
            r#"
[mqtt]
enabled = true
host = "broker"
node_id = "bench1"
username = "user"
password = "direct"
password_env = "ENV_PASSWORD"
"#,
        )
        .unwrap();
        let mut args = CliArgs::default();
        args.watch = true;

        apply_config(&mut args, &config);

        let err = validate_mqtt_auth_combination(
            args.mqtt_username.as_deref(),
            args.mqtt_password.as_deref(),
            args.mqtt_password_env.as_deref(),
        )
        .unwrap_err();

        assert!(err.contains("cannot be used together"));
    }

    #[test]
    fn apply_config_applies_direct_password() {
        let config = config::parse_config_toml(
            r#"
[mqtt]
enabled = true
host = "broker"
node_id = "bench1"
username = "user"
password = "direct"
"#,
        )
        .unwrap();
        let mut args = CliArgs::default();
        args.watch = true;

        apply_config(&mut args, &config);

        assert_eq!(args.mqtt_password.as_deref(), Some("direct"));
        assert!(args.mqtt_password_env.is_none());
    }

    #[test]
    fn config_disabled_mqtt_does_not_activate_mqtt() {
        let config = config::parse_config_toml(
            r#"
[mqtt]
enabled = false
host = "broker"
node_id = "bench1"
"#,
        )
        .unwrap();
        let mut args = CliArgs::default();
        args.watch = true;

        apply_config(&mut args, &config);

        assert!(!mqtt_is_active(&args));
        assert!(args.mqtt_host.is_none());
        assert!(args.mqtt_node_id.is_none());
    }

    #[test]
    fn config_enabled_mqtt_activates_mqtt_for_watch_without_sink_flag() {
        let config = config::parse_config_toml(
            r#"
[mqtt]
enabled = true
host = "broker"
node_id = "bench1"
"#,
        )
        .unwrap();
        let mut args = CliArgs::default();
        args.watch = true;

        apply_config(&mut args, &config);

        assert!(mqtt_is_active(&args));
        assert!(args.mqtt_enabled_from_config);
        assert!(args.sink.is_none());
        assert_eq!(args.mqtt_host.as_deref(), Some("broker"));
        assert_eq!(args.mqtt_node_id.as_deref(), Some("bench1"));
    }

    #[test]
    fn sink_mqtt_activates_mqtt_even_when_config_disables_mqtt() {
        let config = config::parse_config_toml(
            r#"
[mqtt]
enabled = false
host = "broker"
node_id = "bench1"
"#,
        )
        .unwrap();
        let mut args = CliArgs::default();
        args.watch = true;
        args.sink = Some(SinkKind::Mqtt);

        apply_config(&mut args, &config);

        assert!(mqtt_is_active(&args));
        assert!(!args.mqtt_enabled_from_config);
        assert_eq!(args.mqtt_host.as_deref(), Some("broker"));
        assert_eq!(args.mqtt_node_id.as_deref(), Some("bench1"));
    }

    #[test]
    fn cli_mqtt_values_override_config_values() {
        let config = config::parse_config_toml(
            r#"
[mqtt]
enabled = true
host = "config-broker"
port = 1883
username = "config-user"
password_env = "CONFIG_PASSWORD"
topic_prefix = "config-prefix"
node_id = "config-node"

[mqtt.home_assistant]
discovery = true
discovery_prefix = "config-ha"
retain_discovery = true
"#,
        )
        .unwrap();
        let mut args = CliArgs::default();
        args.watch = true;
        args.sink = Some(SinkKind::Mqtt);
        args.mqtt_host = Some("cli-broker".to_string());
        args.mqtt_port = Some("1884".to_string());
        args.mqtt_node_id = Some("cli-node".to_string());
        args.mqtt_ha_prefix = Some("cli-ha".to_string());

        apply_config(&mut args, &config);

        assert_eq!(args.mqtt_host.as_deref(), Some("cli-broker"));
        assert_eq!(args.mqtt_port.as_deref(), Some("1884"));
        assert_eq!(args.mqtt_node_id.as_deref(), Some("cli-node"));
        assert_eq!(args.mqtt_ha_prefix.as_deref(), Some("cli-ha"));
        assert_eq!(args.mqtt_username.as_deref(), Some("config-user"));
        assert_eq!(args.mqtt_password_env.as_deref(), Some("CONFIG_PASSWORD"));
        assert_eq!(args.mqtt_topic_prefix.as_deref(), Some("config-prefix"));
        assert!(args.mqtt_ha_discovery);
        assert!(args.mqtt_retain_discovery);
    }

    #[test]
    fn cleanup_ignores_config_discovery_conflict() {
        let config = config::parse_config_toml(
            r#"
[mqtt]
enabled = true
host = "broker"
node_id = "bench1"

[mqtt.home_assistant]
discovery = true
discovery_prefix = "config-ha"
retain_discovery = true
"#,
        )
        .unwrap();
        let mut args = CliArgs {
            sink: Some(SinkKind::Mqtt),
            mqtt_ha_remove_discovery: true,
            ..CliArgs::default()
        };

        apply_config(&mut args, &config);

        assert!(!args.mqtt_ha_discovery);
        assert!(!args.mqtt_ha_discovery_from_cli);
        assert_eq!(args.mqtt_ha_prefix.as_deref(), Some("config-ha"));
        assert!(args.mqtt_retain_discovery);
        validate_args_result(&args).unwrap();
    }

    #[test]
    fn cleanup_config_loads_mqtt_settings_when_disabled() {
        let config = config::parse_config_toml(
            r#"
[mqtt]
enabled = false
host = "broker"
port = 1884
username = "user"
password = "direct"
topic_prefix = "wtg-test"
node_id = "bench1"

[mqtt.home_assistant]
discovery = false
discovery_prefix = "config-ha"
"#,
        )
        .unwrap();
        let mut args = CliArgs {
            sink: Some(SinkKind::Mqtt),
            mqtt_ha_remove_discovery: true,
            ..CliArgs::default()
        };

        apply_config(&mut args, &config);

        assert_eq!(args.mqtt_host.as_deref(), Some("broker"));
        assert_eq!(args.mqtt_port.as_deref(), Some("1884"));
        assert_eq!(args.mqtt_username.as_deref(), Some("user"));
        assert_eq!(args.mqtt_password.as_deref(), Some("direct"));
        assert_eq!(args.mqtt_topic_prefix.as_deref(), Some("wtg-test"));
        assert_eq!(args.mqtt_node_id.as_deref(), Some("bench1"));
        assert_eq!(args.mqtt_ha_prefix.as_deref(), Some("config-ha"));
        validate_args_result(&args).unwrap();
        assert!(mqtt_options_from_args(&args).is_some());
    }

    #[test]
    fn cleanup_retain_discovery_is_accepted() {
        let args = CliArgs {
            sink: Some(SinkKind::Mqtt),
            mqtt_host: Some("broker".to_string()),
            mqtt_node_id: Some("bench1".to_string()),
            mqtt_ha_remove_discovery: true,
            mqtt_retain_discovery: true,
            ..CliArgs::default()
        };

        validate_args_result(&args).unwrap();
    }

    #[test]
    fn cleanup_does_not_require_ha_discovery() {
        let args = CliArgs {
            sink: Some(SinkKind::Mqtt),
            mqtt_host: Some("broker".to_string()),
            mqtt_node_id: Some("bench1".to_string()),
            mqtt_ha_remove_discovery: true,
            ..CliArgs::default()
        };

        validate_args_result(&args).unwrap();
    }

    #[test]
    fn explicit_cli_discovery_and_cleanup_still_conflict() {
        let args = CliArgs {
            sink: Some(SinkKind::Mqtt),
            mqtt_host: Some("broker".to_string()),
            mqtt_node_id: Some("bench1".to_string()),
            mqtt_ha_discovery: true,
            mqtt_ha_discovery_from_cli: true,
            mqtt_ha_remove_discovery: true,
            ..CliArgs::default()
        };

        let err = validate_args_result(&args).unwrap_err();
        assert!(err.contains("cannot be combined with --mqtt-ha-discovery"));
    }

    #[test]
    fn empty_config_strings_are_not_applied_to_cli_args() {
        let config = config::parse_config_toml(
            r#"
[mqtt]
enabled = true
host = ""
username = ""
password_env = ""
node_id = ""
"#,
        )
        .unwrap();
        let mut args = CliArgs::default();
        args.watch = true;

        apply_config(&mut args, &config);

        assert!(mqtt_is_active(&args));
        assert!(args.mqtt_host.is_none());
        assert!(args.mqtt_username.is_none());
        assert!(args.mqtt_password_env.is_none());
        assert!(args.mqtt_node_id.is_none());
    }

    #[test]
    fn mqtt_auth_resolution_rejects_missing_password_env_var() {
        let err =
            mqtt_settings::resolve_mqtt_auth("user", "WTG_MQTT_PASSWORD", |_| None).unwrap_err();

        assert!(err.contains("WTG_MQTT_PASSWORD"));
        assert!(err.contains("not set"));
    }

    #[test]
    fn mqtt_auth_resolution_rejects_empty_password_env_var() {
        let err =
            mqtt_settings::resolve_mqtt_auth("user", "WTG_MQTT_PASSWORD", |_| Some(String::new()))
                .unwrap_err();

        assert!(err.contains("WTG_MQTT_PASSWORD"));
        assert!(err.contains("must not be empty"));
    }

    #[test]
    fn mqtt_auth_resolution_accepts_username_and_password_env_var() {
        let password = String::from_utf8(vec![115, 101, 99, 114, 101, 116]).unwrap();

        assert!(
            mqtt_settings::resolve_mqtt_auth("user", "WTG_MQTT_PASSWORD", |_| Some(password))
                .is_ok()
        );
    }
}
