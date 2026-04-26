//! Drive API models

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// File
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveFile {
    pub token: String,
    pub name: String,
    #[serde(rename = "type")]
    pub file_type: String,
    #[serde(default)]
    pub parent_token: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub created_time: Option<String>,
    #[serde(default)]
    pub modified_time: Option<String>,
    #[serde(default)]
    pub owner_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListFilesResponse {
    pub code: i32,
    pub msg: String,
    pub data: ListFilesData,
}

#[derive(Debug, Deserialize)]
pub struct ListFilesData {
    pub files: Vec<DriveFile>,
    #[serde(default)]
    pub page_token: Option<String>,
    #[serde(default)]
    pub has_more: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateFolderResponse {
    pub code: i32,
    pub msg: String,
    pub data: CreateFolderData,
}

#[derive(Debug, Deserialize)]
pub struct CreateFolderData {
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MoveFileResponse {
    pub code: i32,
    pub msg: String,
    pub data: MoveFileData,
}

#[derive(Debug, Deserialize)]
pub struct MoveFileData {
    #[serde(default)]
    pub task_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CopyFileResponse {
    pub code: i32,
    pub msg: String,
    pub data: CopyFileData,
}

#[derive(Debug, Deserialize)]
pub struct CopyFileData {
    #[serde(default)]
    pub file: Option<DriveFile>,
}

#[derive(Debug, Deserialize)]
pub struct DeleteFileResponse {
    pub code: i32,
    pub msg: String,
    pub data: DeleteFileData,
}

#[derive(Debug, Deserialize)]
pub struct DeleteFileData {
    #[serde(default)]
    pub task_id: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Shortcut
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateShortcutResponse {
    pub code: i32,
    pub msg: String,
    pub data: CreateShortcutData,
}

#[derive(Debug, Deserialize)]
pub struct CreateShortcutData {
    #[serde(default)]
    pub succ_shortcut_node: Option<ShortcutNode>,
}

#[derive(Debug, Deserialize)]
pub struct ShortcutNode {
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub parent_token: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Media
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct BatchGetTmpDownloadUrlResponse {
    pub code: i32,
    pub msg: String,
    pub data: BatchGetTmpDownloadUrlData,
}

#[derive(Debug, Deserialize)]
pub struct BatchGetTmpDownloadUrlData {
    #[serde(default)]
    pub tmp_download_urls: Vec<TmpDownloadUrl>,
}

#[derive(Debug, Deserialize)]
pub struct TmpDownloadUrl {
    #[serde(default)]
    pub file_token: Option<String>,
    #[serde(default)]
    pub tmp_download_url: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Meta / Statistics
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct BatchGetMetaResponse {
    pub code: i32,
    pub msg: String,
    pub data: BatchGetMetaData,
}

#[derive(Debug, Deserialize)]
pub struct BatchGetMetaData {
    #[serde(default)]
    pub metas: Vec<FileMeta>,
}

#[derive(Debug, Deserialize)]
pub struct FileMeta {
    pub doc_token: String,
    pub doc_type: String,
    pub title: String,
    #[serde(default)]
    pub owner_id: Option<String>,
    #[serde(default)]
    pub create_time: Option<String>,
    #[serde(default)]
    pub latest_modify_user: Option<String>,
    #[serde(default)]
    pub latest_modify_time: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FileStatisticsResponse {
    pub code: i32,
    pub msg: String,
    pub data: FileStatisticsData,
}

#[derive(Debug, Deserialize)]
pub struct FileStatisticsData {
    #[serde(default)]
    pub file_token: Option<String>,
    #[serde(default)]
    pub file_type: Option<String>,
    #[serde(default)]
    pub statistics: Option<FileStats>,
}

#[derive(Debug, Deserialize)]
pub struct FileStats {
    #[serde(default)]
    pub uv: Option<i32>,
    #[serde(default)]
    pub pv: Option<i32>,
    #[serde(default)]
    pub like_count: Option<i32>,
    #[serde(default)]
    pub uv_today: Option<i32>,
    #[serde(default)]
    pub pv_today: Option<i32>,
    #[serde(default)]
    pub like_count_today: Option<i32>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Import
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateImportTaskResponse {
    pub code: i32,
    pub msg: String,
    pub data: CreateImportTaskData,
}

#[derive(Debug, Deserialize)]
pub struct CreateImportTaskData {
    #[serde(default)]
    pub ticket: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct QueryImportTaskResponse {
    pub code: i32,
    pub msg: String,
    pub data: QueryImportTaskData,
}

#[derive(Debug, Deserialize)]
pub struct QueryImportTaskData {
    pub result: ImportTaskResult,
}

#[derive(Debug, Deserialize)]
pub struct ImportTaskResult {
    #[serde(default)]
    pub job_status: Option<i32>, // 0=成功, 1=初始化, 2=处理中
    #[serde(default)]
    pub file_token: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub job_error_msg: Option<String>,
}
