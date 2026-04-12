use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;

/// 导出格式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Docx,
    Pdf,
    Md,
    Xlsx,
    Csv,
    Auto,
}

impl ExportFormat {
    /// 获取文件扩展名
    pub fn extension(&self) -> &str {
        match self {
            ExportFormat::Docx => "docx",
            ExportFormat::Pdf => "pdf",
            ExportFormat::Md => "md",
            ExportFormat::Xlsx => "xlsx",
            ExportFormat::Csv => "csv",
            ExportFormat::Auto => "auto",
        }
    }

    /// 获取对应的飞书 API file_extension 参数
    /// md 格式没有直接的 API 支持，必须先导出 docx 再转 md
    pub fn api_extension(&self) -> &str {
        match self {
            ExportFormat::Md => "docx",
            _ => self.extension(),
        }
    }

    /// 根据节点类型自动选择最佳导出格式
    pub fn for_node_type(obj_type: &str) -> Self {
        match obj_type {
            "sheet" | "bitable" => ExportFormat::Xlsx,
            "docx" | "doc" | "mindnote" | "slides" => ExportFormat::Md,
            "file" | "folder" | "shortcut" | "image" | "video" | "audio"
            | "wiki" | "minutable" | "whiteboard" => ExportFormat::Auto, // 不可导出
            _ => ExportFormat::Docx,
        }
    }

    /// 是否需要做格式转换（md 需要先导出 docx 再转）
    pub fn needs_conversion(&self) -> bool {
        matches!(self, ExportFormat::Md)
    }

    /// 从字符串解析导出格式
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "docx" => Some(ExportFormat::Docx),
            "pdf" => Some(ExportFormat::Pdf),
            "md" | "markdown" => Some(ExportFormat::Md),
            "xlsx" => Some(ExportFormat::Xlsx),
            "csv" => Some(ExportFormat::Csv),
            "auto" | "" => Some(ExportFormat::Auto),
            _ => None,
        }
    }
}

/// 导出任务状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportStatus {
    Pending,
    Creating,
    Polling,
    Downloading,
    Converting,
    Completed,
    Failed(String),
}

impl ExportStatus {
    pub fn is_final(&self) -> bool {
        matches!(self, ExportStatus::Completed | ExportStatus::Failed(_))
    }
}

/// 导出任务
#[derive(Debug, Clone)]
pub struct ExportTask {
    pub node_token: String,
    pub obj_token: String,
    pub title: String,
    pub format: ExportFormat,
    pub status: ExportStatus,
    pub ticket: Option<String>,
    pub file_token: Option<String>,
    pub retry_count: u32,
}

impl ExportTask {
    pub fn new(node_token: String, obj_token: String, title: String, format: ExportFormat) -> Self {
        Self {
            node_token,
            obj_token,
            title,
            format,
            status: ExportStatus::Pending,
            ticket: None,
            file_token: None,
            retry_count: 0,
        }
    }

    pub fn is_retryable(&self) -> bool {
        self.retry_count < 3 && !self.status.is_final()
    }
}

/// 导出进度
#[derive(Debug, Clone, Default)]
pub struct ExportProgress {
    pub total: usize,
    pub completed: usize,
    pub skipped: usize,
    pub failed: usize,
}

impl ExportProgress {
    pub fn new(total: usize) -> Self {
        Self {
            total,
            ..Default::default()
        }
    }

    pub fn increment_completed(&mut self) {
        self.completed += 1;
    }

    pub fn increment_skipped(&mut self) {
        self.skipped += 1;
    }

    pub fn increment_failed(&mut self) {
        self.failed += 1;
    }

    pub fn is_complete(&self) -> bool {
        self.completed + self.skipped + self.failed == self.total
    }

    pub fn success_rate(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        (self.completed as f64) / (self.total as f64)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 导出记录（exports.log）
// ─────────────────────────────────────────────────────────────────────────────

/// 单条导出记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportLogEntry {
    /// ISO 8601 时间戳
    pub timestamp: String,
    /// 文档标题
    pub title: String,
    /// 文档 obj_token
    pub obj_token: String,
    /// 文档类型 docx / sheet / bitable / ...
    pub obj_type: String,
    /// 成功 / 失败 / 跳过
    pub status: String,
    /// 成功时：本地相对路径；失败时：空
    pub local_path: Option<String>,
    /// 失败时：错误信息；成功时：空
    pub error: Option<String>,
}

impl ExportLogEntry {
    pub fn success(title: String, obj_token: String, obj_type: String, local_path: String) -> Self {
        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            title,
            obj_token,
            obj_type,
            status: "success".to_string(),
            local_path: Some(local_path),
            error: None,
        }
    }

    pub fn failed(title: String, obj_token: String, obj_type: String, error: String) -> Self {
        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            title,
            obj_token,
            obj_type,
            status: "failed".to_string(),
            local_path: None,
            error: Some(error),
        }
    }

    /// 写入一行 JSON Line 到 writer
    pub fn write_jsonl<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<()> {
        let line = serde_json::to_string(self).unwrap_or_else(|_| String::from("{}"));
        writeln!(w, "{}", line)
    }
}

/// 导出日志（流式追加写入）
#[derive(Clone)]
pub struct ExportLog {
    path: std::path::PathBuf,
}

impl ExportLog {
    pub fn new(output_dir: &std::path::Path, space_id: &str) -> std::io::Result<Self> {
        let log_path = output_dir.join("exports.log");
        // 写入文件头注释
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?;
        writeln!(file, "# feishu-export log — space_id={} started={}", space_id, chrono::Utc::now().to_rfc3339())?;
        drop(file);
        Ok(Self { path: log_path })
    }

    /// 追加一条成功记录
    pub fn append_success(
        &self,
        title: &str,
        obj_token: &str,
        obj_type: &str,
        local_path: &std::path::Path,
    ) -> std::io::Result<()> {
        let entry = ExportLogEntry::success(
            title.to_string(),
            obj_token.to_string(),
            obj_type.to_string(),
            local_path.display().to_string(),
        );
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        entry.write_jsonl(&mut file)
    }

    /// 追加一条失败记录
    pub fn append_failed(
        &self,
        title: &str,
        obj_token: &str,
        obj_type: &str,
        error: &str,
    ) -> std::io::Result<()> {
        let entry = ExportLogEntry::failed(
            title.to_string(),
            obj_token.to_string(),
            obj_type.to_string(),
            error.to_string(),
        );
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        entry.write_jsonl(&mut file)
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

/// 创建导出任务请求
#[derive(Debug, Serialize)]
pub struct CreateExportTaskRequest {
    pub token: String,
    #[serde(rename = "type")]
    pub doc_type: String,
    pub file_extension: String,
}

/// 创建导出任务响应
#[derive(Debug, Deserialize)]
pub struct CreateExportTaskResponse {
    pub code: i32,
    pub data: CreateExportTaskData,
}

#[derive(Debug, Deserialize)]
pub struct CreateExportTaskData {
    pub ticket: String,
}

/// 查询导出任务响应
#[derive(Debug, Deserialize)]
pub struct ExportTaskStatusResponse {
    pub code: i32,
    pub data: ExportTaskStatusData,
}

#[derive(Debug, Deserialize)]
pub struct ExportTaskStatusData {
    pub status: String,
    #[serde(default)]
    pub file_token: String,
    #[serde(default)]
    pub error_code: String,
    #[serde(default)]
    pub error_message: String,
}

/// 导出结果
#[derive(Debug, Clone)]
pub struct ExportResult {
    pub node_token: String,
    pub title: String,
    pub local_path: Option<PathBuf>,
    pub error: Option<String>,
}

/// 导出缓存（断点续导）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportCache {
    pub space_id: String,
    pub format: ExportFormat,
    pub completed: Vec<String>, // 已完成的 obj_token
    pub failed: Vec<String>,    // 失败的 obj_token
    pub last_run: DateTime<Utc>,
}

impl ExportCache {
    pub fn new(space_id: String, format: ExportFormat) -> Self {
        Self {
            space_id,
            format,
            completed: Vec::new(),
            failed: Vec::new(),
            last_run: Utc::now(),
        }
    }

    pub fn is_completed(&self, obj_token: &str) -> bool {
        self.completed.contains(&obj_token.to_string())
    }

    pub fn is_failed(&self, obj_token: &str) -> bool {
        self.failed.contains(&obj_token.to_string())
    }

    pub fn mark_completed(&mut self, obj_token: String) {
        if !self.is_completed(&obj_token) {
            self.completed.push(obj_token.clone());
        }
        self.failed.retain(|t| t != &obj_token);
    }

    pub fn mark_failed(&mut self, obj_token: String) {
        if !self.is_failed(&obj_token) {
            self.failed.push(obj_token);
        }
    }
}
