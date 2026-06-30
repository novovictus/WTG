// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Adam Hooper

use std::env;

use crate::config::SavedMqttConfig;
use crate::mqtt::{
    MqttAuthOptions, MqttHaDiscoveryOptions, MqttOptions, DEFAULT_HA_DISCOVERY_PREFIX,
    DEFAULT_MQTT_PORT, DEFAULT_TOPIC_PREFIX,
};

pub(crate) fn validate_mqtt_auth_combination(
    username: Option<&str>,
    password: Option<&str>,
    password_env: Option<&str>,
) -> Result<(), String> {
    let has_username = non_empty(username);
    let has_password = non_empty(password);
    let has_password_env = non_empty(password_env);

    match (has_username, has_password, has_password_env) {
        (false, false, false) => Ok(()),
        (true, true, false) => Ok(()),
        (true, false, true) => Ok(()),
        (true, false, false) => {
            Err("--mqtt-username requires --mqtt-password or --mqtt-password-env.".to_string())
        }
        (false, true, false) => Err("--mqtt-password requires --mqtt-username.".to_string()),
        (false, false, true) => Err("--mqtt-password-env requires --mqtt-username.".to_string()),
        (true, true, true) => {
            Err("--mqtt-password and --mqtt-password-env cannot be used together.".to_string())
        }
        (false, true, true) => {
            Err("--mqtt-password and --mqtt-password-env cannot be used together.".to_string())
        }
    }
}

pub(crate) fn saved_mqtt_config_from_values(
    host: Option<&str>,
    port: Option<&str>,
    username: Option<&str>,
    password: Option<&str>,
    password_env: Option<&str>,
    topic_prefix: Option<&str>,
    node_id: Option<&str>,
    ha_discovery: bool,
    ha_prefix: Option<&str>,
    retain_discovery: bool,
) -> Result<SavedMqttConfig, String> {
    validate_mqtt_auth_combination(username, password, password_env)?;

    if !non_empty(host) {
        return Err("--mqtt-save-config requires --mqtt-host <host>.".to_string());
    }
    if !non_empty(node_id) {
        return Err("--mqtt-save-config requires --mqtt-node-id <id>.".to_string());
    }
    if ha_prefix.is_some() && !ha_discovery {
        return Err("--mqtt-ha-prefix requires --mqtt-ha-discovery.".to_string());
    }
    if retain_discovery && !ha_discovery {
        return Err("--mqtt-retain-discovery requires --mqtt-ha-discovery.".to_string());
    }

    Ok(SavedMqttConfig {
        host: host.unwrap_or_default().trim().to_string(),
        port: parse_port_value(port)?,
        username: username.unwrap_or_default().trim().to_string(),
        password: trim_to_option(password),
        password_env: trim_to_option(password_env),
        topic_prefix: topic_prefix
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_TOPIC_PREFIX)
            .to_string(),
        node_id: node_id.unwrap_or_default().trim().to_string(),
        ha_discovery,
        ha_discovery_prefix: ha_prefix
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_HA_DISCOVERY_PREFIX)
            .to_string(),
        ha_retain_discovery: retain_discovery,
    })
}

pub(crate) fn mqtt_options_from_values(
    active: bool,
    host: Option<&str>,
    port: Option<&str>,
    topic_prefix: Option<&str>,
    node_id: Option<&str>,
    username: Option<&str>,
    password: Option<&str>,
    password_env: Option<&str>,
    ha_discovery: bool,
    ha_remove_discovery: bool,
    ha_prefix: Option<&str>,
    retain_discovery: bool,
) -> Result<Option<MqttOptions>, String> {
    if !active {
        return Ok(None);
    }

    let auth = mqtt_auth_from_values(username, password, password_env)?;
    let ha_discovery = if ha_discovery || ha_remove_discovery {
        let prefix = ha_prefix
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_HA_DISCOVERY_PREFIX)
            .to_string();
        Some(MqttHaDiscoveryOptions::new(prefix, retain_discovery)?)
    } else {
        None
    };

    Ok(Some(MqttOptions::new(
        host.unwrap_or_default().to_string(),
        parse_port_value(port)?,
        topic_prefix
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_TOPIC_PREFIX)
            .to_string(),
        node_id.unwrap_or_default().to_string(),
        auth,
        ha_discovery.is_some() && !ha_remove_discovery,
        ha_discovery,
    )?))
}

pub(crate) fn mqtt_auth_from_values(
    username: Option<&str>,
    password: Option<&str>,
    password_env: Option<&str>,
) -> Result<Option<MqttAuthOptions>, String> {
    validate_mqtt_auth_combination(username, password, password_env)?;

    match (
        trim_to_option(username),
        trim_to_option(password),
        trim_to_option(password_env),
    ) {
        (Some(username), Some(password), None) => {
            Ok(Some(MqttAuthOptions::new(username, password)?))
        }
        (Some(username), None, Some(password_env)) => {
            Ok(Some(resolve_mqtt_auth(&username, &password_env, |name| {
                env::var(name).ok()
            })?))
        }
        _ => Ok(None),
    }
}

pub(crate) fn resolve_mqtt_auth<F>(
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

fn parse_port_value(port: Option<&str>) -> Result<u16, String> {
    match trim_to_option(port) {
        Some(value) => value
            .parse::<u16>()
            .map_err(|_| format!("--mqtt-port must be a TCP port number. Got: {value}")),
        None => Ok(DEFAULT_MQTT_PORT),
    }
}

fn trim_to_option(value: Option<&str>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn non_empty(value: Option<&str>) -> bool {
    value.map(str::trim).unwrap_or_default().len() > 0
}
