//! App configuration (app_id, app_secret, redirect_uri, data_dir)

use crate::error::{FeishuCoreError as Error, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt as _;

/// Application configuration (app credentials)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub app_id: String,
    pub app_secret: String,
    #[serde(default = "default_redirect_uri")]
    pub redirect_uri: String,
    #[serde(default)]
    pub data_dir: PathBuf,
}

fn default_redirect_uri() -> String {
    "http://localhost:8765/callback".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            app_id: String::new(),
            app_secret: String::new(),
            redirect_uri: default_redirect_uri(),
            data_dir: dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("feishu-export"),
        }
    }
}

/// Persistent config store (writes to data_dir/config.toml)
pub struct ConfigStore {
    config_path: PathBuf,
}

impl ConfigStore {
    pub fn new() -> Self {
        let data_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("feishu-export");

        Self {
            config_path: data_dir.join("config.toml"),
        }
    }

    pub fn config_path(&self) -> &PathBuf {
        &self.config_path
    }

    pub async fn save(&self, config: &AppConfig) -> Result<()> {
        let toml_str =
            toml::to_string_pretty(config).map_err(|e| Error::StorageError(format!("serialize: {}", e)))?;

        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| Error::StorageError(format!("mkdir: {}", e)))?;
        }

        let mut file = fs::File::create(&self.config_path)
            .await
            .map_err(|e| Error::StorageError(format!("create config file: {}", e)))?;

        file.write_all(toml_str.as_bytes())
            .await
            .map_err(|e| Error::StorageError(format!("write config: {}", e)))?;

        file.flush()
            .await
            .map_err(|e| Error::StorageError(format!("flush: {}", e)))?;

        Ok(())
    }

    pub async fn load(&self) -> Result<AppConfig> {
        if !fs::try_exists(&self.config_path)
            .await
            .map_err(|e| Error::StorageError(format!("check config exists: {}", e)))?
        {
            return Err(Error::ConfigNotFound);
        }

        let contents = fs::read_to_string(&self.config_path)
            .await
            .map_err(|e| Error::StorageError(format!("read: {}", e)))?;

        let config: AppConfig = toml::from_str(&contents)
            .map_err(|e| Error::StorageError(format!("parse TOML: {}", e)))?;

        if config.app_id.is_empty() || config.app_secret.is_empty() {
            return Err(Error::ConfigError("app_id or app_secret is empty".to_string()));
        }

        Ok(config)
    }

    pub async fn clear(&self) -> Result<()> {
        if fs::try_exists(&self.config_path)
            .await
            .map_err(|e| Error::StorageError(format!("check: {}", e)))?
        {
            fs::remove_file(&self.config_path)
                .await
                .map_err(|e| Error::StorageError(format!("remove: {}", e)))?;
        }
        Ok(())
    }

    pub async fn set_credentials(&self, app_id: &str, app_secret: String) -> Result<()> {
        let config = match self.load().await {
            Ok(c) => c,
            Err(Error::ConfigNotFound) => AppConfig::default(),
            Err(e) => return Err(e),
        };

        let config = AppConfig {
            app_id: app_id.to_string(),
            app_secret,
            ..config
        };
        self.save(&config).await
    }
}
