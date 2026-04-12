//! Feishu Open API client

use crate::error::{FeishuCoreError as Error, Result};
use crate::models::auth::*;
use crate::models::export::*;
use crate::models::wiki::*;
use crate::storage::ConfigStore;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

// ─────────────────────────────────────────────────────────────────────────────
// HTTP client
// ─────────────────────────────────────────────────────────────────────────────

fn make_headers(token: &str) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", token)).unwrap());
    h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    h
}

/// Feishu Open API HTTP client
#[derive(Clone)]
pub struct FeishuClient {
    http: Arc<reqwest::Client>,
    base_url: String,
    auth_url: String,
}

impl FeishuClient {
    /// Create a new client
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("feishu client: failed to build HTTP client");
        Self {
            http: Arc::new(http),
            base_url: "https://open.feishu.cn".to_string(),
            auth_url: "https://accounts.feishu.cn".to_string(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn auth_url(&self) -> &str {
        &self.auth_url
    }

    async fn get(&self, path: &str, token: &str) -> Result<reqwest::Response> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.http.get(&url).headers(make_headers(token)).send().await?;
        self.check_response(resp).await
    }

    async fn post(&self, path: &str, token: &str, body: serde_json::Value) -> Result<reqwest::Response> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http
            .post(&url)
            .headers(make_headers(token))
            .json(&body)
            .send()
            .await?;
        self.check_response(resp).await
    }

    async fn post_anonymous(&self, path: &str, body: serde_json::Value) -> Result<reqwest::Response> {
        let url = format!("{}{}", self.base_url, path);
        let mut h = HeaderMap::new();
        h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let resp = self.http.post(&url).headers(h).json(&body).send().await?;
        self.check_response(resp).await
    }

    async fn post_full_url(&self, url: &str, token: &str, body: serde_json::Value) -> Result<reqwest::Response> {
        let resp = self.http.post(url).headers(make_headers(token)).json(&body).send().await?;
        self.check_response(resp).await
    }

    async fn check_response(&self, resp: reqwest::Response) -> Result<reqwest::Response> {
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }
        let body = resp.text().await.unwrap_or_default();
        let short = body.chars().take(300).collect::<String>();
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
            let code = v.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
            let msg = v.get("msg").and_then(|v| v.as_str()).unwrap_or(&short);
            return Err(Error::ApiError { code: code as i32, msg: format!("code={}, msg={}", code, msg) });
        }
        Err(Error::ApiError { code: status.as_u16() as i32, msg: short })
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Auth API
    // ─────────────────────────────────────────────────────────────────────────

    /// Build OAuth authorize URL
    pub async fn build_auth_url(&self, config: &ConfigStore, state: &str) -> Result<String> {
        let config = config.load().await?;
        let scopes = [
            "wiki:wiki:readonly", "wiki:node:read", "wiki:node:retrieve",
            "wiki:space:retrieve", "drive:drive:readonly", "drive:export:readonly",
            "docx:document:readonly", "offline_access",
        ].join(" ");

        let url = url::Url::parse_with_params(
            format!("{}/open-apis/authen/v1/authorize", self.auth_url()).as_str(),
            &[
                ("app_id", config.app_id.as_str()),
                ("redirect_uri", config.redirect_uri.as_str()),
                ("scope", scopes.as_str()),
                ("state", state),
            ],
        ).map_err(|e| Error::InvalidUrl(format!("Invalid auth URL: {}", e)))?;
        Ok(url.to_string())
    }

    /// Get app_access_token (internal)
    async fn get_app_access_token(&self, config: &ConfigStore) -> Result<String> {
        let config = config.load().await?;
        let body = json!({ "app_id": config.app_id, "app_secret": config.app_secret });
        let resp = self.post_anonymous("/open-apis/auth/v3/app_access_token/internal/", body).await?;
        let data: AppAccessTokenResponse = resp.json().await?;
        if data.code != 0 {
            return Err(Error::AuthFailed(format!("get_app_access_token failed: code={}", data.code)));
        }
        Ok(data.app_access_token)
    }

    /// Exchange authorization code for user token
    pub async fn exchange_code_for_token(&self, config: &ConfigStore, code: &str) -> Result<TokenData> {
        let app_token = self.get_app_access_token(config).await?;
        let body = json!({ "grant_type": "authorization_code", "code": code });
        let url = format!("{}/open-apis/authen/v1/oidc/access_token", self.base_url());
        let resp = self.post_full_url(&url, &app_token, body).await?;
        let data: OAuthTokenResponse = resp.json().await?;
        if data.code != 0 {
            return Err(Error::AuthFailed(format!("exchange_code_for_token failed: code={}", data.code)));
        }
        Ok(TokenData::new(data.data.access_token, data.data.refresh_token, data.data.expires_in))
    }

    /// Refresh user access token
    pub async fn refresh_user_token(&self, config: &ConfigStore, refresh_token: &str) -> Result<TokenData> {
        let app_token = self.get_app_access_token(config).await?;
        let body = json!({ "grant_type": "refresh_token", "refresh_token": refresh_token });
        let url = format!("{}/open-apis/authen/v1/oidc/refresh_access_token", self.base_url());
        let resp = self.post_full_url(&url, &app_token, body).await?;
        let data: RefreshTokenResponse = resp.json().await?;
        if data.code != 0 {
            return Err(Error::AuthFailed(format!("refresh_user_token failed: code={}", data.code)));
        }
        Ok(TokenData::new(data.data.access_token, data.data.refresh_token, data.data.expires_in))
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Wiki API
    // ─────────────────────────────────────────────────────────────────────────

    /// List knowledge spaces
    pub async fn list_spaces(&self, token: &str) -> Result<Vec<Space>> {
        let mut spaces = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let mut url = "/open-apis/wiki/v2/spaces?page_size=50".to_string();
            if let Some(ref t) = page_token {
                url.push_str(&format!("&page_token={}", t));
            }

            let resp = self.get(&url, token).await?;
            let data: SpacesResponse = resp.json().await?;
            if data.code != 0 {
                return Err(Error::from_api_response(data.code, "list_spaces failed".to_string()));
            }

            spaces.extend(data.data.items);
            if !data.data.has_more {
                break;
            }
            page_token = Some(data.data.page_token);
        }
        Ok(spaces)
    }

    /// List child nodes of a space or parent node
    pub async fn list_nodes(&self, token: &str, space_id: &str, parent_token: Option<&str>) -> Result<Vec<Node>> {
        let mut nodes = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let mut url = format!("/open-apis/wiki/v2/spaces/{}/nodes?page_size=50", space_id);
            if let Some(t) = parent_token {
                url.push_str(&format!("&parent_node_token={}", t));
            }
            if let Some(ref t) = page_token {
                url.push_str(&format!("&page_token={}", t));
            }

            let resp = self.get(&url, token).await?;
            let data: NodesResponse = resp.json().await?;
            if data.code != 0 {
                return Err(Error::from_api_response(data.code, format!("list_nodes failed for space {}", space_id)));
            }

            nodes.extend(data.data.items);
            if !data.data.has_more {
                break;
            }
            page_token = Some(data.data.page_token);
        }
        Ok(nodes)
    }

    /// Get node info
    pub async fn get_node_info(&self, token: &str, node_token: &str) -> Result<NodeInfo> {
        let url = format!("/open-apis/wiki/v2/spaces/get_node?token={}", node_token);
        let resp = self.get(&url, token).await?;
        let data: NodeInfoResponse = resp.json().await?;
        if data.code != 0 {
            return Err(Error::from_api_response(data.code, format!("get_node_info failed for {}", node_token)));
        }
        Ok(data.data)
    }

    /// Recursively get the full node tree (DFS, iterative)
    pub async fn get_node_tree(&self, token: &str, space_id: &str) -> Result<Vec<Node>> {
        let mut all_nodes = Vec::new();
        let mut stack: Vec<(Option<String>, u32)> = vec![(None, 0)];

        while let Some((parent_token, depth)) = stack.pop() {
            let nodes = self.list_nodes(token, space_id, parent_token.as_deref()).await?;

            for node in nodes.into_iter().rev() {
                let mut node = node;
                node.depth = depth;

                if node.is_shortcut() {
                    continue; // skip shortcuts to avoid duplicates
                }

                let has_child = node.has_child;
                let node_token = node.node_token.clone();
                all_nodes.push(node);

                if has_child {
                    stack.push((Some(node_token), depth + 1));
                }
            }
        }
        Ok(all_nodes)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Export API
    // ─────────────────────────────────────────────────────────────────────────

    /// Create an export task and return the ticket
    pub async fn create_export_task(&self, token: &str, obj_token: &str, obj_type: &str, format: ExportFormat) -> Result<String> {
        // Check if the document type supports export
        match obj_type {
            "file" => {
                return Err(Error::UnsupportedType { doc_type: format!("file (direct download): {}", obj_token) });
            }
            "mindnote" | "slides" => {
                return Err(Error::UnsupportedType { doc_type: obj_type.to_string() });
            }
            _ => {}
        }

        // Resolve Auto to actual format before calling API
        let actual_format = if format == ExportFormat::Auto {
            ExportFormat::for_node_type(obj_type)
        } else {
            format
        };

        let file_extension = actual_format.api_extension();

        let body = json!({
            "token": obj_token,
            "type": obj_type,
            "file_extension": file_extension,
        });

        println!("  📤 创建导出任务...");
        println!("    - 对象 token: {}", obj_token);
        println!("    - 对象类型: {}", obj_type);
        println!("    - 文件扩展名: {}", file_extension);

        let resp = self.post("/open-apis/drive/v1/export_tasks", token, body).await?;
        let data: CreateExportTaskResponse = resp.json().await?;
        if data.code != 0 {
            return Err(Error::from_api_response(data.code, format!("create_export_task failed for {} (type={}, ext={})", obj_token, obj_type, file_extension)));
        }
        let ticket = data.data.ticket;
        println!("  ✅ 导出任务创建成功，ticket: {}", ticket);
        Ok(ticket)
    }

    /// Poll export task status until complete, return file_token
    pub async fn poll_export_task(&self, token: &str, ticket: &str, obj_token: &str) -> Result<String> {
        let mut attempts = 0;
        let max_attempts = 30;
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await; // initial wait

        loop {
            attempts += 1;
            let url = format!(
                "/open-apis/drive/v1/export_tasks/{}?token={}",
                ticket, obj_token
            );

            println!("  🔄 检查导出任务状态 (尝试 {}/{})...", attempts, max_attempts);
            let resp = self.get(&url, token).await?;
            let data: ExportTaskStatusResponse = resp.json().await?;

            if data.code != 0 {
                let err = Error::from_api_response(data.code, format!("poll_export_task failed for ticket {}", ticket));
                println!("  🚨 错误转换: code={}, type={:?}", data.code, err);
                return Err(err);
            }

            let is_complete = data.data.result.extra.is_complete == "true";
            let file_token = data.data.result.file_token.clone();

            println!("    - 完成状态: {}", is_complete);
            println!("    - 文件 token: '{}'", file_token);
            println!("    - 扩展名: {}", data.data.result.file_extension);
            println!("    - 文件名: {}", data.data.result.file_name);

            if is_complete {
                if file_token.is_empty() {
                    println!("  ⚠️ 任务已完成但文件 token 为空");
                    return Err(Error::ExportTimeout { token: obj_token.to_string() });
                }
                return Ok(file_token);
            }

            if attempts >= max_attempts {
                return Err(Error::ExportTimeout { token: obj_token.to_string() });
            }

            let delay = 2u64.pow(attempts.min(5)) * 1000;
            tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
        }
    }

    /// Download an exported file by file_token
    pub async fn download_export_file(&self, token: &str, file_token: &str) -> Result<reqwest::Response> {
        let url = format!(
            "{}/open-apis/drive/v1/export_tasks/file/{}/download",
            self.base_url, file_token
        );

        let mut h = HeaderMap::new();
        h.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", token)).unwrap());

        let resp = self.http.get(&url).headers(h).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            let api_error = Error::ApiError {
                code: status.as_u16() as i32,
                msg: format!("download failed ({}): {}", status, &body[..body.len().min(300)]),
            };
            println!("  🚨 下载错误: {:?}", api_error);
            return Err(api_error);
        }
        Ok(resp)
    }

    /// Full export flow: create → poll → download
    /// Falls back to PDF if the requested format is not supported by the node type
    pub async fn export_document(
        &self,
        token: &str,
        obj_token: &str,
        obj_type: &str,
        format: ExportFormat,
    ) -> Result<reqwest::Response> {
        // Try with requested format first
        let result = self.do_export(token, obj_token, obj_type, format).await;

        // If failed due to file extension mismatch or file token invalid, retry with PDF (most compatible fallback)
        if let Err(err) = &result {
            println!("  ❌ 初始导出失败: {}", err);
            println!("  🔍 错误类型检查:");
            println!("    - is_file_extension_mismatch: {}", err.is_file_extension_mismatch());
            println!("    - is_file_token_invalid: {}", err.is_file_token_invalid());

            if err.is_file_extension_mismatch() || err.is_file_token_invalid() {
                // PDF is the most compatible format for all document types
                if format != ExportFormat::Pdf {
                    println!("    ⚠️ 尝试降级到 PDF 格式...");
                    return self.do_export(token, obj_token, obj_type, ExportFormat::Pdf).await;
                } else {
                    println!("    ⚠️ 已经是 PDF 格式，无法降级");
                }
            } else {
                println!("    ⚠️ 错误类型不匹配，不尝试降级");
            }
        }

        result
    }

    /// Internal export: create → poll → download (no fallback)
    async fn do_export(
        &self,
        token: &str,
        obj_token: &str,
        obj_type: &str,
        format: ExportFormat,
    ) -> Result<reqwest::Response> {
        let ticket = self.create_export_task(token, obj_token, obj_type, format).await?;
        let file_token = self.poll_export_task(token, &ticket, obj_token).await?;
        self.download_export_file(token, &file_token).await
    }
}

impl Default for FeishuClient {
    fn default() -> Self {
        Self::new()
    }
}
