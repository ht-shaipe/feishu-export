//! `feishu-core` — Feishu API client, models, storage and export engine
//!
//! This crate provides a reusable Feishu integration layer:
//! - **FeishuClient**: typed HTTP client for the Feishu Open API
//! - **models**: Space, Node, ExportFormat, ExportCache, TokenData, etc.
//! - **storage**: ConfigStore, TokenStore, CacheStore for persistent credentials
//! - **engine**: ExportEngine for batch export with progress tracking and resume

pub mod api;
pub mod error;
pub mod engine;
pub mod models;
pub mod storage;

pub use api::FeishuClient;
pub use error::{FeishuCoreError, Result};
pub use models::{Node, Space, ExportFormat, ExportCache, TokenData};
pub use storage::{AppConfig, ConfigStore, TokenStore, CacheStore};
pub use engine::ExportEngine;
