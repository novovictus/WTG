// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Adam Hooper

use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use eframe::egui;
use serde_json::Value;
use std::os::windows::process::CommandExt;
use wtg_core::nvml::{
    probe_context::{query_probe_context_for_gpu_with_ctx, GpuProbeContext},
    GpuSnapshot, NvmlContext,
};
use wtg_providers::{amd_adl, intel_level_zero};

use crate::config;
use crate::mqtt::MqttSink;
use crate::mqtt_settings;
use crate::ui_mqtt_settings::{self, LoadedMqttSettings};

const DEFAULT_REFRESH_MS: u64 = 1000;
const MIN_REFRESH_MS: u64 = 250;
const MAX_REFRESH_MS: u64 = 5_000;
const WINDOW_WIDTH: f32 = 980.0;
const WINDOW_HEIGHT: f32 = 600.0;
const DETACHED_PROCESS: u32 = 0x0000_0008;
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn run() -> Result<(), eframe::Error> {
    eframe::run_native(
        "WTG UI Experimental",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([WINDOW_WIDTH, WINDOW_HEIGHT]),
            ..Default::default()
        },
        Box::new(|_cc| Box::new(WtgUiApp::new())),
    )
}

struct WtgUiApp {
    nvml_ctx: Option<NvmlContext>,
    devices: Vec<DeviceView>,
    selected_device: usize,
    mqtt_form: MqttFormState,
    mqtt_status: MqttStatus,
    refresh_interval_ms: u64,
    running: bool,
    last_refresh: Option<Instant>,
    last_refresh_unix_label: String,
    last_error: Option<String>,
}

impl WtgUiApp {
    fn new() -> Self {
        let mut app = Self {
            nvml_ctx: None,
            devices: Vec::new(),
            selected_device: 0,
            mqtt_form: MqttFormState::default(),
            mqtt_status: MqttStatus::idle("MQTT idle. Actions only run when you click a button."),
            refresh_interval_ms: DEFAULT_REFRESH_MS,
            running: true,
            last_refresh: None,
            last_refresh_unix_label: "N/A".to_string(),
            last_error: None,
        };
        app.refresh();
        app
    }

    fn refresh_due(&self) -> bool {
        self.last_refresh
            .map(|last| last.elapsed() >= self.refresh_interval())
            .unwrap_or(true)
    }

    fn refresh_interval(&self) -> Duration {
        Duration::from_millis(self.refresh_interval_ms)
    }

    fn refresh(&mut self) {
        let mut devices = Vec::new();
        let mut errors = Vec::new();

        if self.nvml_ctx.is_none() {
            match wtg_core::nvml::init_context() {
                Ok(ctx) => {
                    self.nvml_ctx = Some(ctx);
                }
                Err(err) => {
                    errors.push(format!("NVML init failed: {err}"));
                }
            }
        }

        if let Some(ctx) = self.nvml_ctx.as_ref() {
            match wtg_core::nvml::snapshot_all_with_ctx(ctx) {
                Ok(snaps) => {
                    devices.extend(snaps.iter().map(|snapshot| {
                        let context = query_probe_context_for_gpu_with_ctx(ctx, snapshot.index);
                        DeviceView::Nvidia(NvidiaDeviceView::from_snapshot(snapshot, context))
                    }));
                }
                Err(err) => {
                    errors.push(format!("NVIDIA telemetry refresh failed: {err}"));
                    self.nvml_ctx = None;
                }
            }
        }

        match collect_amd_device_views() {
            Ok(amd_devices) => devices.extend(amd_devices.into_iter().map(DeviceView::Amd)),
            Err(err) => errors.push(format!("AMD ADL refresh failed: {err}")),
        }

        match collect_intel_device_views() {
            Ok(intel_devices) => devices.extend(intel_devices.into_iter().map(DeviceView::Intel)),
            Err(err) => errors.push(format!("Intel Level Zero refresh failed: {err}")),
        }

        self.devices = devices;
        if self.selected_device >= self.devices.len() {
            self.selected_device = self.devices.len().saturating_sub(1);
        }
        self.last_error = (!errors.is_empty()).then(|| errors.join(" | "));
        self.mark_refreshed();
    }

    fn mark_refreshed(&mut self) {
        self.last_refresh = Some(Instant::now());
        self.last_refresh_unix_label = now_ts();
    }

    fn load_mqtt_config(&mut self) {
        let config_path = normalized_config_path(&self.mqtt_form.config_path);
        self.set_mqtt_status(MqttStatusKind::Running, "Loading MQTT config...");
        match config::load_config_file(Path::new(&config_path)) {
            Ok(config) => {
                self.mqtt_form
                    .apply_loaded_settings(ui_mqtt_settings::mqtt_settings_from_config(&config));
                self.mqtt_form.config_path = config_path.clone();
                self.set_mqtt_status(
                    MqttStatusKind::Success,
                    format!("Loaded MQTT config from {config_path}."),
                );
            }
            Err(err) => {
                self.set_mqtt_status(MqttStatusKind::Error, format!("Config load error: {err}"));
            }
        }
    }

    fn save_mqtt_config(&mut self) {
        let config_path = normalized_config_path(&self.mqtt_form.config_path);
        self.set_mqtt_status(MqttStatusKind::Running, "Saving MQTT config...");
        let saved = mqtt_settings::saved_mqtt_config_from_values(
            Some(self.mqtt_form.host.as_str()),
            Some(self.mqtt_form.port.as_str()),
            Some(self.mqtt_form.username.as_str()),
            direct_password(&self.mqtt_form),
            env_password(&self.mqtt_form),
            Some(self.mqtt_form.topic_prefix.as_str()),
            Some(self.mqtt_form.node_id.as_str()),
            self.mqtt_form.ha_discovery_enabled,
            if self.mqtt_form.ha_discovery_enabled {
                Some(self.mqtt_form.ha_discovery_prefix.as_str())
            } else {
                None
            },
            self.mqtt_form.retain_discovery,
        );

        match saved
            .and_then(|saved| config::write_config_file(&saved, Path::new(&config_path), true))
        {
            Ok(path) => {
                self.mqtt_form.config_path = path.display().to_string();
                self.set_mqtt_status(
                    MqttStatusKind::Success,
                    format!("Saved MQTT config to {}.", path.display()),
                );
            }
            Err(err) => {
                self.set_mqtt_status(MqttStatusKind::Error, format!("Config save error: {err}"));
            }
        }
    }

    fn generate_default_config(&mut self) {
        let config_path = normalized_config_path(&self.mqtt_form.config_path);
        let path = Path::new(&config_path);
        self.set_mqtt_status(MqttStatusKind::Running, "Generating default config...");

        let result = if config_path == config::DEFAULT_CONFIG_FILE_NAME {
            config::create_default_config_file()
        } else {
            write_default_template(path)
        };

        match result {
            Ok(path) => {
                self.mqtt_form.config_path = path.display().to_string();
                self.set_mqtt_status(
                    MqttStatusKind::Success,
                    format!("Generated default config at {}.", path.display()),
                );
            }
            Err(err) => {
                self.set_mqtt_status(
                    MqttStatusKind::Error,
                    format!("Default config error: {err}"),
                );
            }
        }
    }

    fn copy_cli_preview(&mut self, egui_ctx: &egui::Context) {
        egui_ctx.copy_text(self.mqtt_form.cli_preview());
        self.set_mqtt_status(
            MqttStatusKind::Success,
            "Copied equivalent CLI command to the clipboard.",
        );
    }

    fn test_broker_connection(&mut self) {
        self.set_mqtt_status(MqttStatusKind::Running, "Testing MQTT broker connection...");
        let options = mqtt_settings::mqtt_options_from_values(
            true,
            Some(self.mqtt_form.host.as_str()),
            Some(self.mqtt_form.port.as_str()),
            Some(self.mqtt_form.topic_prefix.as_str()),
            Some(self.mqtt_form.node_id.as_str()),
            Some(self.mqtt_form.username.as_str()),
            direct_password(&self.mqtt_form),
            env_password(&self.mqtt_form),
            self.mqtt_form.ha_discovery_enabled,
            false,
            if self.mqtt_form.ha_discovery_enabled {
                Some(self.mqtt_form.ha_discovery_prefix.as_str())
            } else {
                None
            },
            self.mqtt_form.retain_discovery,
        );

        match options.and_then(|options| {
            options.ok_or_else(|| "MQTT connection test requires active MQTT.".to_string())
        }) {
            Ok(options) => match MqttSink::connect(options) {
                Ok(_) => {
                    self.set_mqtt_status(
                        MqttStatusKind::Success,
                        "MQTT broker connection succeeded.",
                    );
                }
                Err(err) => {
                    self.set_mqtt_status(
                        MqttStatusKind::Error,
                        format!("MQTT connection error: {err}"),
                    );
                }
            },
            Err(err) => {
                self.set_mqtt_status(
                    MqttStatusKind::Error,
                    format!("MQTT connection error: {err}"),
                );
            }
        }
    }

    fn clear_retained_ha_discovery(&mut self) {
        self.set_mqtt_status(
            MqttStatusKind::Running,
            "Publishing retained Home Assistant discovery cleanup...",
        );
        let options = mqtt_settings::mqtt_options_from_values(
            true,
            Some(self.mqtt_form.host.as_str()),
            Some(self.mqtt_form.port.as_str()),
            Some(self.mqtt_form.topic_prefix.as_str()),
            Some(self.mqtt_form.node_id.as_str()),
            Some(self.mqtt_form.username.as_str()),
            direct_password(&self.mqtt_form),
            env_password(&self.mqtt_form),
            false,
            true,
            Some(self.mqtt_form.ha_discovery_prefix.as_str()),
            self.mqtt_form.retain_discovery,
        );

        let options = match options.and_then(|options| {
            options.ok_or_else(|| "MQTT cleanup requires active MQTT.".to_string())
        }) {
            Ok(options) => options,
            Err(err) => {
                self.set_mqtt_status(MqttStatusKind::Error, format!("MQTT cleanup error: {err}"));
                return;
            }
        };

        let mut sink = match MqttSink::connect(options) {
            Ok(sink) => sink,
            Err(err) => {
                self.set_mqtt_status(MqttStatusKind::Error, format!("MQTT cleanup error: {err}"));
                return;
            }
        };

        let ctx = match wtg_core::nvml::init_context() {
            Ok(ctx) => ctx,
            Err(err) => {
                self.set_mqtt_status(
                    MqttStatusKind::Error,
                    format!("MQTT cleanup init failed: {err}"),
                );
                return;
            }
        };
        let snapshots = match wtg_core::nvml::snapshot_all_with_ctx(&ctx) {
            Ok(snapshots) => snapshots,
            Err(err) => {
                self.set_mqtt_status(
                    MqttStatusKind::Error,
                    format!("MQTT cleanup snapshot failed: {err}"),
                );
                return;
            }
        };

        match sink.publish_ha_discovery_cleanup_for_snapshots(&snapshots) {
            Ok(_) => {
                self.set_mqtt_status(
                    MqttStatusKind::Success,
                    "MQTT Home Assistant discovery cleanup published.",
                );
            }
            Err(err) => {
                self.set_mqtt_status(MqttStatusKind::Error, format!("MQTT cleanup error: {err}"));
            }
        }
    }

    fn launch_cli_mqtt_publisher(&mut self) {
        let config_path = normalized_config_path(&self.mqtt_form.config_path);
        let config_file = Path::new(&config_path);
        if !config_file.exists() {
            self.set_mqtt_status(
                MqttStatusKind::Error,
                "Save config before launching the CLI publisher.",
            );
            return;
        }

        self.set_mqtt_status(MqttStatusKind::Running, "Launching CLI MQTT publisher...");
        let mut command = cli_launch_command(config_file);
        match command.spawn() {
            Ok(_) => {
                self.set_mqtt_status(
                    MqttStatusKind::Success,
                    format!(
                        "Launched CLI MQTT publisher with saved config {}.",
                        config_file.display()
                    ),
                );
            }
            Err(err) => {
                self.set_mqtt_status(MqttStatusKind::Error, format!("CLI launch error: {err}"));
            }
        }
    }

    fn stop_all_wtg_processes(&mut self) {
        match Command::new("taskkill")
            .args(["/IM", "wtg.exe", "/F"])
            .stdin(Stdio::null())
            .output()
        {
            Ok(output) if output.status.success() => {
                self.set_mqtt_status(
                    MqttStatusKind::Success,
                    "Stop all wtg.exe processes command sent.",
                );
            }
            Ok(output) => {
                let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let err = if err.is_empty() {
                    String::from_utf8_lossy(&output.stdout).trim().to_string()
                } else {
                    err
                };
                let err = if err.is_empty() {
                    format!("taskkill exited with status {}.", output.status)
                } else {
                    err
                };
                self.set_mqtt_status(
                    MqttStatusKind::Error,
                    format!("Stop all wtg.exe processes failed: {err}"),
                );
            }
            Err(err) => {
                self.set_mqtt_status(
                    MqttStatusKind::Error,
                    format!("Stop all wtg.exe processes failed: {err}"),
                );
            }
        }
    }

    fn set_mqtt_status(&mut self, kind: MqttStatusKind, message: impl Into<String>) {
        self.mqtt_status = MqttStatus::new(kind, message);
    }
}

impl eframe::App for WtgUiApp {
    fn update(&mut self, egui_ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.running && self.refresh_due() {
            self.refresh();
        }

        egui::TopBottomPanel::top("toolbar").show(egui_ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("WTG UI Experimental");
                ui.label(format!("WTG v{}", env!("CARGO_PKG_VERSION")));

                let toggle_label = if self.running { "Pause" } else { "Resume" };
                if ui.button(toggle_label).clicked() {
                    self.running = !self.running;
                }

                if ui.button("Refresh now").clicked() {
                    self.refresh();
                }

                ui.separator();
                ui.label("Refresh");
                ui.add(
                    egui::Slider::new(
                        &mut self.refresh_interval_ms,
                        MIN_REFRESH_MS..=MAX_REFRESH_MS,
                    )
                    .suffix(" ms"),
                );

                ui.separator();
                ui.label(format_refresh_label(
                    self.last_refresh,
                    &self.last_refresh_unix_label,
                ));
            });

            if let Some(error) = &self.last_error {
                ui.colored_label(egui::Color32::YELLOW, error);
            }
        });

        egui::SidePanel::left("devices")
            .resizable(true)
            .default_width(340.0)
            .show(egui_ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("Devices");
                    ui.separator();

                    if self.devices.is_empty() {
                        ui.label("No GPU telemetry available");
                    } else {
                        for (idx, device) in self.devices.iter().enumerate() {
                            let label = device.list_label();
                            if ui
                                .selectable_label(self.selected_device == idx, label)
                                .clicked()
                            {
                                self.selected_device = idx;
                            }
                        }
                    }

                    ui.add_space(12.0);
                    egui::CollapsingHeader::new("MQTT / Home Assistant")
                        .default_open(false)
                        .show(ui, |ui| render_mqtt_panel(ui, egui_ctx, self));
                });
            });

        egui::CentralPanel::default().show(egui_ctx, |ui| {
            if let Some(device) = self.devices.get(self.selected_device) {
                render_device(ui, device);
            } else {
                ui.heading("WTG");
                ui.label("Telemetry will appear here when NVML returns a snapshot.");
            }
        });

        if self.running {
            egui_ctx.request_repaint_after(self.refresh_interval());
        }
    }
}

enum DeviceView {
    Nvidia(NvidiaDeviceView),
    Amd(AmdDeviceView),
    Intel(IntelDeviceView),
}

impl DeviceView {
    fn list_label(&self) -> String {
        match self {
            DeviceView::Nvidia(device) => format!("GPU {}: {}", device.index, device.name),
            DeviceView::Amd(device) => device.name.clone(),
            DeviceView::Intel(device) => device.list_label(),
        }
    }
}

struct NvidiaDeviceView {
    index: u32,
    name: String,
    uuid: String,
    driver_version: String,
    cuda_driver_version: String,
    compute_mode: String,
    perf_state: String,
    pci_bus_id: String,
    gpu_util_pct: u32,
    mem_util_pct: u32,
    vram_used_mib: u64,
    vram_total_mib: u64,
    temp_c: Option<u32>,
    power_w: Option<f32>,
    power_limit_w: Option<f32>,
}

struct AmdDeviceView {
    name: String,
    provider_authority: String,
    provider_source: String,
    physical_key: String,
    activity_pct: Option<u32>,
    temperature_c: Option<f64>,
    engine_clock_mhz: Option<f64>,
    memory_clock_mhz: Option<f64>,
    observed_core_clock_mhz: Option<f64>,
    observed_memory_clock_mhz: Option<f64>,
    vbios_version: Option<String>,
    active: Option<bool>,
}

struct IntelDeviceView {
    device_index: usize,
    name: Option<String>,
    provider_authority: String,
    provider_source: String,
    device_key: String,
    uuid: Option<String>,
    core_clock_mhz: Option<f64>,
    actual_mhz: Option<f64>,
    request_mhz: Option<f64>,
    memory_used_mib: Option<u64>,
    memory_total_mib: Option<u64>,
    power_w: Option<f64>,
    temperature_c: Option<f64>,
    engine_utilizations: Vec<(usize, f64)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MqttStatusKind {
    Idle,
    Success,
    Error,
    Running,
}

#[derive(Debug, Clone)]
struct MqttStatus {
    kind: MqttStatusKind,
    message: String,
}

impl MqttStatus {
    fn new(kind: MqttStatusKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    fn idle(message: impl Into<String>) -> Self {
        Self::new(MqttStatusKind::Idle, message)
    }

    fn label(&self) -> &'static str {
        match self.kind {
            MqttStatusKind::Idle => "Idle",
            MqttStatusKind::Success => "Success",
            MqttStatusKind::Error => "Error",
            MqttStatusKind::Running => "Running",
        }
    }

    fn colors(&self) -> (egui::Color32, egui::Color32, egui::Color32) {
        match self.kind {
            MqttStatusKind::Idle => (
                egui::Color32::from_rgb(44, 58, 76),
                egui::Color32::from_rgb(101, 132, 171),
                egui::Color32::from_rgb(225, 233, 244),
            ),
            MqttStatusKind::Success => (
                egui::Color32::from_rgb(29, 82, 51),
                egui::Color32::from_rgb(78, 181, 117),
                egui::Color32::from_rgb(231, 248, 237),
            ),
            MqttStatusKind::Error => (
                egui::Color32::from_rgb(105, 34, 41),
                egui::Color32::from_rgb(218, 95, 109),
                egui::Color32::from_rgb(255, 234, 237),
            ),
            MqttStatusKind::Running => (
                egui::Color32::from_rgb(26, 73, 97),
                egui::Color32::from_rgb(83, 171, 216),
                egui::Color32::from_rgb(230, 246, 255),
            ),
        }
    }
}

impl NvidiaDeviceView {
    fn from_snapshot(snapshot: &GpuSnapshot, context: GpuProbeContext) -> Self {
        Self {
            index: snapshot.index,
            name: snapshot.name.clone(),
            uuid: snapshot.uuid.clone(),
            driver_version: context.driver_version,
            cuda_driver_version: context.cuda_driver_version,
            compute_mode: context.compute_mode,
            perf_state: context.perf_state,
            pci_bus_id: context.pci_bus_id,
            gpu_util_pct: snapshot.gpu_util_pct,
            mem_util_pct: snapshot.mem_util_pct,
            vram_used_mib: bytes_to_mib(snapshot.mem_used_bytes),
            vram_total_mib: bytes_to_mib(snapshot.mem_total_bytes),
            temp_c: snapshot.temp_c,
            power_w: mw_to_w(snapshot.power_mw),
            power_limit_w: mw_to_w(snapshot.power_limit_mw),
        }
    }
}

fn render_device(ui: &mut egui::Ui, device: &DeviceView) {
    match device {
        DeviceView::Nvidia(device) => render_nvidia_device(ui, device),
        DeviceView::Amd(device) => render_amd_device(ui, device),
        DeviceView::Intel(device) => render_intel_device(ui, device),
    }
}

fn render_nvidia_device(ui: &mut egui::Ui, device: &NvidiaDeviceView) {
    ui.heading(format!("GPU {}: {}", device.index, device.name));
    ui.add_space(8.0);

    egui::Grid::new("device_summary")
        .num_columns(2)
        .spacing([20.0, 8.0])
        .striped(true)
        .show(ui, |ui| {
            row(ui, "Provider", "NVIDIA NVML");
            row(ui, "Source", "nvidia.nvml");
            row(ui, "UUID", &device.uuid);
            row(ui, "Driver", &device.driver_version);
            row(ui, "CUDA driver", &device.cuda_driver_version);
            row(ui, "Compute mode", &device.compute_mode);
            row(ui, "Performance state", &device.perf_state);
            row(ui, "PCI bus", &device.pci_bus_id);
            row(ui, "GPU utilization", &format!("{}%", device.gpu_util_pct));
            row(
                ui,
                "Memory-controller utilization",
                &format!("{}%", device.mem_util_pct),
            );
            row(
                ui,
                "VRAM",
                &format!(
                    "{} MiB / {} MiB",
                    device.vram_used_mib, device.vram_total_mib
                ),
            );
            row(
                ui,
                "Power",
                &format_power(device.power_w, device.power_limit_w),
            );
            row(ui, "Temperature", &format_temperature(device.temp_c));
        });

    ui.add_space(14.0);
    ui.label("Utilization");
    ui.add(progress_bar(device.gpu_util_pct, "GPU"));
    ui.add(progress_bar(device.mem_util_pct, "Memory controller"));

    ui.add_space(10.0);
    ui.label("VRAM");
    ui.add(vram_bar(device.vram_used_mib, device.vram_total_mib));
}

fn render_amd_device(ui: &mut egui::Ui, device: &AmdDeviceView) {
    ui.heading(&device.name);
    ui.add_space(8.0);

    egui::Grid::new("amd_device_summary")
        .num_columns(2)
        .spacing([20.0, 8.0])
        .striped(true)
        .show(ui, |ui| {
            row(ui, "Provider", &device.provider_authority);
            row(ui, "Source", &device.provider_source);
            row(ui, "Physical key", &device.physical_key);
            row(ui, "Active", &format_bool(device.active));
            row(
                ui,
                "Activity",
                &format_optional_percent(device.activity_pct),
            );
            row(
                ui,
                "Engine clock",
                &format_optional_mhz(device.engine_clock_mhz),
            );
            row(
                ui,
                "Memory clock",
                &format_optional_mhz(device.memory_clock_mhz),
            );
            row(
                ui,
                "Observed core clock",
                &format_optional_mhz(device.observed_core_clock_mhz),
            );
            row(
                ui,
                "Observed memory clock",
                &format_optional_mhz(device.observed_memory_clock_mhz),
            );
            row(
                ui,
                "Temperature",
                &format_optional_temperature_f64(device.temperature_c),
            );
            row(
                ui,
                "VBIOS",
                device.vbios_version.as_deref().unwrap_or("Unavailable"),
            );
        });

    if let Some(activity_pct) = device.activity_pct {
        ui.add_space(14.0);
        ui.label("Activity");
        ui.add(progress_bar(activity_pct, "Activity"));
    }
}

fn render_intel_device(ui: &mut egui::Ui, device: &IntelDeviceView) {
    ui.heading(device.heading());
    ui.add_space(8.0);

    egui::Grid::new("intel_device_summary")
        .num_columns(2)
        .spacing([20.0, 8.0])
        .striped(true)
        .show(ui, |ui| {
            row(ui, "Provider", &device.provider_authority);
            row(ui, "Source", &device.provider_source);
            row(ui, "Device key", &device.device_key);
            row(ui, "UUID", device.uuid.as_deref().unwrap_or("Unavailable"));
            row(
                ui,
                "Core clock",
                &format_optional_mhz(device.core_clock_mhz),
            );
            row(
                ui,
                "Frequency",
                &format_intel_frequency(device.actual_mhz, device.request_mhz),
            );
            row(
                ui,
                "Memory",
                &format_optional_memory_pair(device.memory_used_mib, device.memory_total_mib),
            );
            row(ui, "Power", &format_optional_watts_f64(device.power_w));
            row(
                ui,
                "Temperature",
                &format_optional_temperature_f64(device.temperature_c),
            );
        });

    if !device.engine_utilizations.is_empty() {
        ui.add_space(14.0);
        ui.label("Engine utilization");
        for (index, pct) in device.engine_utilizations.iter() {
            ui.add(progress_bar_f64(*pct, &format!("Engine {index}")));
        }
    }

    if let (Some(used_mib), Some(total_mib)) = (device.memory_used_mib, device.memory_total_mib) {
        ui.add_space(10.0);
        ui.label("Memory");
        ui.add(vram_bar(used_mib, total_mib));
    }
}

fn row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(label);
    ui.monospace(value);
    ui.end_row();
}

fn progress_bar(percent: u32, label: &str) -> egui::ProgressBar {
    let clamped = percent.min(100);
    egui::ProgressBar::new(clamped as f32 / 100.0).text(format!("{label}: {percent}%"))
}

fn progress_bar_f64(percent: f64, label: &str) -> egui::ProgressBar {
    let clamped = percent.clamp(0.0, 100.0);
    egui::ProgressBar::new((clamped / 100.0) as f32).text(format!("{label}: {percent:.2}%"))
}

fn vram_bar(used_mib: u64, total_mib: u64) -> egui::ProgressBar {
    let ratio = if total_mib == 0 {
        0.0
    } else {
        (used_mib as f32 / total_mib as f32).clamp(0.0, 1.0)
    };

    egui::ProgressBar::new(ratio).text(format!("{used_mib} MiB / {total_mib} MiB"))
}

fn format_temperature(temp_c: Option<u32>) -> String {
    temp_c
        .map(|temp_c| format!("{temp_c} C"))
        .unwrap_or_else(unavailable)
}

fn format_optional_temperature_f64(temp_c: Option<f64>) -> String {
    temp_c
        .map(|temp_c| format!("{temp_c:.1} C"))
        .unwrap_or_else(unavailable)
}

fn format_power(power_w: Option<f32>, power_limit_w: Option<f32>) -> String {
    match (power_w, power_limit_w) {
        (Some(power_w), Some(power_limit_w)) => format!("{power_w:.1} W / {power_limit_w:.1} W"),
        (Some(power_w), None) => format!("{power_w:.1} W / N/A"),
        (None, Some(power_limit_w)) => format!("N/A / {power_limit_w:.1} W"),
        (None, None) => unavailable(),
    }
}

fn format_optional_watts_f64(power_w: Option<f64>) -> String {
    power_w
        .map(|power_w| format!("{power_w:.1} W"))
        .unwrap_or_else(unavailable)
}

fn format_optional_percent(percent: Option<u32>) -> String {
    percent
        .map(|percent| format!("{percent}%"))
        .unwrap_or_else(unavailable)
}

fn format_optional_mhz(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.1} MHz"))
        .unwrap_or_else(unavailable)
}

fn format_optional_memory_pair(used_mib: Option<u64>, total_mib: Option<u64>) -> String {
    match (used_mib, total_mib) {
        (Some(used_mib), Some(total_mib)) => format!("{used_mib} MiB / {total_mib} MiB"),
        _ => unavailable(),
    }
}

fn format_intel_frequency(actual_mhz: Option<f64>, request_mhz: Option<f64>) -> String {
    match (actual_mhz, request_mhz) {
        (Some(actual_mhz), Some(request_mhz)) => {
            format!("actual {actual_mhz:.1} MHz, requested {request_mhz:.1} MHz")
        }
        (Some(actual_mhz), None) => format!("actual {actual_mhz:.1} MHz"),
        (None, Some(request_mhz)) => format!("requested {request_mhz:.1} MHz"),
        (None, None) => unavailable(),
    }
}

fn format_bool(value: Option<bool>) -> String {
    match value {
        Some(true) => "yes".to_string(),
        Some(false) => "no".to_string(),
        None => unavailable(),
    }
}

impl IntelDeviceView {
    fn list_label(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| format!("Intel device {}", self.device_index))
    }

    fn heading(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| format!("Intel device {}", self.device_index))
    }
}

fn collect_amd_device_views() -> Result<Vec<AmdDeviceView>, String> {
    let sample = amd_adl::collect_once(0);
    let tick_ts = now_ts();
    let stats = amd_adl::format_stats_snapshot_json(&sample, 0, &tick_ts);
    let parsed: Value = serde_json::from_str(&stats)
        .map_err(|err| format!("AMD stats JSON parse failed: {err}"))?;

    let provider_authority = parsed
        .get("provider_authority")
        .and_then(Value::as_str)
        .unwrap_or("AMD ADL")
        .to_string();
    let provider_source = parsed
        .get("provider_source")
        .and_then(Value::as_str)
        .unwrap_or("wtg.provider.amd.adl")
        .to_string();

    let mut devices = Vec::new();
    for adapter in parsed
        .get("adapters")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if field_raw_string(adapter, "vendor_kind") != Some("AMD") {
            continue;
        }

        let name = field_raw_string(adapter, "name")
            .map(str::to_string)
            .unwrap_or_else(|| "AMD adapter".to_string());
        let physical_key = field_raw_string(adapter, "physical_key")
            .map(str::to_string)
            .unwrap_or_else(|| "unknown".to_string());

        devices.push(AmdDeviceView {
            name,
            provider_authority: provider_authority.clone(),
            provider_source: provider_source.clone(),
            physical_key,
            activity_pct: field_raw_u64(adapter, "activity_pct").map(|value| value as u32),
            temperature_c: field_raw_f64(adapter, "temperature_c"),
            engine_clock_mhz: field_raw_f64(adapter, "engine_clock_mhz"),
            memory_clock_mhz: field_raw_f64(adapter, "memory_clock_mhz"),
            observed_core_clock_mhz: field_raw_f64(adapter, "observed_core_clock_mhz"),
            observed_memory_clock_mhz: field_raw_f64(adapter, "observed_memory_clock_mhz"),
            vbios_version: field_raw_string(adapter, "vbios_version").map(str::to_string),
            active: field_raw_bool(adapter, "active"),
        });
    }

    Ok(devices)
}

fn collect_intel_device_views() -> Result<Vec<IntelDeviceView>, String> {
    let sample = intel_level_zero::collect_visible_sample(0);
    let tick_ts = now_ts();
    let stats = intel_level_zero::format_stats_snapshot_json(&sample, 0, &tick_ts);
    let parsed: Value = serde_json::from_str(&stats)
        .map_err(|err| format!("Intel stats JSON parse failed: {err}"))?;

    let provider_authority = parsed
        .get("provider_authority")
        .and_then(Value::as_str)
        .unwrap_or("Intel Level Zero")
        .to_string();
    let provider_source = parsed
        .get("provider_source")
        .and_then(Value::as_str)
        .unwrap_or("wtg.provider.intel.level_zero")
        .to_string();

    let mut devices = Vec::new();
    for (device_index, device) in parsed
        .get("devices")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let memory_used_mib =
            field_raw_u64(device, "sysman.memory_modules.0.used_bytes").map(bytes_to_mib);
        let memory_total_mib =
            field_raw_u64(device, "sysman.memory_modules.0.size_bytes").map(bytes_to_mib);

        let mut engine_utilizations = device
            .as_object()
            .map(|object| {
                object
                    .iter()
                    .filter_map(|(key, value)| {
                        let index_text = key
                            .strip_prefix("sysman.engine_groups.")?
                            .strip_suffix(".utilization_pct_delta")?;
                        let index = index_text.parse::<usize>().ok()?;
                        let raw = value.get("raw")?.as_f64()?;
                        Some((index, raw))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        engine_utilizations.sort_by_key(|(index, _)| *index);

        devices.push(IntelDeviceView {
            device_index,
            name: field_raw_string(device, "device_name").map(str::to_string),
            provider_authority: provider_authority.clone(),
            provider_source: provider_source.clone(),
            device_key: field_raw_string(device, "device_key")
                .map(str::to_string)
                .unwrap_or_else(|| format!("device={device_index}")),
            uuid: field_raw_string(device, "uuid").map(str::to_string),
            core_clock_mhz: field_raw_f64(device, "core_clock_mhz"),
            actual_mhz: field_raw_f64(device, "sysman.frequency_domains.0.actual_mhz"),
            request_mhz: field_raw_f64(device, "sysman.frequency_domains.0.request_mhz"),
            memory_used_mib,
            memory_total_mib,
            power_w: field_raw_f64(device, "sysman.power_domains.0.watts_delta"),
            temperature_c: temperature_reading(device),
            engine_utilizations,
        });
    }

    Ok(devices)
}

fn field_object<'a>(object: &'a Value, key: &str) -> Option<&'a Value> {
    object.get(key)
}

fn field_raw<'a>(object: &'a Value, key: &str) -> Option<&'a Value> {
    field_object(object, key)?.get("raw")
}

fn field_raw_string<'a>(object: &'a Value, key: &str) -> Option<&'a str> {
    field_raw(object, key)?.as_str()
}

fn field_raw_u64(object: &Value, key: &str) -> Option<u64> {
    field_raw(object, key)?.as_u64()
}

fn field_raw_f64(object: &Value, key: &str) -> Option<f64> {
    field_raw(object, key)?.as_f64()
}

fn field_raw_bool(object: &Value, key: &str) -> Option<bool> {
    field_raw(object, key)?.as_bool()
}

fn temperature_reading(device: &Value) -> Option<f64> {
    let object = device.as_object()?;
    for (key, value) in object.iter() {
        if !key.starts_with("sysman.temperature_sensors.") || !key.ends_with(".state") {
            continue;
        }
        if let Some(temp_c) = value
            .get("raw")
            .and_then(Value::as_object)
            .and_then(|raw| raw.get("temperature_c"))
            .and_then(Value::as_f64)
        {
            return Some(temp_c);
        }
    }
    None
}

fn bytes_to_mib(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
}

fn mw_to_w(mw: Option<u32>) -> Option<f32> {
    mw.map(|mw| mw as f32 / 1000.0)
}

fn format_refresh_label(last_refresh: Option<Instant>, unix_label: &str) -> String {
    format!(
        "Refreshed: {} | unix {}",
        format_refresh_age(last_refresh),
        unix_label
    )
}

fn format_refresh_age(last_refresh: Option<Instant>) -> String {
    let Some(last_refresh) = last_refresh else {
        return unavailable();
    };

    let elapsed = last_refresh.elapsed();
    if elapsed < Duration::from_secs(1) {
        "just now".to_string()
    } else if elapsed < Duration::from_secs(60) {
        format!("{}s ago", elapsed.as_secs())
    } else {
        let total_seconds = elapsed.as_secs();
        format!("{}m {}s ago", total_seconds / 60, total_seconds % 60)
    }
}

fn now_ts() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => format!("{}.{:03}", duration.as_secs(), duration.subsec_millis()),
        Err(_) => unavailable(),
    }
}

fn unavailable() -> String {
    "N/A".to_string()
}

#[derive(Debug, Clone)]
struct MqttFormState {
    config_path: String,
    mqtt_enabled: bool,
    host: String,
    port: String,
    username: String,
    password: String,
    password_env: String,
    topic_prefix: String,
    node_id: String,
    ha_discovery_prefix: String,
    ha_discovery_enabled: bool,
    retain_discovery: bool,
    use_password_env: bool,
}

impl Default for MqttFormState {
    fn default() -> Self {
        Self {
            config_path: ui_mqtt_settings::default_config_path(),
            mqtt_enabled: false,
            host: String::new(),
            port: ui_mqtt_settings::default_port(),
            username: String::new(),
            password: String::new(),
            password_env: String::new(),
            topic_prefix: ui_mqtt_settings::default_topic_prefix(),
            node_id: String::new(),
            ha_discovery_prefix: ui_mqtt_settings::default_ha_discovery_prefix(),
            ha_discovery_enabled: false,
            retain_discovery: false,
            use_password_env: false,
        }
    }
}

impl MqttFormState {
    fn apply_loaded_settings(&mut self, settings: LoadedMqttSettings) {
        self.mqtt_enabled = settings.mqtt_enabled;
        self.host = settings.host;
        self.port = settings.port;
        self.username = settings.username;
        self.password = settings.password;
        self.password_env = settings.password_env;
        self.topic_prefix = settings.topic_prefix;
        self.node_id = settings.node_id;
        self.ha_discovery_enabled = settings.ha_discovery_enabled;
        self.ha_discovery_prefix = settings.ha_discovery_prefix;
        self.retain_discovery = settings.retain_discovery;
        self.use_password_env =
            !self.password_env.trim().is_empty() && self.password.trim().is_empty();
    }

    fn cli_preview(&self) -> String {
        let mut parts = vec![".\\wtg.exe".to_string(), "--watch".to_string()];

        if self.mqtt_enabled {
            push_cli_flag_value(&mut parts, "--config", &self.config_path);
        } else {
            parts.push("--sink".to_string());
            parts.push("mqtt".to_string());
        }

        push_cli_flag_value(&mut parts, "--mqtt-host", &self.host);
        if self.port.trim() != ui_mqtt_settings::default_port() {
            push_cli_flag_value(&mut parts, "--mqtt-port", &self.port);
        }
        if self.topic_prefix.trim() != ui_mqtt_settings::default_topic_prefix() {
            push_cli_flag_value(&mut parts, "--mqtt-topic-prefix", &self.topic_prefix);
        }
        push_cli_flag_value(&mut parts, "--mqtt-node-id", &self.node_id);

        if !self.username.trim().is_empty() {
            push_cli_flag_value(&mut parts, "--mqtt-username", &self.username);
            if self.use_password_env {
                push_cli_flag_value(&mut parts, "--mqtt-password-env", &self.password_env);
            } else {
                push_cli_flag_value(&mut parts, "--mqtt-password", &self.password);
            }
        }

        if self.ha_discovery_enabled {
            parts.push("--mqtt-ha-discovery".to_string());
            if self.ha_discovery_prefix.trim() != ui_mqtt_settings::default_ha_discovery_prefix() {
                push_cli_flag_value(&mut parts, "--mqtt-ha-prefix", &self.ha_discovery_prefix);
            }
            if self.retain_discovery {
                parts.push("--mqtt-retain-discovery".to_string());
            }
        }

        parts.join(" ")
    }
}

fn render_mqtt_panel(ui: &mut egui::Ui, egui_ctx: &egui::Context, app: &mut WtgUiApp) {
    ui.label("Switch-to-widget adapter for existing MQTT and Home Assistant CLI/config behavior.");
    ui.add_space(6.0);

    egui::Grid::new("mqtt_settings_grid")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            row_text_edit(ui, "Config path", &mut app.mqtt_form.config_path, false);
            row_text_edit(ui, "MQTT host", &mut app.mqtt_form.host, false);
            row_text_edit(ui, "MQTT port", &mut app.mqtt_form.port, false);
            row_text_edit(ui, "MQTT username", &mut app.mqtt_form.username, false);

            ui.label("MQTT password");
            ui.add_enabled(
                !app.mqtt_form.use_password_env,
                egui::TextEdit::singleline(&mut app.mqtt_form.password).password(true),
            );
            ui.end_row();

            ui.label("MQTT password env var");
            ui.add_enabled(
                app.mqtt_form.use_password_env,
                egui::TextEdit::singleline(&mut app.mqtt_form.password_env),
            );
            ui.end_row();

            row_text_edit(
                ui,
                "MQTT topic prefix",
                &mut app.mqtt_form.topic_prefix,
                false,
            );
            row_text_edit(ui, "MQTT node ID", &mut app.mqtt_form.node_id, false);
            row_text_edit(
                ui,
                "HA discovery prefix",
                &mut app.mqtt_form.ha_discovery_prefix,
                false,
            );
        });

    ui.add_space(8.0);
    ui.checkbox(
        &mut app.mqtt_form.ha_discovery_enabled,
        "Home Assistant discovery enabled",
    );
    ui.checkbox(&mut app.mqtt_form.retain_discovery, "Retain discovery");
    ui.checkbox(
        &mut app.mqtt_form.use_password_env,
        "Use password environment variable",
    );

    let mut availability_coupled = app.mqtt_form.ha_discovery_enabled;
    ui.horizontal_wrapped(|ui| {
        ui.add_enabled(
            false,
            egui::Checkbox::new(&mut availability_coupled, "Retained availability / LWT"),
        );
        ui.small("?").on_hover_text(
            "Availability/LWT remains coupled to existing Home Assistant discovery behavior.",
        );
    });

    ui.add_space(10.0);
    render_mqtt_status_banner(ui, &app.mqtt_status);

    ui.add_space(8.0);
    ui.horizontal_wrapped(|ui| {
        if ui.button("Load config").clicked() {
            app.load_mqtt_config();
        }
        if ui.button("Save config").clicked() {
            app.save_mqtt_config();
        }
        if ui.button("Generate default config").clicked() {
            app.generate_default_config();
        }
        if ui.button("Generate/copy CLI").clicked() {
            app.copy_cli_preview(egui_ctx);
        }
        if ui.button("Test broker connection").clicked() {
            app.test_broker_connection();
        }
        if ui.button("Clear retained HA discovery").clicked() {
            app.clear_retained_ha_discovery();
        }
    });

    ui.add_space(8.0);
    ui.horizontal_wrapped(|ui| {
        if ui.button("Launch CLI MQTT publisher").clicked() {
            app.launch_cli_mqtt_publisher();
        }
        ui.small("?").on_hover_text(
            "Launch uses the saved config file.\n\
Click Save config first after editing fields.\n\
Launched publishers run in the background.",
        );
        if ui.button("Stop all wtg.exe processes").clicked() {
            app.stop_all_wtg_processes();
        }
        ui.small("?").on_hover_text(
            "Terminates any WTG CLI publisher currently running.\n\
This broadly stops all wtg.exe processes.",
        );
    });

    ui.add_space(10.0);
    ui.label("Generated CLI preview");
    let mut preview = app.mqtt_form.cli_preview();
    ui.add(
        egui::TextEdit::multiline(&mut preview)
            .font(egui::TextStyle::Monospace)
            .desired_rows(4)
            .desired_width(f32::INFINITY)
            .interactive(true),
    );
}

fn row_text_edit(ui: &mut egui::Ui, label: &str, value: &mut String, password: bool) {
    ui.label(label);
    let mut edit = egui::TextEdit::singleline(value);
    if password {
        edit = edit.password(true);
    }
    ui.add(edit);
    ui.end_row();
}

fn render_mqtt_status_banner(ui: &mut egui::Ui, status: &MqttStatus) {
    let (fill, stroke, text) = status.colors();
    egui::Frame::none()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, stroke))
        .rounding(egui::Rounding::same(6.0))
        .inner_margin(egui::Margin::symmetric(10.0, 8.0))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(text, status.label());
                ui.separator();
                ui.colored_label(text, &status.message);
            });
        });
}

fn normalized_config_path(config_path: &str) -> String {
    let trimmed = config_path.trim();
    if trimmed.is_empty() {
        ui_mqtt_settings::default_config_path()
    } else {
        trimmed.to_string()
    }
}

fn direct_password(form: &MqttFormState) -> Option<&str> {
    if form.use_password_env {
        None
    } else {
        Some(form.password.as_str())
    }
}

fn env_password(form: &MqttFormState) -> Option<&str> {
    if form.use_password_env {
        Some(form.password_env.as_str())
    } else {
        None
    }
}

fn push_cli_flag_value(parts: &mut Vec<String>, flag: &str, value: &str) {
    if value.trim().is_empty() {
        return;
    }

    parts.push(flag.to_string());
    parts.push(shell_quote(value));
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':' | '\\'))
    {
        return value.to_string();
    }

    format!("\"{}\"", value.replace('"', "\\\""))
}

fn write_default_template(path: &Path) -> Result<std::path::PathBuf, String> {
    if path
        .try_exists()
        .map_err(|e| format!("failed to inspect {}: {e}", path.display()))?
    {
        return Err(format!(
            "{} already exists; refusing to overwrite it.",
            path.display()
        ));
    }

    fs::write(path, config::config_template())
        .map_err(|e| format!("failed to create {}: {e}", path.display()))?;
    Ok(path.to_path_buf())
}

fn cli_launch_command(config_path: &Path) -> Command {
    let mut command = match resolve_wtg_cli_path() {
        Some(path) => Command::new(path),
        None => Command::new("wtg.exe"),
    };

    command
        .arg("--watch")
        .arg("--config")
        .arg(config_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    command
}

fn resolve_wtg_cli_path() -> Option<std::path::PathBuf> {
    let current_exe = std::env::current_exe().ok()?;
    let sibling = current_exe.with_file_name("wtg.exe");
    sibling.exists().then_some(sibling)
}

#[cfg(test)]
mod tests {
    use super::MqttFormState;

    #[test]
    fn cli_preview_starts_with_windows_relative_exe() {
        let preview = MqttFormState::default().cli_preview();

        assert!(preview.starts_with(".\\wtg.exe "));
    }

    #[test]
    fn cli_preview_uses_config_activation_when_mqtt_enabled() {
        let preview = MqttFormState {
            config_path: ".\\wtg.toml".to_string(),
            mqtt_enabled: true,
            host: "broker".to_string(),
            port: "1884".to_string(),
            username: "user".to_string(),
            password_env: "WTG_MQTT_PASSWORD".to_string(),
            topic_prefix: "lab".to_string(),
            node_id: "bench".to_string(),
            ha_discovery_prefix: "ha".to_string(),
            ha_discovery_enabled: true,
            retain_discovery: true,
            use_password_env: true,
            ..MqttFormState::default()
        }
        .cli_preview();

        assert!(preview.contains("--config .\\wtg.toml"));
        assert!(!preview.contains("--sink mqtt"));
        assert!(preview.contains("--mqtt-host broker"));
        assert!(preview.contains("--mqtt-port 1884"));
        assert!(preview.contains("--mqtt-username user"));
        assert!(preview.contains("--mqtt-password-env WTG_MQTT_PASSWORD"));
        assert!(preview.contains("--mqtt-topic-prefix lab"));
        assert!(preview.contains("--mqtt-node-id bench"));
        assert!(preview.contains("--mqtt-ha-discovery"));
        assert!(preview.contains("--mqtt-ha-prefix ha"));
        assert!(preview.contains("--mqtt-retain-discovery"));
    }

    #[test]
    fn cli_preview_uses_sink_activation_when_mqtt_disabled() {
        let preview = MqttFormState {
            mqtt_enabled: false,
            host: "broker".to_string(),
            port: "1884".to_string(),
            username: "user".to_string(),
            password: "secret".to_string(),
            topic_prefix: "lab".to_string(),
            node_id: "bench".to_string(),
            ha_discovery_prefix: "ha".to_string(),
            ha_discovery_enabled: true,
            retain_discovery: true,
            ..MqttFormState::default()
        }
        .cli_preview();

        assert!(preview.contains("--sink mqtt"));
        assert!(!preview.contains("--config"));
        assert!(preview.contains("--mqtt-host broker"));
        assert!(preview.contains("--mqtt-port 1884"));
        assert!(preview.contains("--mqtt-username user"));
        assert!(preview.contains("--mqtt-password secret"));
        assert!(preview.contains("--mqtt-topic-prefix lab"));
        assert!(preview.contains("--mqtt-node-id bench"));
        assert!(preview.contains("--mqtt-ha-discovery"));
        assert!(preview.contains("--mqtt-ha-prefix ha"));
        assert!(preview.contains("--mqtt-retain-discovery"));
    }
}
