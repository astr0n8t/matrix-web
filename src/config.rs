use serde::{Deserialize, Serialize};
use std::env;

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
        let mut config: Config = serde_yaml::from_str(&content)?;
        
        // Override with environment variables if present
        config.apply_env_overrides();
        
        Ok(config)
    }
    
    fn apply_env_overrides(&mut self) {
        // Matrix configuration
        if let Ok(val) = env::var("MATRIX_HOMESERVER") {
            self.homeserver = val;
        }
        if let Ok(val) = env::var("MATRIX_USERNAME") {
            self.username = val;
        }
        if let Ok(val) = env::var("MATRIX_PASSWORD") {
            self.password = val;
        }
        if let Ok(val) = env::var("MATRIX_ROOM_ID") {
            self.room_id = val;
        }
        
        // Web configuration
        if let Ok(val) = env::var("WEB_HOST") {
            self.web.host = val;
        }
        if let Ok(val) = env::var("WEB_PORT") {
            if let Ok(port) = val.parse::<u16>() {
                self.web.port = port;
            }
        }
        
        // Authentication configuration
        if let Ok(header_name) = env::var("WEB_AUTH_HEADER_NAME") {
            if let Ok(header_value) = env::var("WEB_AUTH_HEADER_VALUE") {
                self.web.auth = Some(AuthConfig {
                    header_name,
                    header_value,
                });
            }
        }
        
        // Message history configuration
        if let Ok(val) = env::var("MESSAGE_HISTORY_LIMIT") {
            if let Ok(limit) = val.parse::<usize>() {
                self.message_history.limit = limit;
            }
        }
    }
}
