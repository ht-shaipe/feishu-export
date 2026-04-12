use thiserror::Error;

/// 飞书导出工具统一错误类型
#[derive(Error, Debug)]
pub enum FeishuError {
    #[error("API error: code={code}, msg={msg}")]
    ApiError { code: i32, msg: String },

    #[error("Token expired or invalid")]
    TokenExpired,

    #[error("Permission denied for node: {node}")]
    PermissionDenied { node: String },

    #[error("Export timeout for token: {token}")]
    ExportTimeout { token: String },

    #[error("File too large: {size} bytes")]
    FileTooLarge { size: u64 },

    #[error("Rate limited, retry after {retry_after} seconds")]
    RateLimited { retry_after: u64 },

    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("TOML error: {0}")]
    TomlError(#[from] toml::de::Error),

    #[error("Config error: {0}")]
    ConfigError(String),

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    #[error("Unsupported document type: {doc_type}")]
    UnsupportedType { doc_type: String },

    #[error("Conversion error: {0}")]
    ConversionError(String),

    #[error("HTTP server error: {0}")]
    HttpServerError(String),

    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    #[error("Not logged in, please run 'feishu-export login' first")]
    NotLoggedIn,

    #[error("Config not found, please run 'feishu-export config set' first")]
    ConfigNotFound,

    #[error("ZIP error: {0}")]
    ZipError(#[from] zip::result::ZipError),

    #[error("Address parse error: {0}")]
    AddrParseError(#[from] std::net::AddrParseError),

    #[error("Path strip prefix error: {0}")]
    StripPrefixError(#[from] std::path::StripPrefixError),
}

pub type Result<T> = std::result::Result<T, FeishuError>;

impl FeishuError {
    /// 从飞书 API 响应创建错误
    pub fn from_api_response(code: i32, msg: String) -> Self {
        match code {
            99991663 => FeishuError::TokenExpired,
            1310006 => FeishuError::PermissionDenied { node: msg },
            1310007 => FeishuError::ExportTimeout { token: msg },
            _ => FeishuError::ApiError { code, msg },
        }
    }

    /// 判断是否可重试
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            FeishuError::NetworkError(_)
                | FeishuError::RateLimited { .. }
                | FeishuError::ExportTimeout { .. }
        )
    }
}
