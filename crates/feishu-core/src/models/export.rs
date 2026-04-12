use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::Write as _;
use std::path::{Path, PathBuf};

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
    pub fn extension(&self) -> &'static str {
        match self {
            ExportFormat::Docx => "docx",
            ExportFormat::Pdf => "pdf",
            ExportFormat::Md => "md",
            ExportFormat::Xlsx => "xlsx",
            ExportFormat::Csv => "csv",
            ExportFormat::Auto => "auto",
        }
    }

    /// 飞书 API 的 file_extension 参数（md 先导出 docx）
    pub fn api_extension(&self) -> &'static str {
        match self {
            ExportFormat::Md => "docx",
            _ => self.extension(),
        }
    }

    /// 按节点类型自动选择最佳导出格式
    pub fn for_node_type(obj_type: &str) -> Self {
        match obj_type {
            "sheet" | "bitable" => ExportFormat::Xlsx,
            "docx" | "doc" | "mindnote" | "slides" => ExportFormat::Md,
            _ => ExportFormat::Docx,
        }
    }

    /// 导 CSV 时需要 sub_id（sheet_id / table_id）
    pub fn needs_sub_id(&self) -> bool {
        matches!(self, ExportFormat::Csv)
    }

    pub fn needs_conversion(&self) -> bool {
        matches!(self, ExportFormat::Md)
    }

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
#[serde(tag = "status", rename_all = "lowercase")]
pub enum ExportStatus {
    Pending,
    Creating,
    Polling,
    Downloading,
    Converting,
    Completed,
    Failed { reason: String },
}

impl ExportStatus {
    pub fn is_final(&self) -> bool {
        matches!(self, ExportStatus::Completed | ExportStatus::Failed { .. })
    }
}

/// 导出进度（内存中进度统计）
#[derive(Debug, Clone, Default)]
pub struct ExportProgress {
    pub total: usize,
    pub completed: usize,
    pub skipped: usize,
    pub failed: usize,
}

impl ExportProgress {
    pub fn new(total: usize) -> Self {
        Self { total, ..Default::default() }
    }

    pub fn increment_completed(&mut self) { self.completed += 1; }
    pub fn increment_skipped(&mut self) { self.skipped += 1; }
    pub fn increment_failed(&mut self) { self.failed += 1; }
    pub fn is_complete(&self) -> bool {
        self.completed + self.skipped + self.failed == self.total
    }
    pub fn success_rate(&self) -> f64 {
        if self.total == 0 { 0.0 } else { self.completed as f64 / self.total as f64 }
    }
}

/// 单条导出记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportLogEntry {
    pub timestamp: String,
    pub title: String,
    pub obj_token: String,
    pub obj_type: String,
    pub status: String,
    pub local_path: Option<String>,
    pub error: Option<String>,
}

impl ExportLogEntry {
    pub fn success(title: String, obj_token: String, obj_type: String, local_path: String) -> Self {
        Self { timestamp: Utc::now().to_rfc3339(), title, obj_token, obj_type, status: "success".to_string(), local_path: Some(local_path), error: None }
    }
    pub fn failed(title: String, obj_token: String, obj_type: String, error: String) -> Self {
        Self { timestamp: Utc::now().to_rfc3339(), title, obj_token, obj_type, status: "failed".to_string(), local_path: None, error: Some(error) }
    }
    pub fn write_jsonl<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<()> {
        let line = serde_json::to_string(self).unwrap_or_else(|_| String::from("{}"));
        writeln!(w, "{}", line)
    }
}

/// 导出日志（流式追加写入）
#[derive(Clone)]
pub struct ExportLog { path: PathBuf }

impl ExportLog {
    pub fn new(output_dir: &Path, space_id: &str) -> std::io::Result<Self> {
        let log_path = output_dir.join("exports.log");
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&log_path)?;
        writeln!(file, "# feishu-export log — space_id={} started={}", space_id, Utc::now().to_rfc3339())?;
        drop(file);
        Ok(Self { path: log_path })
    }
    pub fn append_success(&self, title: &str, obj_token: &str, obj_type: &str, local_path: &Path) -> std::io::Result<()> {
        let entry = ExportLogEntry::success(title.to_string(), obj_token.to_string(), obj_type.to_string(), local_path.display().to_string());
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&self.path)?;
        entry.write_jsonl(&mut file)
    }
    pub fn append_failed(&self, title: &str, obj_token: &str, obj_type: &str, error: &str) -> std::io::Result<()> {
        let entry = ExportLogEntry::failed(title.to_string(), obj_token.to_string(), obj_type.to_string(), error.to_string());
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&self.path)?;
        entry.write_jsonl(&mut file)
    }
    pub fn path(&self) -> &Path { &self.path }
}

// ─────────────────────────────────────────────────────────────────────────────
// API request/response models
// ─────────────────────────────────────────────────────────────────────────────

/// 创建导出任务响应
#[derive(Debug, Deserialize)]
pub struct CreateExportTaskResponse {
    pub code: i32,
    pub msg: String,
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
    pub msg: String,
    pub data: ExportTaskStatusData,
}

#[derive(Debug, Deserialize)]
pub struct ExportTaskStatusData {
    pub result: ExportTaskResult,
}

#[derive(Debug, Deserialize)]
pub struct ExportTaskResult {
    #[serde(default)]
    pub extra: ExportTaskExtra,
    #[serde(default)]
    pub file_token: String,
    #[serde(default)]
    pub file_extension: String,
    #[serde(default)]
    pub file_name: String,
    #[serde(default)]
    pub file_size: i64,
}

#[derive(Debug, Default, Deserialize)]
pub struct ExportTaskExtra {
    #[serde(default)]
    pub is_complete: String,
}

/// Sheet 工作表列表响应
#[derive(Debug, Deserialize)]
pub struct SheetListSheetsResponse {
    pub code: i32,
    pub msg: String,
    pub data: SheetListSheetsData,
}

#[derive(Debug, Deserialize)]
pub struct SheetListSheetsData {
    pub sheets: Vec<SheetSheetInfo>,
}

#[derive(Debug, Deserialize)]
pub struct SheetSheetInfo {
    pub sheet_id: String,
    pub title: String,
    #[serde(default)]
    pub hidden: bool,
}

/// Bitable 多维表格数据表列表响应
#[derive(Debug, Deserialize)]
pub struct BitableListTablesResponse {
    pub code: i32,
    pub msg: String,
    pub data: BitableListTablesData,
}

#[derive(Debug, Deserialize)]
pub struct BitableListTablesData {
    pub items: Vec<BitableTableInfo>,
}

#[derive(Debug, Deserialize)]
pub struct BitableTableInfo {
    pub table_id: String,
    pub name: String,
}

/// 导出缓存（断点续导）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportCache {
    pub space_id: String,
    pub format: ExportFormat,
    pub completed: Vec<String>,
    pub failed: Vec<String>,
    pub last_run: DateTime<Utc>,
}

impl ExportCache {
    pub fn new(space_id: String, format: ExportFormat) -> Self {
        Self { space_id, format, completed: Vec::new(), failed: Vec::new(), last_run: Utc::now() }
    }
    pub fn is_completed(&self, obj_token: &str) -> bool {
        self.completed.contains(&obj_token.to_string())
    }
    pub fn mark_completed(&mut self, obj_token: String) {
        if !self.is_completed(&obj_token) {
            self.completed.push(obj_token.clone());
        }
        self.failed.retain(|t| t != &obj_token);
    }
    pub fn mark_failed(&mut self, obj_token: String) {
        if !self.failed.contains(&obj_token) {
            self.failed.push(obj_token);
        }
    }
}
