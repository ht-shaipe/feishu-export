use crate::api::FeishuClient;
use crate::error::{FeishuError, Result};
use crate::models::export::*;
use serde_json::json;
use std::time::Duration;
use tokio::time::sleep;

/// 导出 API
impl FeishuClient {
    /// 创建导出任务
    pub async fn create_export_task(
        &self,
        access_token: &str,
        obj_token: &str,
        obj_type: &str,
        format: ExportFormat,
    ) -> Result<String> {
        // 将 obj_type 映射到 API 的 type 参数
        let doc_type = match obj_type {
            "docx" => "docx",
            "doc" => "doc",
            "sheet" => "sheet",
            "bitable" => "bitable",
            "file" => "file",
            _ => {
                return Err(FeishuError::UnsupportedType {
                    doc_type: obj_type.to_string(),
                })
            }
        };

        let body = json!({
            "token": obj_token,
            "type": doc_type,
            "file_extension": format.api_extension(),
        });

        eprintln!("[API] POST /open-apis/drive/v1/export_tasks token={} type={}", obj_token, doc_type);
        let response = self
            .post("/open-apis/drive/v1/export_tasks", access_token, body)
            .await?;

        // 读取 body 并克隆一份用于日志+解析（不消费原始 response）
        let raw_text = response.text().await?;
        eprintln!("[API] Response: {}", raw_text.chars().take(200).collect::<String>());

        let data: CreateExportTaskResponse = serde_json::from_str(&raw_text).map_err(|e| {
            eprintln!("[API] JSON parse error: {}", e);
            FeishuError::ApiError { code: -1, msg: format!("JSON parse error: {}", e) }
        })?;

        if data.code != 0 {
            return Err(FeishuError::from_api_response(
                data.code,
                format!("Failed to create export task for token {}: {}", obj_token, &raw_text),
            ));
        }

        Ok(data.data.ticket)
    }

    /// 查询导出任务状态
    pub async fn get_export_task_status(
        &self,
        access_token: &str,
        ticket: &str,
        obj_token: &str,
    ) -> Result<ExportTaskStatusData> {
        let url = format!(
            "/open-apis/drive/v1/export_tasks/{}?token={}",
            ticket, obj_token
        );
        let response = self.get(&url, access_token).await?;
        let raw_text = response.text().await?;
        eprintln!("[API] Poll response: {}", raw_text.chars().take(200).collect::<String>());
        let data: ExportTaskStatusResponse = serde_json::from_str(&raw_text).map_err(|e| {
            FeishuError::ApiError { code: -1, msg: format!("Poll JSON parse error: {} | raw: {}", e, &raw_text.chars().take(100).collect::<String>()) }
        })?;

        if data.code != 0 {
            return Err(FeishuError::from_api_response(
                data.code,
                format!("Failed to get export task status for ticket {}", ticket),
            ));
        }

        Ok(data.data)
    }

    /// 轮询导出任务直到完成
    pub async fn poll_export_task(
        &self,
        access_token: &str,
        ticket: &str,
        obj_token: &str,
    ) -> Result<String> {
        let mut attempts = 0;
        let max_attempts = 30; // 最多轮询 30 次（约 60 秒）

        // 首次等待 3 秒
        sleep(Duration::from_secs(3)).await;

        loop {
            attempts += 1;
            eprintln!("[API] Poll attempt {}/{} for ticket={} token={}", attempts, max_attempts, ticket, obj_token);
            let status = self
                .get_export_task_status(access_token, ticket, obj_token)
                .await?;

            match status.status.as_str() {
                "success" => {
                    if status.file_token.is_empty() {
                        return Err(FeishuError::ExportTimeout {
                            token: obj_token.to_string(),
                        });
                    }
                    return Ok(status.file_token);
                }
                "failed" => {
                    return Err(FeishuError::ApiError {
                        code: -1,
                        msg: format!(
                            "Export failed for token {}: {}",
                            obj_token, status.error_message
                        ),
                    });
                }
                "pending" | "processing" => {
                    if attempts >= max_attempts {
                        return Err(FeishuError::ExportTimeout {
                            token: obj_token.to_string(),
                        });
                    }
                    // 指数退避：2秒，4秒，...
                    let delay = 2u64.pow(attempts.min(5)) * 1000;
                    sleep(Duration::from_millis(delay)).await;
                }
                _ => {
                    return Err(FeishuError::ApiError {
                        code: -1,
                        msg: format!("Unknown export status: {}", status.status),
                    });
                }
            }
        }
    }

    /// 下载导出文件
    pub async fn download_export_file(
        &self,
        access_token: &str,
        file_token: &str,
    ) -> Result<reqwest::Response> {
        let url = format!(
            "{}/open-apis/drive/v1/export_tasks/file/{}/download",
            self.base_url(),
            file_token
        );

        let response = self.download(&url, access_token).await?;

        // 先保存 status 再读 body（response 在 .text() 后不可再访问）
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await?;
            return Err(FeishuError::ApiError {
                code: status.as_u16() as i32,
                msg: format!(
                    "Download failed ({}): {}",
                    status,
                    body.chars().take(300).collect::<String>()
                ),
            });
        }

        // 状态 OK，保留 Response 给调用方读 bytes（二进制文件流）
        Ok(response)
    }

    /// 完整导出流程：创建任务 -> 轮询 -> 下载
    pub async fn export_document(
        &self,
        access_token: &str,
        obj_token: &str,
        obj_type: &str,
        format: ExportFormat,
    ) -> Result<reqwest::Response> {
        // Step 1: 创建导出任务
        let ticket = self
            .create_export_task(access_token, obj_token, obj_type, format)
            .await?;

        // Step 2: 轮询任务状态
        let file_token = self
            .poll_export_task(access_token, &ticket, obj_token)
            .await?;

        // Step 3: 下载文件
        eprintln!("[API] Downloading file_token={}", file_token);
        let response = self.download_export_file(access_token, &file_token).await?;
        eprintln!("[API] Download OK, content-length: {:?}", response.content_length());
        Ok(response)
    }
}
