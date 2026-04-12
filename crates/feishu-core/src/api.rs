//! Feishu Open API client

use crate::error::{FeishuCoreError as Error, Result};
use crate::models::auth::*;
use crate::models::export::*;
use crate::models::wiki::*;
use crate::storage::ConfigStore;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::Deserialize;
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

#[derive(Clone)]
pub struct FeishuClient {
    http: Arc<reqwest::Client>,
    base_url: String,
    auth_url: String,
}

impl FeishuClient {
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

    pub fn base_url(&self) -> &str { &self.base_url }
    pub fn auth_url(&self) -> &str { &self.auth_url }

    async fn get(&self, path: &str, token: &str) -> Result<reqwest::Response> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.http.get(&url).headers(make_headers(token)).send().await?;
        self.check_response(resp).await
    }

    async fn post(&self, path: &str, token: &str, body: serde_json::Value) -> Result<reqwest::Response> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.http.post(&url).headers(make_headers(token)).json(&body).send().await?;
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

    async fn get_app_access_token(&self, config: &ConfigStore) -> Result<String> {
        let config = config.load().await?;
        let body = json!({ "app_id": config.app_id, "app_secret": config.app_secret });
        let resp = self.post_anonymous("/open-apis/auth/v3/app_access_token/internal/", body).await?;
        #[derive(Deserialize)]
        struct AppTokenResp { code: i32, app_access_token: String }
        let data: AppTokenResp = resp.json().await?;
        if data.code != 0 {
            return Err(Error::AuthFailed(format!("get_app_access_token failed: code={}", data.code)));
        }
        Ok(data.app_access_token)
    }

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
                url.push_str(&format!("&parent_token={}", t));
            }
            if let Some(ref t) = page_token {
                url.push_str(&format!("&page_token={}", t));
            }

            let resp = self.get(&url, token).await?;
            let data: NodesResponse = resp.json().await?;
            if data.code != 0 {
                return Err(Error::from_api_response(data.code, "list_nodes failed".to_string()));
            }
            // Filter out non-exportable nodes
            for node in data.data.items {
                if node.is_exportable() {
                    nodes.push(node);
                }
            }
            if !data.data.has_more {
                break;
            }
            page_token = Some(data.data.page_token);
        }
        Ok(nodes)
    }

    /// Get node info (parent chain lookup)
    pub async fn get_node_info(&self, token: &str, space_id: &str, node_token: &str) -> Result<NodeInfo> {
        let url = format!("/open-apis/wiki/v2/spaces/{}/nodes/{}", space_id, node_token);
        let resp = self.get(&url, token).await?;
        let data: NodeInfoResponse = resp.json().await?;
        if data.code != 0 {
            return Err(Error::from_api_response(data.code, format!("get_node_info {} failed", node_token)));
        }
        Ok(data.data)
    }

    /// 递归获取整个知识空间节点树（DFS）
    pub async fn get_node_tree(&self, token: &str, space_id: &str) -> Result<Vec<Node>> {
        let mut all_nodes = Vec::new();
        let mut stack: Vec<(Option<String>, u32)> = vec![(None, 0)];

        while let Some((parent_token, depth)) = stack.pop() {
            let nodes = self.list_nodes(token, space_id, parent_token.as_deref()).await?;
            for node in nodes.into_iter().rev() {
                let mut node = node;
                node.depth = depth;
                if node.is_shortcut() {
                    continue;
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

    /// Get node info via baike API (obj_type + obj_token)
    pub async fn get_baike_node(&self, token: &str, obj_type: &str, obj_token: &str) -> Result<NodeInfo> {
        let url = format!(
            "/open-apis/wiki/v2/spaces/get_node?obj_type={}&obj_token={}",
            obj_type, obj_token
        );
        let resp = self.get(&url, token).await?;
        let data: NodeInfoResponse = resp.json().await?;
        if data.code != 0 {
            return Err(Error::from_api_response(data.code, format!("get_baike_node {} failed", obj_token)));
        }
        Ok(data.data)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Sheet API — 获取工作表 ID
    // ─────────────────────────────────────────────────────────────────────────

    /// 获取电子表格的第一个工作表 ID（用于导出 CSV）
    pub async fn get_sheet_first_sheet_id(&self, token: &str, spreadsheet_token: &str) -> Result<String> {
        let path = format!("/open-apis/sheets/v3/spreadsheets/{}/sheets/query", spreadsheet_token);
        let resp = self.get(&path, token).await?;
        let data: SheetListSheetsResponse = resp.json().await?;
        if data.code != 0 {
            return Err(Error::ApiError {
                code: data.code,
                msg: format!("get_sheet_first_sheet_id: {}", data.msg),
            });
        }
        data.data.sheets.first()
            .map(|s| s.sheet_id.clone())
            .ok_or_else(|| Error::ApiError { code: -1, msg: "工作表列表为空".to_string() })
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Bitable API — 获取数据表 ID
    // ─────────────────────────────────────────────────────────────────────────

    /// 获取多维表格的第一个数据表 ID（用于导出 CSV）
    pub async fn get_bitable_first_table_id(&self, token: &str, app_token: &str) -> Result<String> {
        let path = format!("/open-apis/bitable/v1/apps/{}/tables", app_token);
        let resp = self.get(&path, token).await?;
        let data: BitableListTablesResponse = resp.json().await?;
        if data.code != 0 {
            return Err(Error::ApiError {
                code: data.code,
                msg: format!("get_bitable_first_table_id: {}", data.msg),
            });
        }
        data.data.items.first()
            .map(|t| t.table_id.clone())
            .ok_or_else(|| Error::ApiError { code: -1, msg: "数据表列表为空".to_string() })
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Export API
    // ─────────────────────────────────────────────────────────────────────────

    /// 创建导出任务，返回 ticket
    ///
    /// 对 sheet/bitable：
    /// - 导出 xlsx：不传 sub_id（整个文档导出）
    /// - 导出 csv：必须传 sub_id（sheet_id 或 table_id）
    pub async fn create_export_task(
        &self,
        token: &str,
        obj_token: &str,
        obj_type: &str,
        format: ExportFormat,
        sub_id: Option<&str>,
    ) -> Result<String> {
        let actual_format = if format == ExportFormat::Auto {
            ExportFormat::for_node_type(obj_type)
        } else {
            format
        };
        let file_extension = actual_format.api_extension();

        let mut body = json!({
            "token": obj_token,
            "type": obj_type,
            "file_extension": file_extension,
        });

        if let Some(sid) = sub_id {
            body["sub_id"] = serde_json::Value::String(sid.to_string());
        }

        println!("  📤 创建导出任务 | token={} | type={} | ext={}", obj_token, obj_type, file_extension);

        let resp = self.post("/open-apis/drive/v1/export_tasks", token, body).await?;
        let data: CreateExportTaskResponse = resp.json().await?;

        if data.code != 0 {
            let friendly = Self::friendly_error(obj_type, file_extension, data.code, &data.msg);
            return Err(Error::from_api_response(data.code, friendly));
        }

        println!("  ✅ ticket={}", data.data.ticket);
        Ok(data.data.ticket)
    }

    fn friendly_error(obj_type: &str, file_extension: &str, code: i32, msg: &str) -> String {
        if code == 1069918 {
            let supported = match obj_type {
                "sheet" | "bitable" => "xlsx / csv",
                _ => "docx / pdf",
            };
            format!("{} 不支持导出为 {}，仅支持: {}", obj_type, file_extension, supported)
        } else {
            format!("code={} msg={}", code, msg)
        }
    }

    /// 轮询导出任务直到完成，返回 file_token
    pub async fn poll_export_task(&self, token: &str, ticket: &str, obj_token: &str, _obj_type: &str) -> Result<String> {
        let max_attempts = 60;
        let initial_delay = 5;
        let delay = 5;

        tokio::time::sleep(tokio::time::Duration::from_secs(initial_delay)).await;

        for attempt in 1..=max_attempts {
            let url = format!(
                "/open-apis/drive/v1/export_tasks/{}?token={}",
                ticket, obj_token
            );

            if attempt == 1 || attempt % 10 == 0 {
                println!("  🔄 轮询导出状态 ({}/{})", attempt, max_attempts);
            }

            let resp = self.get(&url, token).await?;
            let data: ExportTaskStatusResponse = resp.json().await?;

            if data.code != 0 {
                let err = Error::from_api_response(data.code,
                    format!("poll_export_task ticket={}: {}", ticket, data.msg));
                return Err(err);
            }

            let is_complete = data.data.result.extra.is_complete == "true";
            let file_token = data.data.result.file_token.clone();

            if attempt == 1 || attempt % 10 == 0 {
                println!("    完成={} | file_token='{}' | ext={}",
                    is_complete, file_token, data.data.result.file_extension);
            }

            if is_complete {
                if file_token.is_empty() {
                    return Err(Error::ExportTimeout { token: obj_token.to_string() });
                }
                println!("  ✅ 导出完成");
                return Ok(file_token);
            }

            if attempt >= max_attempts {
                return Err(Error::ExportTimeout { token: obj_token.to_string() });
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(delay)).await;
        }

        Err(Error::ExportTimeout { token: obj_token.to_string() })
    }

    /// 下载导出的文件
    pub async fn download_export_file(&self, token: &str, file_token: &str) -> Result<reqwest::Response> {
        let url = format!("{}/open-apis/drive/v1/export_tasks/file/{}/download", self.base_url, file_token);
        let mut h = HeaderMap::new();
        h.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", token)).unwrap());
        let resp = self.http.get(&url).headers(h).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::ApiError {
                code: status.as_u16() as i32,
                msg: format!("download failed ({}): {}", status, &body[..body.len().min(300)]),
            });
        }
        Ok(resp)
    }

    /// 下载普通文件（file 类型）
    pub async fn download_file(&self, token: &str, file_token: &str) -> Result<reqwest::Response> {
        let url = format!("{}/open-apis/drive/v1/files/{}/download", self.base_url, file_token);
        let mut h = HeaderMap::new();
        h.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", token)).unwrap());
        let resp = self.http.get(&url).headers(h).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::ApiError {
                code: status.as_u16() as i32,
                msg: format!("file download failed ({}): {}", status, &body[..body.len().min(300)]),
            });
        }
        Ok(resp)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Combined export flows
    // ─────────────────────────────────────────────────────────────────────────

    /// 完整导出流程（自动处理 sheet/bitable）
    ///
    /// sheet/bitable 导出 CSV 时自动获取第一个工作表/数据表的 ID
    pub async fn export_document(
        &self,
        token: &str,
        obj_token: &str,
        obj_type: &str,
        format: ExportFormat,
    ) -> Result<reqwest::Response> {
        // CSV 导出 sheet/bitable 时必须传 sub_id
        let sub_id: Option<String> = if format == ExportFormat::Csv {
            match obj_type {
                "sheet" => Some(self.get_sheet_first_sheet_id(token, obj_token).await?),
                "bitable" => Some(self.get_bitable_first_table_id(token, obj_token).await?),
                _ => None,
            }
        } else {
            None
        };

        let ticket = self.create_export_task(token, obj_token, obj_type, format, sub_id.as_deref()).await?;
        let file_token = self.poll_export_task(token, &ticket, obj_token, obj_type).await?;
        self.download_export_file(token, &file_token).await
    }
}

impl Default for FeishuClient {
    fn default() -> Self { Self::new() }
}
