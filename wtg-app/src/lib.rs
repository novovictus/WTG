// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Adam Hooper

pub mod ui;

mod config;
mod mqtt;
mod mqtt_settings;

fn now_ts() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => format!("{}.{:03}", duration.as_secs(), duration.subsec_millis()),
        Err(_) => "N/A".to_string(),
    }
}

fn bytes_to_mib(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
}

fn mw_to_w(mw: Option<u32>) -> Option<f32> {
    mw.map(|mw| mw as f32 / 1000.0)
}
