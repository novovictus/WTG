// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Adam Hooper

use std::time::{Duration, Instant};

use eframe::egui;
use wtg_core::nvml::{
    probe_context::{query_probe_context_for_gpu_with_ctx, GpuProbeContext},
    GpuSnapshot, NvmlContext,
};

const DEFAULT_REFRESH_MS: u64 = 1000;
const MIN_REFRESH_MS: u64 = 250;
const MAX_REFRESH_MS: u64 = 5_000;

pub(crate) fn run() -> Result<(), eframe::Error> {
    eframe::run_native(
        "WTG",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Box::new(WtgUiApp::new())),
    )
}

struct WtgUiApp {
    nvml_ctx: Option<NvmlContext>,
    devices: Vec<DeviceView>,
    selected_device: usize,
    refresh_interval_ms: u64,
    running: bool,
    last_refresh: Option<Instant>,
    last_refresh_label: String,
    last_error: Option<String>,
}

impl WtgUiApp {
    fn new() -> Self {
        let mut app = Self {
            nvml_ctx: None,
            devices: Vec::new(),
            selected_device: 0,
            refresh_interval_ms: DEFAULT_REFRESH_MS,
            running: true,
            last_refresh: None,
            last_refresh_label: "N/A".to_string(),
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
        if self.nvml_ctx.is_none() {
            match wtg_core::nvml::init_context() {
                Ok(ctx) => {
                    self.nvml_ctx = Some(ctx);
                    self.last_error = None;
                }
                Err(err) => {
                    self.last_error = Some(format!("NVML init failed: {err}"));
                    self.mark_refreshed();
                    return;
                }
            }
        }

        let Some(ctx) = self.nvml_ctx.as_ref() else {
            return;
        };

        match wtg_core::nvml::snapshot_all_with_ctx(ctx) {
            Ok(snaps) => {
                self.devices = snaps
                    .iter()
                    .map(|snapshot| {
                        let context = query_probe_context_for_gpu_with_ctx(ctx, snapshot.index);
                        DeviceView::from_snapshot(snapshot, context)
                    })
                    .collect();

                if self.selected_device >= self.devices.len() {
                    self.selected_device = self.devices.len().saturating_sub(1);
                }

                self.last_error = None;
            }
            Err(err) => {
                self.last_error = Some(format!("Telemetry refresh failed: {err}"));
                self.nvml_ctx = None;
            }
        }

        self.mark_refreshed();
    }

    fn mark_refreshed(&mut self) {
        self.last_refresh = Some(Instant::now());
        self.last_refresh_label = now_ts();
    }
}

impl eframe::App for WtgUiApp {
    fn update(&mut self, egui_ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.running && self.refresh_due() {
            self.refresh();
        }

        egui::TopBottomPanel::top("toolbar").show(egui_ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("WTG");

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
                ui.label(format!("Last: {}", self.last_refresh_label));
            });

            if let Some(error) = &self.last_error {
                ui.colored_label(egui::Color32::YELLOW, error);
            }
        });

        egui::SidePanel::left("devices")
            .resizable(true)
            .default_width(230.0)
            .show(egui_ctx, |ui| {
                ui.heading("Devices");
                ui.separator();

                if self.devices.is_empty() {
                    ui.label("No GPU telemetry available");
                    return;
                }

                for (idx, device) in self.devices.iter().enumerate() {
                    let label = format!("GPU {}: {}", device.index, device.name);
                    if ui
                        .selectable_label(self.selected_device == idx, label)
                        .clicked()
                    {
                        self.selected_device = idx;
                    }
                }
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

struct DeviceView {
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

impl DeviceView {
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
    ui.heading(format!("GPU {}: {}", device.index, device.name));
    ui.add_space(8.0);

    egui::Grid::new("device_summary")
        .num_columns(2)
        .spacing([20.0, 8.0])
        .striped(true)
        .show(ui, |ui| {
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

fn row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(label);
    ui.monospace(value);
    ui.end_row();
}

fn progress_bar(percent: u32, label: &str) -> egui::ProgressBar {
    let clamped = percent.min(100);
    egui::ProgressBar::new(clamped as f32 / 100.0).text(format!("{label}: {percent}%"))
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

fn format_power(power_w: Option<f32>, power_limit_w: Option<f32>) -> String {
    match (power_w, power_limit_w) {
        (Some(power_w), Some(power_limit_w)) => format!("{power_w:.1} W / {power_limit_w:.1} W"),
        (Some(power_w), None) => format!("{power_w:.1} W / N/A"),
        (None, Some(power_limit_w)) => format!("N/A / {power_limit_w:.1} W"),
        (None, None) => unavailable(),
    }
}

fn bytes_to_mib(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
}

fn mw_to_w(mw: Option<u32>) -> Option<f32> {
    mw.map(|mw| mw as f32 / 1000.0)
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
