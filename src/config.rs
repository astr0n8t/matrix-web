use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub homeserver: String,
    pub username: String,
    pub password: String,
    pub room_id: String,
    pub web: WebConfig,
    #[serde(default)]
    pub message_history: MessageHistoryConfig,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WebConfig {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub auth: Option<AuthConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AuthConfig {
    pub header_name: String,
    pub header_value: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MessageHistoryConfig {
    #[serde(default = "default_history_limit")]
    pub limit: usize,
}

impl Default for MessageHistoryConfig {
    fn default() -> Self {
        Self {
            limit: default_history_limit(),
        }
    }
}

fn default_history_limit() -> usize {
    50
}

impl Config {
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }
}
