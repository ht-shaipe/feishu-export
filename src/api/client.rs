use crate::error::{FeishuError, Result};
use reqwest::{header, Client as HttpClient, Response};
use std::sync::Arc;
use std::time::Duration;

/// 飞书 API 客户端
#[derive(Clone)]
pub struct FeishuClient {
    pub(crate) http: Arc<HttpClient>,
    base_url: String,
    auth_url: String,
}

impl FeishuClient {
    /// 创建新的 API 客户端
    pub fn new() -> Self {
        let http = HttpClient::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            http: Arc::new(http),
            base_url: "https://open.feishu.cn".to_string(),
            auth_url: "https://accounts.feishu.cn".to_string(),
        }
    }

    /// 获取基础 URL
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// 获取授权 URL
    pub fn auth_url(&self) -> &str {
        &self.auth_url
    }

    /// 发送 GET 请求（带用户 token）
    pub async fn get(&self, path: &str, access_token: &str) -> Result<Response> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .http
            .get(&url)
            .header(header::AUTHORIZATION, format!("Bearer {}", access_token))
            .send()
            .await?;

        self.check_response(response).await
    }

    /// 发送 POST 请求（带用户 token）
    pub async fn post(
        &self,
        path: &str,
        access_token: &str,
        body: serde_json::Value,
    ) -> Result<Response> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .http
            .post(&url)
            .header(header::AUTHORIZATION, format!("Bearer {}", access_token))
            .header(header::CONTENT_TYPE, "application/json")
            .json(&body)
            .send()
            .await?;

        self.check_response(response).await
    }

    /// 发送 POST 请求（不带 token，用于获取 app_access_token）
    pub async fn post_anonymous(&self, path: &str, body: serde_json::Value) -> Result<Response> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .http
            .post(&url)
            .header(header::CONTENT_TYPE, "application/json")
            .json(&body)
            .send()
            .await?;

        self.check_response(response).await
    }

    /// 发送 POST 请求到完整 URL（带 token）
    pub async fn post_full_url(
        &self,
        url: &str,
        access_token: &str,
        body: serde_json::Value,
    ) -> Result<Response> {
        let response = self
            .http
            .post(url)
            .header(header::AUTHORIZATION, format!("Bearer {}", access_token))
            .header(header::CONTENT_TYPE, "application/json")
            .json(&body)
            .send()
            .await?;

        self.check_response(response).await
    }

    /// 下载文件
    pub async fn download(&self, url: &str, access_token: &str) -> Result<reqwest::Response> {
        let response = self
            .http
            .get(url)
            .header(header::AUTHORIZATION, format!("Bearer {}", access_token))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(FeishuError::NetworkError(reqwest::Error::from(
                response.error_for_status().unwrap_err(),
            )));
        }

        Ok(response)
    }

    /// 检查响应状态
    async fn check_response(&self, response: Response) -> Result<Response> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }

        // 非 2xx: 先读 body 再抛错，保留飞书返回的 error message
        let body = response.text().await.unwrap_or_default();
        let short = body.chars().take(300).collect::<String>();

        eprintln!("[API] HTTP error body: {}", short);

        // 尝试解析飞书错误格式
        if let Ok(feishu_err) = serde_json::from_str::<serde_json::Value>(&body) {
            let code = feishu_err.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
            let msg = feishu_err.get("msg").and_then(|v| v.as_str()).unwrap_or(&short);
            return Err(FeishuError::ApiError {
                code: code as i32,
                msg: format!("code={}, msg={}", code, msg),
            });
        }

        Err(FeishuError::ApiError {
            code: status.as_u16() as i32,
            msg: short,
        })
    }
}

impl Default for FeishuClient {
    fn default() -> Self {
        Self::new()
    }
}
