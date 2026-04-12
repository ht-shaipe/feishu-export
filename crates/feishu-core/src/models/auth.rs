use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 用户授权令牌数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenData {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: DateTime<Utc>,
    #[serde(default)]
    pub user_id: Option<String>,
}

impl TokenData {
    pub fn new(access_token: String, refresh_token: String, expires_in: i64) -> Self {
        Self {
            access_token,
            refresh_token,
            expires_at: Utc::now() + chrono::Duration::seconds(expires_in),
            user_id: None,
        }
    }

    /// 提前5分钟判定为过期，留出刷新窗口
    pub fn is_expired(&self) -> bool {
        Utc::now() + chrono::Duration::minutes(5) >= self.expires_at
    }

    pub fn with_user_id(mut self, user_id: String) -> Self {
        self.user_id = Some(user_id);
        self
    }
}

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
    #[serde(default)]
    pub token_type: String,
}

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

#[derive(Debug, Deserialize)]
pub struct AppAccessTokenResponse {
    pub code: i32,
    pub app_access_token: String,
    #[serde(default)]
    pub expire: i64,
}

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

#[derive(Debug, Deserialize)]
pub struct OAuthCallback {
    pub code: String,
    pub state: String,
}
