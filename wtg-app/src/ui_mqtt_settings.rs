// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Adam Hooper

use crate::config::{self, WtgConfig};
use crate::mqtt::{DEFAULT_HA_DISCOVERY_PREFIX, DEFAULT_MQTT_PORT, DEFAULT_TOPIC_PREFIX};

pub(crate) fn default_config_path() -> String {
    config::DEFAULT_CONFIG_FILE_NAME.to_string()
}

pub(crate) fn default_port() -> String {
    DEFAULT_MQTT_PORT.to_string()
}

pub(crate) fn default_topic_prefix() -> String {
    DEFAULT_TOPIC_PREFIX.to_string()
}

pub(crate) fn default_ha_discovery_prefix() -> String {
    DEFAULT_HA_DISCOVERY_PREFIX.to_string()
}

#[derive(Debug, Clone, Default)]
pub(crate) struct LoadedMqttSettings {
    pub(crate) mqtt_enabled: bool,
    pub(crate) host: String,
    pub(crate) port: String,
    pub(crate) username: String,
    pub(crate) password: String,
    pub(crate) password_env: String,
    pub(crate) topic_prefix: String,
    pub(crate) node_id: String,
    pub(crate) ha_discovery_enabled: bool,
    pub(crate) ha_discovery_prefix: String,
    pub(crate) retain_discovery: bool,
}

pub(crate) fn mqtt_settings_from_config(config: &WtgConfig) -> LoadedMqttSettings {
    let mut settings = LoadedMqttSettings {
        port: default_port(),
        topic_prefix: default_topic_prefix(),
        ha_discovery_prefix: default_ha_discovery_prefix(),
        ..LoadedMqttSettings::default()
    };

    let Some(mqtt) = config.mqtt.as_ref() else {
        return settings;
    };

    settings.mqtt_enabled = mqtt.enabled();
    settings.host = mqtt.host().unwrap_or_default().to_string();
    settings.port = mqtt.port.unwrap_or(DEFAULT_MQTT_PORT).to_string();
    settings.username = mqtt.username().unwrap_or_default().to_string();
    settings.password = mqtt.password().unwrap_or_default().to_string();
    settings.password_env = mqtt.password_env().unwrap_or_default().to_string();
    settings.topic_prefix = mqtt
        .topic_prefix()
        .unwrap_or(DEFAULT_TOPIC_PREFIX)
        .to_string();
    settings.node_id = mqtt.node_id().unwrap_or_default().to_string();

    if let Some(ha) = mqtt.home_assistant.as_ref() {
        settings.ha_discovery_enabled = ha.discovery.unwrap_or(false);
        settings.ha_discovery_prefix = ha
            .discovery_prefix()
            .unwrap_or(DEFAULT_HA_DISCOVERY_PREFIX)
            .to_string();
        settings.retain_discovery = ha.retain_discovery.unwrap_or(false);
    }

    settings
}
