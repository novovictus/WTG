// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Adam Hooper

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

pub(crate) const DEFAULT_CONFIG_FILE_NAME: &str = "wtg.toml";

const CONFIG_TEMPLATE: &str = r#"# WTG CLI configuration.
# WTG never auto-loads this file. Use --config <path> explicitly.
# Leave environment-specific values blank until you are ready to use them.

[mqtt]
enabled = false
host = ""
port = 1883
username = ""
password_env = ""
topic_prefix = "wtg"
node_id = ""

[mqtt.home_assistant]
discovery = false
discovery_prefix = "homeassistant"
retain_discovery = true
"#;

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct WtgConfig {
    pub(crate) mqtt: Option<MqttConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct MqttConfig {
    pub(crate) enabled: Option<bool>,
    pub(crate) host: Option<String>,
    pub(crate) port: Option<u16>,
    pub(crate) username: Option<String>,
    pub(crate) password_env: Option<String>,
    pub(crate) topic_prefix: Option<String>,
    pub(crate) node_id: Option<String>,
    pub(crate) home_assistant: Option<HomeAssistantConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct HomeAssistantConfig {
    pub(crate) discovery: Option<bool>,
    pub(crate) discovery_prefix: Option<String>,
    pub(crate) retain_discovery: Option<bool>,
}

impl MqttConfig {
    pub(crate) fn enabled(&self) -> bool {
        self.enabled.unwrap_or(false)
    }

    pub(crate) fn host(&self) -> Option<&str> {
        non_blank(self.host.as_deref())
    }

    pub(crate) fn username(&self) -> Option<&str> {
        non_blank(self.username.as_deref())
    }

    pub(crate) fn password_env(&self) -> Option<&str> {
        non_blank(self.password_env.as_deref())
    }

    pub(crate) fn topic_prefix(&self) -> Option<&str> {
        non_blank(self.topic_prefix.as_deref())
    }

    pub(crate) fn node_id(&self) -> Option<&str> {
        non_blank(self.node_id.as_deref())
    }
}

impl HomeAssistantConfig {
    pub(crate) fn discovery_prefix(&self) -> Option<&str> {
        non_blank(self.discovery_prefix.as_deref())
    }
}

pub(crate) fn config_template() -> &'static str {
    CONFIG_TEMPLATE
}

pub(crate) fn create_default_config_file() -> Result<PathBuf, String> {
    let path = PathBuf::from(DEFAULT_CONFIG_FILE_NAME);
    if path
        .try_exists()
        .map_err(|e| format!("failed to inspect {DEFAULT_CONFIG_FILE_NAME}: {e}"))?
    {
        return Err(format!(
            "{DEFAULT_CONFIG_FILE_NAME} already exists; refusing to overwrite it."
        ));
    }

    fs::write(&path, config_template())
        .map_err(|e| format!("failed to create {DEFAULT_CONFIG_FILE_NAME}: {e}"))?;
    Ok(path)
}

pub(crate) fn load_config_file(path: &Path) -> Result<WtgConfig, String> {
    let text = fs::read_to_string(path)
        .map_err(|e| format!("failed to read config file {}: {e}", path.display()))?;
    parse_config_toml(&text)
        .map_err(|e| format!("failed to parse config file {}: {e}", path.display()))
}

pub(crate) fn parse_config_toml(text: &str) -> Result<WtgConfig, String> {
    toml::from_str(text).map_err(|e| e.to_string())
}

fn non_blank(value: Option<&str>) -> Option<&str> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_contains_mqtt_sections() {
        let template = config_template();

        assert!(template.contains("[mqtt]"));
        assert!(template.contains("[mqtt.home_assistant]"));
    }

    #[test]
    fn template_keeps_environment_specific_values_blank() {
        let template = config_template();

        assert!(template.contains("host = \"\""));
        assert!(template.contains("username = \"\""));
        assert!(template.contains("password_env = \"\""));
        assert!(template.contains("node_id = \"\""));
    }

    #[test]
    fn valid_config_parse_succeeds() {
        let config = parse_config_toml(
            r#"
[mqtt]
enabled = true
host = "broker"
port = 1883
username = "wtg"
password_env = "WTG_MQTT_PASSWORD"
topic_prefix = "wtg"
node_id = "bench1"

[mqtt.home_assistant]
discovery = true
discovery_prefix = "homeassistant"
retain_discovery = true
"#,
        )
        .unwrap();

        let mqtt = config.mqtt.unwrap();
        assert!(mqtt.enabled());
        assert_eq!(mqtt.host(), Some("broker"));
        assert_eq!(mqtt.port, Some(1883));
        assert_eq!(mqtt.username(), Some("wtg"));
        assert_eq!(mqtt.password_env(), Some("WTG_MQTT_PASSWORD"));
        assert_eq!(mqtt.topic_prefix(), Some("wtg"));
        assert_eq!(mqtt.node_id(), Some("bench1"));

        let ha = mqtt.home_assistant.unwrap();
        assert_eq!(ha.discovery, Some(true));
        assert_eq!(ha.discovery_prefix(), Some("homeassistant"));
        assert_eq!(ha.retain_discovery, Some(true));
    }

    #[test]
    fn empty_config_strings_are_absent() {
        let config = parse_config_toml(
            r#"
[mqtt]
host = ""
username = "  "
password_env = ""
topic_prefix = "wtg"
node_id = ""
"#,
        )
        .unwrap();

        let mqtt = config.mqtt.unwrap();
        assert_eq!(mqtt.host(), None);
        assert_eq!(mqtt.username(), None);
        assert_eq!(mqtt.password_env(), None);
        assert_eq!(mqtt.topic_prefix(), Some("wtg"));
        assert_eq!(mqtt.node_id(), None);
    }

    #[test]
    fn missing_config_path_errors_cleanly() {
        let err = load_config_file(Path::new("__wtg_missing_config_for_test__.toml")).unwrap_err();

        assert!(err.contains("failed to read config file"));
        assert!(err.contains("__wtg_missing_config_for_test__.toml"));
    }
}
