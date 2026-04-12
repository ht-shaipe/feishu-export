//! Export cache store (for resume-on-interrupt)

use crate::error::{FeishuCoreError as Error, Result};
use crate::models::export::ExportCache;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs;

pub struct CacheStore {
    cache_dir: PathBuf,
}

impl CacheStore {
    pub fn new() -> Self {
        let data_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("feishu-export");

        Self {
            cache_dir: data_dir.join("cache"),
        }
    }

    fn cache_path(&self, space_id: &str, format: &str) -> PathBuf {
        self.cache_dir.join(format!("{}_{}.json", space_id, format))
    }

    /// Load cache for a space+format (returns empty cache if not found)
    pub async fn load(&self, space_id: &str, format: &str) -> Result<ExportCache> {
        let path = self.cache_path(space_id, format);

        if !fs::try_exists(&path)
            .await
            .map_err(|e| Error::StorageError(format!("check cache exists: {}", e)))?
        {
            return Ok(ExportCache::new(
                space_id.to_string(),
                crate::models::ExportFormat::Docx,
            ));
        }

        let json = fs::read_to_string(&path)
            .await
            .map_err(|e| Error::StorageError(format!("read cache: {}", e)))?;

        serde_json::from_str(&json)
            .map_err(|e| Error::StorageError(format!("parse cache: {}", e)))
    }

    /// Save cache to disk
    pub async fn save(&self, cache: &ExportCache) -> Result<()> {
        let format_str = format!("{:?}", cache.format);
        let path = self.cache_path(&cache.space_id, &format_str);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| Error::StorageError(format!("mkdir: {}", e)))?;
        }

        let json = serde_json::to_string_pretty(cache)
            .map_err(|e| Error::StorageError(format!("serialize cache: {}", e)))?;

        fs::write(&path, json.as_bytes())
            .await
            .map_err(|e| Error::StorageError(format!("write cache: {}", e)))?;

        Ok(())
    }

    /// Clear cache for a specific space or all spaces
    pub async fn clear(&self, space_id: Option<&str>) -> Result<()> {
        if !fs::try_exists(&self.cache_dir).await
            .map_err(|e| Error::StorageError(format!("check cache dir: {}", e)))?
        {
            return Ok(());
        }

        let mut entries = fs::read_dir(&self.cache_dir)
            .await
            .map_err(|e| Error::StorageError(format!("read cache dir: {}", e)))?;

        while let Some(entry) = entries.next_entry().await
            .map_err(|e| Error::StorageError(format!("read dir entry: {}", e)))?
        {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(sid) = space_id {
                if name.starts_with(&format!("{}_", sid)) {
                    fs::remove_file(entry.path())
                        .await
                        .map_err(|e| Error::StorageError(format!("remove: {}", e)))?;
                }
            } else {
                fs::remove_file(entry.path())
                    .await
                    .map_err(|e| Error::StorageError(format!("remove: {}", e)))?;
            }
        }
        Ok(())
    }

    /// List all caches
    pub async fn list(&self) -> Result<HashMap<String, ExportCache>> {
        let mut caches = HashMap::new();

        if !fs::try_exists(&self.cache_dir).await
            .map_err(|e| Error::StorageError(format!("check cache dir: {}", e)))?
        {
            return Ok(caches);
        }

        let mut entries = fs::read_dir(&self.cache_dir)
            .await
            .map_err(|e| Error::StorageError(format!("read cache dir: {}", e)))?;

        while let Some(entry) = entries.next_entry().await
            .map_err(|e| Error::StorageError(format!("read entry: {}", e)))?
        {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }

            let json = fs::read_to_string(&path)
                .await
                .map_err(|e| Error::StorageError(format!("read cache file: {}", e)))?;

            if let Ok(cache) = serde_json::from_str::<ExportCache>(&json) {
                let key = format!("{}_{:?}", cache.space_id, cache.format);
                caches.insert(key, cache);
            }
        }
        Ok(caches)
    }
}
