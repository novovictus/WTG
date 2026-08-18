#![cfg_attr(windows, windows_subsystem = "windows")]

// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Adam Hooper

fn main() {
    tracing_subscriber::fmt::init();

    if let Err(err) = wtg_app::ui::run() {
        eprintln!("WTG UI failed: {err}");
        std::process::exit(wtg_core::exit_code_for_status("error"));
    }
}
