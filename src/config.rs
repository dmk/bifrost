use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub listeners: HashMap<String, ListenerConfig>,
    pub backends: HashMap<String, BackendConfig>,
    pub routes: HashMap<String, RouteConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListenerConfig {
    pub bind: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BackendConfig {
    #[serde(rename = "type")]
    pub backend_type: String,
    pub server: String, // Single server for now - keep it simple
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RouteConfig {
    pub matcher: String,
    pub backend: String, // Only blind forward for now
}

impl Config {
    /// Load configuration from a YAML file
    pub async fn from_yaml_file(path: &str) -> Result<Self, ConfigError> {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| ConfigError::IoError(e.to_string()))?;

        let config: Config = serde_yaml::from_str(&content)
            .map_err(|e| ConfigError::ParseError(e.to_string()))?;

        Ok(config)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("File not found: {0}")]
    FileNotFound(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("IO error: {0}")]
    IoError(String),
}