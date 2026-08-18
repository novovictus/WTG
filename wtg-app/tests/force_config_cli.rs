// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Adam Hooper

use std::process::Command;

#[test]
fn force_config_is_rejected_without_save_config() {
    for args in [
        ["--force-config", "--watch"].as_slice(),
        ["--force-config", "--once"].as_slice(),
        ["--mqtt-init-config", "--force-config"].as_slice(),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_wtg"))
            .args(args)
            .output()
            .unwrap();

        assert_eq!(output.status.code(), Some(1), "args: {args:?}");

        let stderr = String::from_utf8(output.stderr).unwrap();
        assert_eq!(
            stderr.trim(),
            "WTG usage error: --force-config is valid only with --mqtt-save-config.",
            "args: {args:?}"
        );
    }
}
