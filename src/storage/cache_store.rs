use crate::error::{FeishuError, Result};
use crate::models::export::ExportCache;
use serde_json;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// 导出缓存存储（用于断点续导）
pub struct CacheStore {
    cache_dir: PathBuf,
}

impl CacheStore {
    /// 创建新的缓存存储
    pub fn new() -> Result<Self> {
        let data_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("feishu-export");

        let cache_dir = data_dir.join("cache");
        fs::create_dir_all(&cache_dir)
            .map_err(|e| FeishuError::StorageError(format!("Failed to create cache dir: {}", e)))?;

        Ok(Self { cache_dir })
    }

    /// 获取缓存文件路径
    fn cache_path(&self, space_id: &str, format: &str) -> PathBuf {
        self.cache_dir.join(format!("{}_{}.json", space_id, format))
    }

    /// 加载缓存
    pub fn load(&self, space_id: &str, format: &str) -> Result<ExportCache> {
        let path = self.cache_path(space_id, format);
        if !path.exists() {
            return Ok(ExportCache::new(
                space_id.to_string(),
                crate::models::export::ExportFormat::Docx,
            ));
        }

        let json = fs::read_to_string(&path)
            .map_err(|e| FeishuError::StorageError(format!("Failed to read cache: {}", e)))?;

        let cache: ExportCache = serde_json::from_str(&json)
            .map_err(|e| FeishuError::StorageError(format!("Failed to parse cache: {}", e)))?;

        Ok(cache)
    }

    /// 保存缓存
    pub fn save(&self, cache: &ExportCache) -> Result<()> {
        let format_str = format!("{:?}", cache.format);
        let path = self.cache_path(&cache.space_id, &format_str);
        let json = serde_json::to_string_pretty(cache)
            .map_err(|e| FeishuError::StorageError(format!("Failed to serialize cache: {}", e)))?;

        fs::write(&path, json)
            .map_err(|e| FeishuError::StorageError(format!("Failed to write cache: {}", e)))?;

        Ok(())
    }

    /// 清除缓存
    pub fn clear(&self, space_id: Option<&str>) -> Result<()> {
        if let Some(sid) = space_id {
            // 清除特定空间的缓存
            let entries = fs::read_dir(&self.cache_dir).map_err(|e| {
                FeishuError::StorageError(format!("Failed to read cache dir: {}", e))
            })?;

            for entry in entries {
                let entry = entry.map_err(|e| {
                    FeishuError::StorageError(format!("Failed to read cache entry: {}", e))
                })?;
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with(&format!("{}_", sid)) {
                    fs::remove_file(entry.path()).map_err(|e| {
                        FeishuError::StorageError(format!("Failed to remove cache: {}", e))
                    })?;
                }
            }
        } else {
            // 清除所有缓存
            for entry in fs::read_dir(&self.cache_dir).map_err(|e| {
                FeishuError::StorageError(format!("Failed to read cache dir: {}", e))
            })? {
                let entry = entry.map_err(|e| {
                    FeishuError::StorageError(format!("Failed to read cache entry: {}", e))
                })?;
                fs::remove_file(entry.path()).map_err(|e| {
                    FeishuError::StorageError(format!("Failed to remove cache: {}", e))
                })?;
            }
        }
        Ok(())
    }

    /// 列出所有缓存
    pub fn list(&self) -> Result<HashMap<String, ExportCache>> {
        let mut caches = HashMap::new();

        if !self.cache_dir.exists() {
            return Ok(caches);
        }

        let entries = fs::read_dir(&self.cache_dir)
            .map_err(|e| FeishuError::StorageError(format!("Failed to read cache dir: {}", e)))?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                FeishuError::StorageError(format!("Failed to read cache entry: {}", e))
            })?;

            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }

            let json = fs::read_to_string(&path)
                .map_err(|e| FeishuError::StorageError(format!("Failed to read cache: {}", e)))?;

            if let Ok(cache) = serde_json::from_str::<ExportCache>(&json) {
                let key = format!("{}_{}", cache.space_id, format!("{:?}", cache.format));
                caches.insert(key, cache);
            }
        }

        Ok(caches)
    }
}
