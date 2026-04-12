use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub bind_addr: String,
    pub tls: TlsConfig,
    pub auth: AuthConfig,
    pub database: DatabaseConfig,
    pub log_level: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TlsConfig {
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
}

#[derive(Debug, Deserialize)]
pub struct AuthConfig {
    pub jwt_secret_env: String,
    pub token_expiry_secs: i64,
}

#[derive(Debug, Deserialize)]
pub struct DatabaseConfig {
    pub path: PathBuf,
}

impl ServerConfig {
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: ServerConfig = toml::from_str(&content)?;
        Ok(config)
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:8443".to_string(),
            tls: TlsConfig {
                cert_path: PathBuf::from("certs/server.crt"),
                key_path: PathBuf::from("certs/server.key"),
            },
            auth: AuthConfig {
                jwt_secret_env: "GC_JWT_SECRET".to_string(),
                token_expiry_secs: 7 * 24 * 3600, // 7 days
            },
            database: DatabaseConfig {
                path: PathBuf::from("gamecenter.db"),
            },
            log_level: Some("info".to_string()),
        }
    }
}
