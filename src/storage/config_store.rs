use crate::error::{FeishuError, Result};
use crate::models::auth::TokenData;
use crate::models::export::ExportCache;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// 应用配置
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
        let data_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("feishu-export");

        Self {
            app_id: String::new(),
            app_secret: String::new(),
            redirect_uri: default_redirect_uri(),
            data_dir,
        }
    }
}

/// 配置存储
pub struct ConfigStore {
    config_path: PathBuf,
}

impl ConfigStore {
    /// 创建新的配置存储
    pub fn new() -> Result<Self> {
        let data_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("feishu-export");

        // 确保目录存在
        fs::create_dir_all(&data_dir).map_err(|e| {
            FeishuError::StorageError(format!("Failed to create config dir: {}", e))
        })?;

        let config_path = data_dir.join("config.toml");
        Ok(Self { config_path })
    }

    /// 获取配置文件路径
    pub fn config_path(&self) -> &PathBuf {
        &self.config_path
    }

    /// 保存配置
    pub fn save(&self, config: &AppConfig) -> Result<()> {
        let toml_str = toml::to_string_pretty(config)
            .map_err(|e| FeishuError::StorageError(format!("Failed to serialize config: {}", e)))?;

        fs::write(&self.config_path, toml_str)
            .map_err(|e| FeishuError::StorageError(format!("Failed to write config: {}", e)))?;

        Ok(())
    }

    /// 加载配置
    pub fn load(&self) -> Result<AppConfig> {
        if !self.config_path.exists() {
            return Err(FeishuError::ConfigNotFound);
        }

        let contents = fs::read_to_string(&self.config_path)
            .map_err(|e| FeishuError::StorageError(format!("Failed to read config: {}", e)))?;

        let config: AppConfig = toml::from_str(&contents)
            .map_err(|e| FeishuError::StorageError(format!("Failed to parse config: {}", e)))?;

        // 检查配置是否有效
        if config.app_id.is_empty() || config.app_secret.is_empty() {
            return Err(FeishuError::ConfigError(
                "App ID or App Secret is empty".to_string(),
            ));
        }

        Ok(config)
    }

    /// 清除配置
    pub fn clear(&self) -> Result<()> {
        if self.config_path.exists() {
            fs::remove_file(&self.config_path).map_err(|e| {
                FeishuError::StorageError(format!("Failed to remove config: {}", e))
            })?;
        }
        Ok(())
    }

    /// 设置应用凭证
    pub fn set_credentials(&self, app_id: &str, app_secret: String) -> Result<()> {
        let mut config = self.load().unwrap_or_default();
        config.app_id = app_id.to_string();
        config.app_secret = app_secret;
        self.save(&config)
    }
}
