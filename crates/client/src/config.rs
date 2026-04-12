use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
pub struct ClientConfig {
    pub server_url: String,
    pub language: String,
    pub username: Option<String>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            server_url: "wss://localhost:8443".to_string(),
            language: "en".to_string(),
            username: None,
        }
    }
}

#[allow(dead_code)] // save() used in later phases
impl ClientConfig {
    /// Load config from file, or create default if not found.
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(content) => toml::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Save config to file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self).expect("config serialization failed");
        std::fs::write(path, content)
    }

    /// Get the config directory (~/.gamecenter/)
    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("gamecenter")
    }

    /// Full path to the config file.
    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_config_values() {
        let config = ClientConfig::default();
        assert_eq!(config.server_url, "wss://localhost:8443");
        assert_eq!(config.language, "en");
        assert!(config.username.is_none());
    }

    #[test]
    fn save_and_load_config() {
        let dir = std::env::temp_dir().join("gc-test-config");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test-config.toml");

        let config = ClientConfig {
            server_url: "wss://example.com:9443".to_string(),
            language: "fr".to_string(),
            username: Some("testuser".to_string()),
        };
        config.save(&path).unwrap();

        let loaded = ClientConfig::load(&path);
        assert_eq!(loaded.server_url, "wss://example.com:9443");
        assert_eq!(loaded.language, "fr");
        assert_eq!(loaded.username.as_deref(), Some("testuser"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_missing_file_returns_default() {
        let config = ClientConfig::load(Path::new("/nonexistent/path.toml"));
        assert_eq!(config.language, "en");
    }
}
