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
password = ""
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
    pub(crate) password: Option<String>,
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

/// Configuration to be written to wtg.toml via --mqtt-save-config.
/// Redacts password in Debug output to prevent accidental exposure.
#[derive(Clone)]
pub(crate) struct SavedMqttConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: Option<String>,
    pub password_env: Option<String>,
    pub topic_prefix: String,
    pub node_id: String,
    pub ha_discovery: bool,
    pub ha_discovery_prefix: String,
    pub ha_retain_discovery: bool,
}

impl std::fmt::Debug for SavedMqttConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SavedMqttConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "[REDACTED]"))
            .field("password_env", &self.password_env)
            .field("topic_prefix", &self.topic_prefix)
            .field("node_id", &self.node_id)
            .field("ha_discovery", &self.ha_discovery)
            .field("ha_discovery_prefix", &self.ha_discovery_prefix)
            .field("ha_retain_discovery", &self.ha_retain_discovery)
            .finish()
    }
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

    pub(crate) fn password(&self) -> Option<&str> {
        non_blank(self.password.as_deref())
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

/// Write a SavedMqttConfig to a TOML file at the specified path.
/// Refuses to overwrite unless force is true.
pub(crate) fn write_config_file(
    config: &SavedMqttConfig,
    path: &Path,
    force: bool,
) -> Result<PathBuf, String> {
    if !force
        && path
            .try_exists()
            .map_err(|e| format!("failed to inspect {}: {e}", path.display()))?
    {
        return Err(format!(
            "{} already exists; refusing to overwrite it. Use --force-config to overwrite.",
            path.display()
        ));
    }

    let password_line = if let Some(pwd) = &config.password {
        format!("password = \"{}\"", toml_escape(pwd))
    } else {
        "password = \"\"".to_string()
    };

    let password_env_line = if let Some(env_var) = &config.password_env {
        format!("password_env = \"{}\"", toml_escape(env_var))
    } else {
        "password_env = \"\"".to_string()
    };

    let content = format!(
        r#"# WTG CLI configuration.
# WTG never auto-loads this file. Use --config <path> explicitly.
# Saved by wtg --mqtt-save-config

[mqtt]
enabled = true
host = "{}"
port = {}
username = "{}"
{}
{}
topic_prefix = "{}"
node_id = "{}"

[mqtt.home_assistant]
discovery = {}
discovery_prefix = "{}"
retain_discovery = {}
"#,
        toml_escape(&config.host),
        config.port,
        toml_escape(&config.username),
        password_line,
        password_env_line,
        toml_escape(&config.topic_prefix),
        toml_escape(&config.node_id),
        config.ha_discovery,
        toml_escape(&config.ha_discovery_prefix),
        config.ha_retain_discovery,
    );

    fs::write(path, content).map_err(|e| format!("failed to write {}: {e}", path.display()))?;
    Ok(path.to_path_buf())
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

/// Escape a string for safe inclusion in TOML string values.
/// Escapes backslashes, double quotes, newlines, carriage returns, and tabs
/// using valid TOML escape sequences.
fn toml_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
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
    fn template_contains_password_field() {
        let template = config_template();
        assert!(template.contains("password = \"\""));
    }

    #[test]
    fn template_keeps_environment_specific_values_blank() {
        let template = config_template();

        assert!(template.contains("host = \"\""));
        assert!(template.contains("username = \"\""));
        assert!(template.contains("password = \"\""));
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
password = ""
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
        assert_eq!(mqtt.password(), None);
        assert_eq!(mqtt.password_env(), Some("WTG_MQTT_PASSWORD"));
        assert_eq!(mqtt.topic_prefix(), Some("wtg"));
        assert_eq!(mqtt.node_id(), Some("bench1"));

        let ha = mqtt.home_assistant.unwrap();
        assert_eq!(ha.discovery, Some(true));
        assert_eq!(ha.discovery_prefix(), Some("homeassistant"));
        assert_eq!(ha.retain_discovery, Some(true));
    }

    #[test]
    fn config_parses_direct_password() {
        let config = parse_config_toml(
            r#"
[mqtt]
password = "test123"
"#,
        )
        .unwrap();

        let mqtt = config.mqtt.unwrap();
        assert_eq!(mqtt.password(), Some("test123"));
    }

    #[test]
    fn config_parses_empty_password_as_absent() {
        let config = parse_config_toml(
            r#"
[mqtt]
password = ""
"#,
        )
        .unwrap();

        let mqtt = config.mqtt.unwrap();
        assert_eq!(mqtt.password(), None);
    }

    #[test]
    fn empty_config_strings_are_absent() {
        let config = parse_config_toml(
            r#"
[mqtt]
host = ""
username = "  "
password = ""
password_env = ""
topic_prefix = "wtg"
node_id = ""
"#,
        )
        .unwrap();

        let mqtt = config.mqtt.unwrap();
        assert_eq!(mqtt.host(), None);
        assert_eq!(mqtt.username(), None);
        assert_eq!(mqtt.password(), None);
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

    #[test]
    fn saved_mqtt_config_debug_redacts_password() {
        let config = SavedMqttConfig {
            host: "broker".to_string(),
            port: 1883,
            username: "user".to_string(),
            password: Some("secret123".to_string()),
            password_env: None,
            topic_prefix: "wtg".to_string(),
            node_id: "test".to_string(),
            ha_discovery: false,
            ha_discovery_prefix: "homeassistant".to_string(),
            ha_retain_discovery: false,
        };

        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("[REDACTED]"));
        assert!(!debug_str.contains("secret123"));
    }

    #[test]
    fn saved_mqtt_config_debug_no_password() {
        let config = SavedMqttConfig {
            host: "broker".to_string(),
            port: 1883,
            username: String::new(),
            password: None,
            password_env: Some("VAR".to_string()),
            topic_prefix: "wtg".to_string(),
            node_id: "test".to_string(),
            ha_discovery: false,
            ha_discovery_prefix: "homeassistant".to_string(),
            ha_retain_discovery: false,
        };

        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("None"));
        assert!(!debug_str.contains("[REDACTED]"));
    }

    #[test]
    fn toml_escape_quote() {
        let result = toml_escape("test\"quote");
        assert_eq!(result, "test\\\"quote");
    }

    #[test]
    fn toml_escape_backslash() {
        let result = toml_escape("test\\backslash");
        assert_eq!(result, "test\\\\backslash");
    }

    #[test]
    fn toml_escape_newline() {
        let result = toml_escape("test\nnewline");
        assert_eq!(result, "test\\nnewline");
    }

    #[test]
    fn toml_escape_carriage_return() {
        let result = toml_escape("test\rcarriage");
        assert_eq!(result, "test\\rcarriage");
    }

    #[test]
    fn toml_escape_tab() {
        let result = toml_escape("test\ttab");
        assert_eq!(result, "test\\ttab");
    }

    #[test]
    fn toml_escape_mixed() {
        let result = toml_escape("line1\nline2\t\"quoted\"\\end");
        assert_eq!(result, "line1\\nline2\\t\\\"quoted\\\"\\\\end");
    }

    #[test]
    fn write_config_file_with_direct_password() {
        let config = SavedMqttConfig {
            host: "broker.local".to_string(),
            port: 1883,
            username: "wtg".to_string(),
            password: Some("test123".to_string()),
            password_env: None,
            topic_prefix: "wtg".to_string(),
            node_id: "bench1".to_string(),
            ha_discovery: true,
            ha_discovery_prefix: "homeassistant".to_string(),
            ha_retain_discovery: true,
        };

        let temp_path = std::env::temp_dir().join(format!("wtg_test_{}.toml", std::process::id()));
        let _ = fs::remove_file(&temp_path);

        let result = write_config_file(&config, &temp_path, false);
        assert!(result.is_ok());

        let content = fs::read_to_string(&temp_path).unwrap();
        assert!(content.contains("enabled = true"));
        assert!(content.contains("password = \"test123\""));

        let _ = fs::remove_file(&temp_path);
    }

    #[test]
    fn write_config_file_with_password_env() {
        let config = SavedMqttConfig {
            host: "broker.local".to_string(),
            port: 1883,
            username: "wtg".to_string(),
            password: None,
            password_env: Some("WTG_MQTT_PASSWORD".to_string()),
            topic_prefix: "wtg".to_string(),
            node_id: "bench1".to_string(),
            ha_discovery: false,
            ha_discovery_prefix: "homeassistant".to_string(),
            ha_retain_discovery: false,
        };

        let temp_path =
            std::env::temp_dir().join(format!("wtg_test_env_{}.toml", std::process::id()));
        let _ = fs::remove_file(&temp_path);

        let result = write_config_file(&config, &temp_path, false);
        assert!(result.is_ok());

        let content = fs::read_to_string(&temp_path).unwrap();
        assert!(content.contains("password = \"\""));
        assert!(content.contains("password_env = \"WTG_MQTT_PASSWORD\""));

        let _ = fs::remove_file(&temp_path);
    }

    #[test]
    fn write_config_file_refuses_overwrite() {
        let config = SavedMqttConfig {
            host: "broker.local".to_string(),
            port: 1883,
            username: String::new(),
            password: None,
            password_env: None,
            topic_prefix: "wtg".to_string(),
            node_id: "monitor".to_string(),
            ha_discovery: false,
            ha_discovery_prefix: "homeassistant".to_string(),
            ha_retain_discovery: false,
        };

        let temp_path =
            std::env::temp_dir().join(format!("wtg_test_overwrite_{}.toml", std::process::id()));
        let _ = fs::remove_file(&temp_path);

        let result1 = write_config_file(&config, &temp_path, false);
        assert!(result1.is_ok());

        let result2 = write_config_file(&config, &temp_path, false);
        assert!(result2.is_err());
        assert!(result2.unwrap_err().contains("already exists"));

        let _ = fs::remove_file(&temp_path);
    }

    #[test]
    fn write_config_file_allows_overwrite_with_force() {
        let config1 = SavedMqttConfig {
            host: "broker1.local".to_string(),
            port: 1883,
            username: String::new(),
            password: None,
            password_env: None,
            topic_prefix: "wtg".to_string(),
            node_id: "node1".to_string(),
            ha_discovery: false,
            ha_discovery_prefix: "homeassistant".to_string(),
            ha_retain_discovery: false,
        };

        let config2 = SavedMqttConfig {
            host: "broker2.local".to_string(),
            port: 1883,
            username: String::new(),
            password: None,
            password_env: None,
            topic_prefix: "wtg".to_string(),
            node_id: "node2".to_string(),
            ha_discovery: false,
            ha_discovery_prefix: "homeassistant".to_string(),
            ha_retain_discovery: false,
        };

        let temp_path =
            std::env::temp_dir().join(format!("wtg_test_force_{}.toml", std::process::id()));
        let _ = fs::remove_file(&temp_path);

        let result1 = write_config_file(&config1, &temp_path, false);
        assert!(result1.is_ok());

        let result2 = write_config_file(&config2, &temp_path, true);
        assert!(result2.is_ok());

        let content = fs::read_to_string(&temp_path).unwrap();
        assert!(content.contains("broker2.local"));

        let _ = fs::remove_file(&temp_path);
    }

    #[test]
    fn write_config_file_sets_enabled_true() {
        let config = SavedMqttConfig {
            host: "broker.local".to_string(),
            port: 1883,
            username: String::new(),
            password: None,
            password_env: None,
            topic_prefix: "wtg".to_string(),
            node_id: "test".to_string(),
            ha_discovery: false,
            ha_discovery_prefix: "homeassistant".to_string(),
            ha_retain_discovery: false,
        };

        let temp_path =
            std::env::temp_dir().join(format!("wtg_test_enabled_{}.toml", std::process::id()));
        let _ = fs::remove_file(&temp_path);

        let result = write_config_file(&config, &temp_path, false);
        assert!(result.is_ok());

        let content = fs::read_to_string(&temp_path).unwrap();
        assert!(content.contains("enabled = true"));

        let _ = fs::remove_file(&temp_path);
    }

    #[test]
    fn write_config_file_includes_ha_settings() {
        let config = SavedMqttConfig {
            host: "broker.local".to_string(),
            port: 1883,
            username: String::new(),
            password: None,
            password_env: None,
            topic_prefix: "wtg".to_string(),
            node_id: "test".to_string(),
            ha_discovery: true,
            ha_discovery_prefix: "custom_ha".to_string(),
            ha_retain_discovery: true,
        };

        let temp_path =
            std::env::temp_dir().join(format!("wtg_test_ha_{}.toml", std::process::id()));
        let _ = fs::remove_file(&temp_path);

        let result = write_config_file(&config, &temp_path, false);
        assert!(result.is_ok());

        let content = fs::read_to_string(&temp_path).unwrap();
        assert!(content.contains("discovery = true"));
        assert!(content.contains("discovery_prefix = \"custom_ha\""));

        let _ = fs::remove_file(&temp_path);
    }
}
