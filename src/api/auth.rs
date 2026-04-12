use crate::api::FeishuClient;
use crate::error::{FeishuError, Result};
use crate::models::auth::*;
use crate::storage::ConfigStore;
use serde_json::json;

/// 授权 API
impl FeishuClient {
    /// 构造 OAuth 授权 URL
    pub fn build_auth_url(&self, config: &ConfigStore, state: &str) -> Result<String> {
        let config = config.load()?;
        let scopes = [
            "wiki:wiki:readonly",
            "wiki:node:read",
            "wiki:node:retrieve",
            "wiki:space:retrieve",
            "drive:drive:readonly",
            "drive:export:readonly",
            "docx:document:readonly",
            "offline_access",
        ]
        .join(" ");

        let url = url::Url::parse_with_params(
            format!("{}/open-apis/authen/v1/authorize", self.auth_url()).as_str(),
            &[
                ("app_id", config.app_id.as_str()),
                ("redirect_uri", config.redirect_uri.as_str()),
                ("scope", scopes.as_str()),
                ("state", state),
            ],
        )
        .map_err(|e| FeishuError::InvalidUrl(format!("Invalid auth URL: {}", e)))?;

        Ok(url.to_string())
    }

    /// 获取 app_access_token
    pub async fn get_app_access_token(&self, config: &ConfigStore) -> Result<String> {
        let config = config.load()?;
        let body = json!({
            "app_id": config.app_id,
            "app_secret": config.app_secret,
        });

        let response = self
            .post_anonymous("/open-apis/auth/v3/app_access_token/internal/", body)
            .await?;

        let data: AppAccessTokenResponse = response.json().await?;
        if data.code != 0 {
            return Err(FeishuError::AuthFailed(format!(
                "Failed to get app_access_token: code {}",
                data.code
            )));
        }

        Ok(data.app_access_token)
    }

    /// 用授权码换取用户 token
    pub async fn exchange_code_for_token(
        &self,
        config: &ConfigStore,
        code: &str,
    ) -> Result<TokenData> {
        let app_token = self.get_app_access_token(config).await?;

        let body = json!({
            "grant_type": "authorization_code",
            "code": code,
        });

        let url = format!("{}/open-apis/authen/v1/oidc/access_token", self.base_url());
        let response = self.post_full_url(&url, &app_token, body).await?;

        let data: OAuthTokenResponse = response.json().await?;
        if data.code != 0 {
            return Err(FeishuError::AuthFailed(format!(
                "Failed to exchange code: code {}",
                data.code
            )));
        }

        Ok(TokenData::new(
            data.data.access_token,
            data.data.refresh_token,
            data.data.expires_in,
        ))
    }

    /// 刷新用户 token
    pub async fn refresh_user_token(
        &self,
        config: &ConfigStore,
        refresh_token: &str,
    ) -> Result<TokenData> {
        let app_token = self.get_app_access_token(config).await?;

        let body = json!({
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
        });

        let url = format!(
            "{}/open-apis/authen/v1/oidc/refresh_access_token",
            self.base_url()
        );
        let response = self.post_full_url(&url, &app_token, body).await?;

        let data: RefreshTokenResponse = response.json().await?;
        if data.code != 0 {
            return Err(FeishuError::AuthFailed(format!(
                "Failed to refresh token: code {}",
                data.code
            )));
        }

        Ok(TokenData::new(
            data.data.access_token,
            data.data.refresh_token,
            data.data.expires_in,
        ))
    }
}
