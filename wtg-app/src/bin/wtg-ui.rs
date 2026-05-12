// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Adam Hooper

#[path = "../ui.rs"]
mod ui;

fn main() {
    tracing_subscriber::fmt::init();

    if let Err(err) = ui::run() {
        eprintln!("WTG UI failed: {err}");
        std::process::exit(2);
    }
}
