//! Persistent token store

use crate::error::{FeishuCoreError as Error, Result};
use crate::models::auth::TokenData;
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt as _;

pub struct TokenStore {
    token_path: PathBuf,
}

impl TokenStore {
    pub fn new() -> Self {
        let data_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("feishu-export");

        Self {
            token_path: data_dir.join("token.json"),
        }
    }

    /// Save token to disk
    pub async fn save(&self, token: &TokenData) -> Result<()> {
        let json = serde_json::to_string_pretty(token)
            .map_err(|e| Error::StorageError(format!("serialize token: {}", e)))?;

        if let Some(parent) = self.token_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| Error::StorageError(format!("mkdir: {}", e)))?;
        }

        let mut file = fs::File::create(&self.token_path)
            .await
            .map_err(|e| Error::StorageError(format!("create token file: {}", e)))?;

        file.write_all(json.as_bytes())
            .await
            .map_err(|e| Error::StorageError(format!("write token: {}", e)))?;

        file.flush()
            .await
            .map_err(|e| Error::StorageError(format!("flush: {}", e)))?;

        Ok(())
    }

    /// Load token from disk
    pub async fn load(&self) -> Result<TokenData> {
        if !fs::try_exists(&self.token_path)
            .await
            .map_err(|e| Error::StorageError(format!("check token exists: {}", e)))?
        {
            return Err(Error::NotLoggedIn);
        }

        let json = fs::read_to_string(&self.token_path)
            .await
            .map_err(|e| Error::StorageError(format!("read token: {}", e)))?;

        serde_json::from_str(&json)
            .map_err(|e| Error::StorageError(format!("parse token JSON: {}", e)))
    }

    /// Delete token file
    pub async fn clear(&self) -> Result<()> {
        if fs::try_exists(&self.token_path)
            .await
            .map_err(|e| Error::StorageError(format!("check: {}", e)))?
        {
            fs::remove_file(&self.token_path)
                .await
                .map_err(|e| Error::StorageError(format!("remove: {}", e)))?;
        }
        Ok(())
    }

    /// Check if logged in (token file exists)
    pub fn is_logged_in(&self) -> bool {
        self.token_path.exists()
    }
}
