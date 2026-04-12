use crate::error::{FeishuError, Result};
use crate::models::auth::TokenData;
use serde_json;
use std::fs;
use std::path::PathBuf;

/// Token 存储
pub struct TokenStore {
    token_path: PathBuf,
}

impl TokenStore {
    /// 创建新的 Token 存储
    pub fn new() -> Result<Self> {
        let data_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("feishu-export");

        // 确保目录存在
        fs::create_dir_all(&data_dir)
            .map_err(|e| FeishuError::StorageError(format!("Failed to create data dir: {}", e)))?;

        let token_path = data_dir.join("token.json");
        Ok(Self { token_path })
    }

    /// 保存 Token
    pub fn save(&self, token: &TokenData) -> Result<()> {
        let json = serde_json::to_string_pretty(token)
            .map_err(|e| FeishuError::StorageError(format!("Failed to serialize token: {}", e)))?;

        fs::write(&self.token_path, json)
            .map_err(|e| FeishuError::StorageError(format!("Failed to write token: {}", e)))?;

        Ok(())
    }

    /// 加载 Token
    pub fn load(&self) -> Result<TokenData> {
        if !self.token_path.exists() {
            return Err(FeishuError::NotLoggedIn);
        }

        let json = fs::read_to_string(&self.token_path)
            .map_err(|e| FeishuError::StorageError(format!("Failed to read token: {}", e)))?;

        let token: TokenData = serde_json::from_str(&json)
            .map_err(|e| FeishuError::StorageError(format!("Failed to parse token: {}", e)))?;

        Ok(token)
    }

    /// 清除 Token
    pub fn clear(&self) -> Result<()> {
        if self.token_path.exists() {
            fs::remove_file(&self.token_path)
                .map_err(|e| FeishuError::StorageError(format!("Failed to remove token: {}", e)))?;
        }
        Ok(())
    }

    /// 检查是否已登录
    pub fn is_logged_in(&self) -> bool {
        self.token_path.exists()
    }
}
