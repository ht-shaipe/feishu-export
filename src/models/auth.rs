use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 用户授权令牌数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenData {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: DateTime<Utc>,
    pub user_id: Option<String>,
}

impl TokenData {
    /// 创建新的令牌数据
    pub fn new(access_token: String, refresh_token: String, expires_in: i64) -> Self {
        let expires_at = Utc::now() + chrono::Duration::seconds(expires_in);
        Self {
            access_token,
            refresh_token,
            expires_at,
            user_id: None,
        }
    }

    /// 检查令牌是否即将过期（提前5分钟刷新）
    pub fn is_expired(&self) -> bool {
        Utc::now() + chrono::Duration::minutes(5) >= self.expires_at
    }

    /// 设置用户 ID
    pub fn with_user_id(mut self, user_id: String) -> Self {
        self.user_id = Some(user_id);
        self
    }
}

/// OAuth 授权响应
#[derive(Debug, Deserialize)]
pub struct OAuthTokenResponse {
    pub code: i32,
    pub data: OAuthTokenData,
}

#[derive(Debug, Deserialize)]
pub struct OAuthTokenData {
    pub access_token: String,
    pub refresh_token: String,
    #[serde(default)]
    pub expires_in: i64,
    #[serde(default)]
    pub refresh_expires_in: i64,
    pub token_type: String,
}

/// 刷新令牌响应
#[derive(Debug, Deserialize)]
pub struct RefreshTokenResponse {
    pub code: i32,
    pub data: RefreshTokenData,
}

#[derive(Debug, Deserialize)]
pub struct RefreshTokenData {
    pub access_token: String,
    pub refresh_token: String,
    #[serde(default)]
    pub expires_in: i64,
}

/// 应用访问令牌响应
#[derive(Debug, Deserialize)]
pub struct AppAccessTokenResponse {
    pub code: i32,
    pub app_access_token: String,
    #[serde(default)]
    pub expire: i64,
}

/// OAuth 授权状态
#[derive(Debug, Clone)]
pub struct OAuthState {
    pub state: String,
    pub created_at: DateTime<Utc>,
}

impl OAuthState {
    pub fn new() -> Self {
        Self {
            state: uuid::Uuid::new_v4().to_string(),
            created_at: Utc::now(),
        }
    }
}

/// 授权回调参数
#[derive(Debug, Deserialize)]
pub struct OAuthCallback {
    pub code: String,
    pub state: String,
}
