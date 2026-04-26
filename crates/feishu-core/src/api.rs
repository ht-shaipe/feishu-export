//! Feishu Open API client

use crate::error::{FeishuCoreError as Error, Result};
use crate::models::auth::*;
use crate::models::drive::*;
use crate::models::export::*;
use crate::models::permission::*;
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
        // 完整权限 scope（覆盖导出、权限管理、Wiki、多维表格）
        let scopes = [
            "wiki:wiki:readonly", "wiki:node:read", "wiki:node:retrieve",
            "wiki:node:write", "wiki:node:delete",
            "wiki:space:readonly", "wiki:space:write",
            "drive:drive:readonly", "drive:export:readonly",
            "drive:import", "drive:permission",
            "docx:document:readonly", "sheet:spreadsheet:readonly",
            "bitable:app:readonly", "offline_access",
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
    // Wiki Node Management
    // ─────────────────────────────────────────────────────────────────────────

    /// 在知识空间中创建节点（文档）
    pub async fn create_wiki_node(
        &self,
        token: &str,
        space_id: &str,
        title: &str,
        obj_type: &str,
        parent_node_token: Option<&str>,
    ) -> Result<WikiNodeCreateResult> {
        let mut body = json!({
            "node": {
                "title": title,
                "obj_type": obj_type,
                "node_type": "origin",
            }
        });
        if let Some(p) = parent_node_token {
            body["node"]["parent_node_token"] = serde_json::Value::String(p.to_string());
        }
        let url = format!("/open-apis/wiki/v2/spaces/{}/nodes", space_id);
        let resp = self.post(&url, token, body).await?;
        #[derive(Deserialize)]
        struct R { code: i32, msg: String, data: Option<WikiNodeCreateResultData> }
        #[derive(Deserialize)]
        struct WikiNodeCreateResultData {
            node: Option<crate::models::wiki::NodeInfo>,
        }
        let r: R = resp.json().await?;
        if r.code != 0 {
            return Err(Error::from_api_response(r.code, format!("create_wiki_node failed: {}", r.msg)));
        }
        let node = r.data.and_then(|d| d.node)
            .ok_or_else(|| Error::ApiError { code: -1, msg: "create_wiki_node: no node returned".to_string() })?;
        Ok(WikiNodeCreateResult {
            space_id: node.space_id.unwrap_or_default(),
            node_token: node.node_token,
            obj_token: node.obj_token,
            obj_type: node.obj_type,
        })
    }

    /// 更新知识库节点标题
    pub async fn update_wiki_node_title(
        &self,
        token: &str,
        space_id: &str,
        node_token: &str,
        title: &str,
    ) -> Result<()> {
        let url = format!("/open-apis/wiki/v2/spaces/{}/nodes/{}/update_title", space_id, node_token);
        let body = json!({ "title": title });
        let resp = self.post(&url, token, body).await?;
        #[derive(Deserialize)]
        struct R { code: i32, msg: String }
        let r: R = resp.json().await?;
        if r.code != 0 {
            return Err(Error::from_api_response(r.code, format!("update_wiki_node_title: {}", r.msg)));
        }
        Ok(())
    }

    /// 移动知识库节点到另一个空间或父节点
    pub async fn move_wiki_node(
        &self,
        token: &str,
        space_id: &str,
        node_token: &str,
        target_space_id: &str,
        target_parent: Option<&str>,
    ) -> Result<String> {
        let mut body = json!({ "target_space_id": target_space_id });
        if let Some(p) = target_parent {
            body["target_parent_token"] = serde_json::Value::String(p.to_string());
        }
        let url = format!("/open-apis/wiki/v2/spaces/{}/nodes/{}/move", space_id, node_token);
        let resp = self.post(&url, token, body).await?;
        #[derive(Deserialize)]
        struct R { code: i32, msg: String, data: Option<MoveWikiNodeData> }
        #[derive(Deserialize)]
        struct MoveWikiNodeData { node: Option<serde_json::Value> }
        let r: R = resp.json().await?;
        if r.code != 0 {
            return Err(Error::from_api_response(r.code, format!("move_wiki_node: {}", r.msg)));
        }
        Ok(node_token.to_string())
    }

    /// 获取知识空间详情
    pub async fn get_wiki_space(&self, token: &str, space_id: &str) -> Result<WikiSpaceDetail> {
        let url = format!("/open-apis/wiki/v2/spaces/{}", space_id);
        let resp = self.get(&url, token).await?;
        #[derive(Deserialize)]
        struct R { code: i32, msg: String, data: Option<WikiSpaceDetailData> }
        #[derive(Deserialize)]
        struct WikiSpaceDetailData { space: Option<crate::models::wiki::WikiSpaceDetail> }
        let r: R = resp.json().await?;
        r.data.and_then(|d| d.space)
            .ok_or_else(|| Error::ApiError { code: -1, msg: "get_wiki_space: no space returned".to_string() })
    }

    /// 列出知识空间成员
    pub async fn list_wiki_space_members(
        &self,
        token: &str,
        space_id: &str,
    ) -> Result<Vec<WikiSpaceMember>> {
        let mut members = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let mut url = format!("/open-apis/wiki/v2/spaces/{}/members?page_size=50", space_id);
            if let Some(ref t) = page_token {
                url.push_str(&format!("&page_token={}", t));
            }
            let resp = self.get(&url, token).await?;
            #[derive(Deserialize)]
            struct R { code: i32, msg: String, data: Option<WikiMembersData> }
            #[derive(Deserialize)]
            struct WikiMembersData {
                members: Vec<crate::models::wiki::WikiSpaceMember>,
                #[serde(default)]
                pub page_token: Option<String>,
                #[serde(default)]
                pub has_more: bool,
            }
            let r: R = resp.json().await?;
            if r.code != 0 {
                return Err(Error::from_api_response(r.code, format!("list_wiki_space_members: {}", r.msg)));
            }
            if let Some(d) = r.data {
                members.extend(d.members);
                if !d.has_more {
                    break;
                }
                page_token = d.page_token;
            } else {
                break;
            }
        }
        Ok(members)
    }

    /// 添加知识空间成员
    pub async fn add_wiki_space_member(
        &self,
        token: &str,
        space_id: &str,
        member_type: &str,
        member_id: &str,
        member_role: &str,
    ) -> Result<()> {
        let url = format!("/open-apis/wiki/v2/spaces/{}/members", space_id);
        let body = json!({
            "member": {
                "member_type": member_type,
                "member_id": member_id,
                "member_role": member_role,
            },
            "need_notification": true,
        });
        let resp = self.post(&url, token, body).await?;
        #[derive(Deserialize)]
        struct R { code: i32, msg: String }
        let r: R = resp.json().await?;
        if r.code != 0 {
            return Err(Error::from_api_response(r.code, format!("add_wiki_space_member: {}", r.msg)));
        }
        Ok(())
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
            return Err(Error::ApiError { code: data.code, msg: format!("get_sheet_first_sheet_id: {}", data.msg) });
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
            return Err(Error::ApiError { code: data.code, msg: format!("get_bitable_first_table_id: {}", data.msg) });
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
                return Err(Error::from_api_response(data.code,
                    format!("poll_export_task ticket={}: {}", ticket, data.msg)));
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

    // ─────────────────────────────────────────────────────────────────────────
    // Drive File API
    // ─────────────────────────────────────────────────────────────────────────

    /// 列出文件夹中的文件
    pub async fn list_files(
        &self,
        token: &str,
        folder_token: Option<&str>,
    ) -> Result<Vec<DriveFile>> {
        let mut files = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let mut url = "/open-apis/drive/v1/files?page_size=50".to_string();
            if let Some(t) = folder_token {
                url.push_str(&format!("&folder_token={}", t));
            }
            if let Some(ref t) = page_token {
                url.push_str(&format!("&page_token={}", t));
            }

            let resp = self.get(&url, token).await?;
            let data: ListFilesResponse = resp.json().await?;
            if data.code != 0 {
                return Err(Error::from_api_response(data.code, format!("list_files: {}", data.msg)));
            }
            files.extend(data.data.files);
            if !data.data.has_more {
                break;
            }
            page_token = data.data.page_token;
        }
        Ok(files)
    }

    /// 创建文件夹
    pub async fn create_folder(&self, token: &str, name: &str, parent_folder_token: &str) -> Result<(String, String)> {
        let body = json!({
            "name": name,
            "folder_token": parent_folder_token,
        });
        let resp = self.post("/open-apis/drive/v1/files/create_folder", token, body).await?;
        let data: CreateFolderResponse = resp.json().await?;
        if data.code != 0 {
            return Err(Error::from_api_response(data.code, format!("create_folder: {}", data.msg)));
        }
        Ok((
            data.data.token.unwrap_or_default(),
            data.data.url.unwrap_or_default(),
        ))
    }

    /// 移动文件或文件夹到目标文件夹
    pub async fn move_file(&self, token: &str, file_token: &str, file_type: &str, target_folder_token: &str) -> Result<Option<String>> {
        let body = json!({
            "type": file_type,
            "folder_token": target_folder_token,
        });
        let url = format!("/open-apis/drive/v1/files/{}/move", file_token);
        let resp = self.post(&url, token, body).await?;
        let data: MoveFileResponse = resp.json().await?;
        if data.code != 0 {
            return Err(Error::from_api_response(data.code, format!("move_file: {}", data.msg)));
        }
        Ok(data.data.task_id)
    }

    /// 复制文件
    pub async fn copy_file(
        &self,
        token: &str,
        file_token: &str,
        file_type: &str,
        target_folder_token: &str,
        new_name: Option<&str>,
    ) -> Result<(String, String)> {
        let mut body = json!({
            "type": file_type,
            "folder_token": target_folder_token,
        });
        if let Some(n) = new_name {
            body["name"] = serde_json::Value::String(n.to_string());
        }
        let url = format!("/open-apis/drive/v1/files/{}/copy", file_token);
        let resp = self.post(&url, token, body).await?;
        let data: CopyFileResponse = resp.json().await?;
        if data.code != 0 {
            return Err(Error::from_api_response(data.code, format!("copy_file: {}", data.msg)));
        }
        let f = data.data.file;
        let f = f.ok_or_else(|| Error::ApiError { code: -1, msg: "copy_file: no file returned".to_string() })?;
        Ok((
            f.token.clone(),
            f.url.clone().unwrap_or_default(),
        ))
    }

    /// 删除文件或文件夹
    pub async fn delete_file(&self, token: &str, file_token: &str, file_type: &str) -> Result<Option<String>> {
        let url = format!("/open-apis/drive/v1/files/{}/delete?type={}", file_token, file_type);
        // DELETE with body — use post with empty json
        let body = json!({});
        let resp = self.post(&url, token, body).await?;
        let data: DeleteFileResponse = resp.json().await?;
        if data.code != 0 {
            return Err(Error::from_api_response(data.code, format!("delete_file: {}", data.msg)));
        }
        Ok(data.data.task_id)
    }

    /// 创建文件快捷方式
    pub async fn create_shortcut(
        &self,
        token: &str,
        parent_folder_token: &str,
        target_file_token: &str,
        target_type: &str,
    ) -> Result<(String, String)> {
        let body = json!({
            "parent_token": parent_folder_token,
            "refer_entity": {
                "refer_token": target_file_token,
                "refer_type": target_type,
            }
        });
        let resp = self.post("/open-apis/drive/v1/files/create_shortcut", token, body).await?;
        let data: CreateShortcutResponse = resp.json().await?;
        if data.code != 0 {
            return Err(Error::from_api_response(data.code, format!("create_shortcut: {}", data.msg)));
        }
        let node = data.data.succ_shortcut_node;
        Ok((
            node.as_ref().and_then(|n| n.token.clone()).unwrap_or_default(),
            node.and_then(|n| n.parent_token).unwrap_or_default(),
        ))
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Media API
    // ─────────────────────────────────────────────────────────────────────────

    /// 批量获取媒体文件的临时下载链接
    pub async fn batch_get_tmp_download_url(
        &self,
        token: &str,
        file_tokens: &[String],
    ) -> Result<Vec<(String, String)>> {
        let body = json!({
            "file_tokens": file_tokens,
        });
        let resp = self.post("/open-apis/drive/v1/medias/batch_get_tmp_download_url", token, body).await?;
        let data: BatchGetTmpDownloadUrlResponse = resp.json().await?;
        if data.code != 0 {
            return Err(Error::from_api_response(data.code, format!("batch_get_tmp_download_url: {}", data.msg)));
        }
        let urls: Vec<(String, String)> = data.data.tmp_download_urls
            .into_iter()
            .filter_map(|u| {
                let tk = u.file_token?;
                let url = u.tmp_download_url?;
                Some((tk, url))
            })
            .collect();
        Ok(urls)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Meta / Statistics API
    // ─────────────────────────────────────────────────────────────────────────

    /// 批量获取文件元数据
    pub async fn batch_get_meta(
        &self,
        token: &str,
        doc_tokens: &[String],
        doc_type: &str,
    ) -> Result<Vec<FileMeta>> {
        let request_docs: Vec<serde_json::Value> = doc_tokens
            .iter()
            .map(|t| json!({ "doc_token": t, "doc_type": doc_type }))
            .collect();

        let body = json!({
            "request_docs": request_docs,
            "with_url": true,
        });
        let resp = self.post("/open-apis/drive/v1/metas/batch_query", token, body).await?;
        let data: BatchGetMetaResponse = resp.json().await?;
        if data.code != 0 {
            return Err(Error::from_api_response(data.code, format!("batch_get_meta: {}", data.msg)));
        }
        Ok(data.data.metas)
    }

    /// 获取文件统计信息（阅读数、编辑数等）
    pub async fn get_file_statistics(
        &self,
        token: &str,
        file_token: &str,
        file_type: &str,
    ) -> Result<FileStats> {
        let url = format!(
            "/open-apis/drive/v1/files/{}/statistics?file_type={}",
            file_token, file_type
        );
        let resp = self.get(&url, token).await?;
        let data: FileStatisticsResponse = resp.json().await?;
        if data.code != 0 {
            return Err(Error::from_api_response(data.code, format!("get_file_statistics: {}", data.msg)));
        }
        data.data.statistics
            .ok_or_else(|| Error::ApiError { code: -1, msg: "no statistics returned".to_string() })
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Import API
    // ─────────────────────────────────────────────────────────────────────────

    /// 上传本地文件用于导入飞书（通过 medias/upload_all，临时文件不过度占用云盘）
    ///
    /// 返回 file_token，供后续 create_import_task 使用
    pub async fn upload_media_for_import(
        &self,
        token: &str,
        file_path: &str,
        file_name: &str,
        obj_type: &str,
        file_extension: &str,
    ) -> Result<String> {
        let file_name = file_name.to_string();
        use std::io::Read as _;
        let mut file = std::fs::File::open(file_path)
            .map_err(|e| Error::IoError(e))?;
        use std::io::Read;
        let mut file = file;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .map_err(|e| Error::IoError(e))?;
        let file_size = buffer.len() as usize;
        let bytes = bytes::Bytes::from(buffer);

        // multipart 上传 — 使用 reqwest multipart
        use reqwest::multipart;
        let part = multipart::Part::stream(bytes)
            .file_name(file_name.to_string())
            .mime_str("application/octet-stream")
            .map_err(|e| Error::ApiError { code: -1, msg: format!("multipart error: {}", e) })?;

        let form = reqwest::multipart::Form::new()
            .text("file_name", file_name)
            .text("parent_type", "ccm_import_open")
            .text("parent_node", "ccm_import_open")
            .text("size", file_size.to_string())
            .text("extra", format!(r#"{{"obj_type":"{}","file_extension":"{}"}}"#, obj_type, file_extension))
            .part("file", part);

        let url = format!("{}/open-apis/drive/v1/medias/upload_all", self.base_url);
        let mut h = HeaderMap::new();
        h.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", token)).unwrap());

        let resp = self.http.post(&url)
            .headers(h)
            .multipart(form)
            .send().await?;

        // 先读 body，再检查响应（check_response 会 consume resp）
        let body = resp.text().await
            .map_err(|e| Error::ApiError { code: -1, msg: format!("read upload resp: {}", e) })?;
        // 不调 check_response（resp 已被 consume），status 200 已由 send().await? 保证

        #[derive(Deserialize)]
        struct UploadResp {
            code: i32,
            msg: String,
            data: Option<UploadData>,
        }
        #[derive(Deserialize)]
        struct UploadData { file_token: Option<String> }
        let r: UploadResp = serde_json::from_str(&body)
            .map_err(|e| Error::JsonError(e))?;
        if r.code != 0 {
            return Err(Error::from_api_response(r.code, format!("upload_media_for_import: {}", r.msg)));
        }
        r.data.and_then(|d| d.file_token)
            .ok_or_else(|| Error::ApiError { code: -1, msg: "upload_media_for_import: no file_token returned".to_string() })
    }

    /// 创建导入任务
    pub async fn create_import_task(
        &self,
        token: &str,
        file_token: &str,
        file_extension: &str,
        target_parent_type: &str,
        target_parent_node: &str,
        target_file_name: Option<&str>,
    ) -> Result<String> {
        let mut body = json!({
            "file_extension": file_extension,
            "target_parent_type": target_parent_type,
            "target_parent_node": target_parent_node,
        });
        if let Some(name) = target_file_name {
            body["target_file_name"] = serde_json::Value::String(name.to_string());
        }
        let url = format!("/open-apis/drive/v1/import_tasks?file_token={}", file_token);
        let resp = self.post(&url, token, body).await?;
        let data: CreateImportTaskResponse = resp.json().await?;
        if data.code != 0 {
            return Err(Error::from_api_response(data.code, format!("create_import_task: {}", data.msg)));
        }
        data.data.ticket
            .ok_or_else(|| Error::ApiError { code: -1, msg: "create_import_task: no ticket returned".to_string() })
    }

    /// 轮询导入任务直到完成，返回 file_token
    pub async fn poll_import_task(&self, token: &str, ticket: &str) -> Result<String> {
        let max_attempts = 60;
        let delay = 5;

        for attempt in 1..=max_attempts {
            let url = format!("/open-apis/drive/v1/import_tasks/{}", ticket);
            let resp = self.get(&url, token).await?;
            let data: QueryImportTaskResponse = resp.json().await?;

            if data.code != 0 {
                return Err(Error::from_api_response(data.code,
                    format!("poll_import_task {}: {}", ticket, data.msg)));
            }

            let status = data.data.result.job_status.unwrap_or(1);
            if attempt == 1 || attempt % 10 == 0 {
                println!("  🔄 轮询导入任务 ({}/{}) status={}", attempt, max_attempts, status);
            }

            if status == 0 {
                // 成功
                let ft = data.data.result.file_token.clone()
                    .ok_or_else(|| Error::ApiError { code: -1, msg: "poll_import_task: no file_token on success".to_string() })?;
                println!("  ✅ 导入完成 file_token={}", ft);
                return Ok(ft);
            }

            if status > 2 {
                let err = data.data.result.job_error_msg.clone()
                    .unwrap_or_else(|| format!("job_status={}", status));
                return Err(Error::ApiError { code: status, msg: format!("import failed: {}", err) });
            }

            if attempt >= max_attempts {
                return Err(Error::ApiError { code: -1, msg: format!("import timeout after {} attempts", max_attempts) });
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(delay)).await;
        }

        Err(Error::ApiError { code: -1, msg: "poll_import_task: unexpected exit".to_string() })
    }

    /// 导入本地文件到飞书云空间
    pub async fn import_file(
        &self,
        token: &str,
        file_path: &str,
        file_name: &str,
        obj_type: &str,
        file_extension: &str,
        parent_type: &str,
        parent_node: &str,
        target_name: Option<&str>,
    ) -> Result<String> {
        let file_token = self.upload_media_for_import(token, file_path, file_name, obj_type, file_extension).await?;
        let ticket = self.create_import_task(token, &file_token, file_extension, parent_type, parent_node, target_name).await?;
        self.poll_import_task(token, &ticket).await
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Permission API
    // ─────────────────────────────────────────────────────────────────────────

    /// 添加文档协作者权限
    pub async fn add_permission(
        &self,
        token: &str,
        doc_token: &str,
        doc_type: &str,
        member_type: &str,
        member_id: &str,
        perm: &str,
        notify: bool,
    ) -> Result<()> {
        let body = json!({
            "member_type": member_type,
            "member_id": member_id,
            "perm": perm,
            "need_notification": notify,
        });
        let url = format!("/open-apis/drive/v1/permissions/{}/members?type={}", doc_token, doc_type);
        let resp = self.post(&url, token, body).await?;
        let data: CreatePermissionResponse = resp.json().await?;
        if data.code != 0 {
            return Err(Error::from_api_response(data.code, format!("add_permission: {}", data.msg)));
        }
        Ok(())
    }

    /// 列出文档的所有权限成员
    pub async fn list_permission(&self, token: &str, doc_token: &str, doc_type: &str) -> Result<Vec<PermissionMember>> {
        let url = format!("/open-apis/drive/v1/permissions/{}/members?type={}", doc_token, doc_type);
        let resp = self.get(&url, token).await?;
        let data: ListPermissionResponse = resp.json().await?;
        if data.code != 0 {
            return Err(Error::from_api_response(data.code, format!("list_permission: {}", data.msg)));
        }
        Ok(data.data.items)
    }

    /// 删除文档权限成员
    pub async fn delete_permission(
        &self,
        token: &str,
        doc_token: &str,
        doc_type: &str,
        member_type: &str,
        member_id: &str,
    ) -> Result<()> {
        let url = format!(
            "/open-apis/drive/v1/permissions/{}/members/{}?type={}&member_type={}",
            doc_token, member_id, doc_type, member_type
        );
        let body = json!({});
        let resp = self.post(&url, token, body).await?;
        let data: DeletePermissionResponse = resp.json().await?;
        if data.code != 0 {
            return Err(Error::from_api_response(data.code, format!("delete_permission: {}", data.msg)));
        }
        Ok(())
    }

    /// 更新权限成员的权限级别
    pub async fn update_permission(
        &self,
        token: &str,
        doc_token: &str,
        doc_type: &str,
        member_type: &str,
        member_id: &str,
        perm: &str,
    ) -> Result<()> {
        let body = json!({
            "member_type": member_type,
            "member_id": member_id,
            "perm": perm,
        });
        let url = format!(
            "/open-apis/drive/v1/permissions/{}/members/{}?type={}",
            doc_token, member_id, doc_type
        );
        let resp = self.post(&url, token, body).await?;
        let data: UpdatePermissionResponse = resp.json().await?;
        if data.code != 0 {
            return Err(Error::from_api_response(data.code, format!("update_permission: {}", data.msg)));
        }
        Ok(())
    }

    /// 批量添加协作者权限
    pub async fn batch_add_permission(
        &self,
        token: &str,
        doc_token: &str,
        doc_type: &str,
        members: Vec<(String, String, String)>, // (member_type, member_id, perm)
        notify: bool,
    ) -> Result<()> {
        let member_list: Vec<serde_json::Value> = members
            .into_iter()
            .map(|(mt, mid, p)| {
                json!({
                    "member_type": mt,
                    "member_id": mid,
                    "perm": p,
                })
            })
            .collect();

        let body = json!({
            "members": member_list,
            "need_notification": notify,
        });
        let url = format!("/open-apis/drive/v1/permissions/{}/members/batch_create?type={}", doc_token, doc_type);
        let resp = self.post(&url, token, body).await?;
        let data: BatchAddPermissionResponse = resp.json().await?;
        if data.code != 0 {
            return Err(Error::from_api_response(data.code, format!("batch_add_permission: {}", data.msg)));
        }
        Ok(())
    }

    /// 转移文档所有权
    pub async fn transfer_ownership(
        &self,
        token: &str,
        doc_token: &str,
        doc_type: &str,
        member_type: &str,
        member_id: &str,
        notify: bool,
        remove_old_owner: bool,
        old_owner_perm: Option<&str>,
    ) -> Result<()> {
        let mut body = json!({
            "need_notification": notify,
            "remove_old_owner": remove_old_owner,
            "owner": {
                "member_type": member_type,
                "member_id": member_id,
            },
        });
        if let Some(p) = old_owner_perm {
            body["old_owner_perm"] = serde_json::Value::String(p.to_string());
        }
        let url = format!("/open-apis/drive/v1/permissions/{}/members/transfer_owner?type={}", doc_token, doc_type);
        let resp = self.post(&url, token, body).await?;
        let data: TransferOwnerResponse = resp.json().await?;
        if data.code != 0 {
            return Err(Error::from_api_response(data.code, format!("transfer_ownership: {}", data.msg)));
        }
        Ok(())
    }

    /// 获取文档公共权限设置
    pub async fn get_public_permission(&self, token: &str, doc_token: &str, doc_type: &str) -> Result<PermissionPublic> {
        let url = format!("/open-apis/drive/v1/permissions/{}/public?type={}", doc_token, doc_type);
        let resp = self.get(&url, token).await?;
        let data: GetPublicPermissionResponse = resp.json().await?;
        if data.code != 0 {
            return Err(Error::from_api_response(data.code, format!("get_public_permission: {}", data.msg)));
        }
        data.data.permission_public
            .ok_or_else(|| Error::ApiError { code: -1, msg: "get_public_permission: no data".to_string() })
    }

    /// 更新文档公共权限设置
    pub async fn patch_public_permission(
        &self,
        token: &str,
        doc_token: &str,
        doc_type: &str,
        external_access: Option<bool>,
        link_share_entity: Option<&str>,
        security_entity: Option<&str>,
        comment_entity: Option<&str>,
        share_entity: Option<&str>,
        invite_external: Option<bool>,
    ) -> Result<PermissionPublic> {
        let mut body = serde_json::Map::new();
        if let Some(v) = external_access {
            body.insert("external_access".to_string(), serde_json::Value::Bool(v));
        }
        if let Some(v) = link_share_entity {
            body.insert("link_share_entity".to_string(), serde_json::Value::String(v.to_string()));
        }
        if let Some(v) = security_entity {
            body.insert("security_entity".to_string(), serde_json::Value::String(v.to_string()));
        }
        if let Some(v) = comment_entity {
            body.insert("comment_entity".to_string(), serde_json::Value::String(v.to_string()));
        }
        if let Some(v) = share_entity {
            body.insert("share_entity".to_string(), serde_json::Value::String(v.to_string()));
        }
        if let Some(v) = invite_external {
            body.insert("invite_external".to_string(), serde_json::Value::Bool(v));
        }

        let url = format!("/open-apis/drive/v1/permissions/{}/public?type={}", doc_token, doc_type);
        let resp = self.post(&url, token, serde_json::Value::Object(body)).await?;
        let data: PatchPublicPermissionResponse = resp.json().await?;
        if data.code != 0 {
            return Err(Error::from_api_response(data.code, format!("patch_public_permission: {}", data.msg)));
        }
        data.data.permission_public
            .ok_or_else(|| Error::ApiError { code: -1, msg: "patch_public_permission: no data".to_string() })
    }

    /// 判断当前用户对文档是否有指定权限
    pub async fn auth_permission(
        &self,
        token: &str,
        doc_token: &str,
        doc_type: &str,
        action: &str,
    ) -> Result<bool> {
        let url = format!(
            "/open-apis/drive/v1/permissions/{}/members/auth?type={}&action={}",
            doc_token, doc_type, action
        );
        let resp = self.get(&url, token).await?;
        let data: AuthPermissionResponse = resp.json().await?;
        if data.code != 0 {
            return Err(Error::from_api_response(data.code, format!("auth_permission: {}", data.msg)));
        }
        Ok(data.data.has_permission.unwrap_or(false))
    }
}

impl Default for FeishuClient {
    fn default() -> Self { Self::new() }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper types
// ─────────────────────────────────────────────────────────────────────────────

/// 创建 Wiki 节点返回结果
pub struct WikiNodeCreateResult {
    pub space_id: String,
    pub node_token: String,
    pub obj_token: String,
    pub obj_type: String,
}
