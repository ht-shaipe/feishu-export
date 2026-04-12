use crate::api::FeishuClient;
use crate::error::Result;

/// 文档内容 API
impl FeishuClient {
    /// 获取文档原始内容（用于 MD 转换）
    pub async fn get_document_raw_content(
        &self,
        access_token: &str,
        doc_token: &str,
    ) -> Result<serde_json::Value> {
        let url = format!("/open-apis/docx/v1/documents/{}/raw_content", doc_token);
        let response = self.get(&url, access_token).await?;
        let data: serde_json::Value = response.json().await?;

        if data["code"] != 0 {
            return Err(crate::error::FeishuError::ApiError {
                code: data["code"].as_i64().unwrap_or(-1) as i32,
                msg: data["msg"].as_str().unwrap_or("Unknown error").to_string(),
            });
        }

        Ok(data)
    }
}
