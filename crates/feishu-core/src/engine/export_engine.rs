//! Batch export engine with concurrency, resume, and zip packaging

use crate::api::FeishuClient;
use crate::error::{FeishuCoreError as Error, Result};
use crate::models::export::{ExportCache, ExportFormat, ExportLog, ExportProgress};
use crate::models::wiki::Node;
use crate::storage::CacheStore;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;
use reqwest;

/// Progress callback: called after each node is processed (success or failure)
/// Parameters: node, result_path, status, current, total
pub type ProgressCallback = Arc<dyn Fn(Node, Option<PathBuf>, &str, usize, usize) + Send + Sync>;

/// Batch export engine
pub struct ExportEngine {
    client: FeishuClient,
    access_token: String,
    cache_store: CacheStore,
    concurrency: usize,
    progress_callback: Option<ProgressCallback>,
}

impl ExportEngine {
    pub fn new(client: FeishuClient, access_token: String) -> Self {
        Self {
            client,
            access_token,
            cache_store: CacheStore::new(),
            concurrency: 5,
            progress_callback: None,
        }
    }

    pub fn with_concurrency(mut self, n: usize) -> Self {
        self.concurrency = n.min(10);
        self
    }

    pub fn with_progress_callback(mut self, cb: ProgressCallback) -> Self {
        self.progress_callback = Some(cb);
        self
    }

    /// Export an entire knowledge space, returns the path to the resulting ZIP
    pub async fn export_space(
        &self,
        space_id: &str,
        format: ExportFormat,
        output_dir: &Path,
        resume: bool,
    ) -> Result<PathBuf> {
        // For md format, we first export in original format, then convert to md
        let is_md_export = format == ExportFormat::Md;
        let initial_format = if is_md_export {
            ExportFormat::Auto
        } else {
            format
        };

        // Build node tree
        let nodes = self.client.get_node_tree(&self.access_token, space_id).await?;

        let tree_mgr = crate::engine::NodeTreeManager::new(self.client.clone());
        let nodes = tree_mgr.filter_exportable(nodes);
        let path_map = tree_mgr.build_path_map(&nodes);

        if nodes.is_empty() {
            return Err(Error::ApiError { code: -1, msg: "No exportable documents found".to_string() });
        }

        // Load resume cache
        let mut cache = if resume {
            self.cache_store.load(space_id, initial_format.extension()).await
                .unwrap_or_else(|_| ExportCache::new(space_id.to_string(), initial_format))
        } else {
            ExportCache::new(space_id.to_string(), initial_format)
        };

        let remaining: Vec<Node> = nodes.clone()
            .into_iter()
            .filter(|n| !cache.is_completed(&n.obj_token))
            .collect();

        if remaining.is_empty() {
            return Ok(output_dir.join("all_done.zip"));
        }

        let total = remaining.len();

        // Setup output
        let temp_dir = output_dir.join(format!("temp_{}", space_id));
        fs::create_dir_all(&temp_dir)
            .map_err(Error::IoError)?;

        let export_log = ExportLog::new(output_dir, space_id)
            .map_err(|e| Error::StorageError(format!("create export log: {}", e)))?;

        let progress = Arc::new(Mutex::new(ExportProgress::new(total)));
        let sem = Arc::new(Semaphore::new(self.concurrency));

        let mut handles: Vec<(JoinHandle<Result<()>>, String)> = Vec::new();

        for node in remaining {
            // Clone all fields we'll need inside the async block (can't borrow after move)
            let node_obj_token = node.obj_token.clone();
            let node_title = node.title.clone();
            let node_type = node.obj_type.clone();
            let node_obj_token_saved = node.obj_token.clone();

            let token = self.access_token.clone();
            let client = self.client.clone();
            let sem_clone = Arc::clone(&sem);
            let progress_clone = Arc::clone(&progress);
            let temp_dir_clone = temp_dir.clone();
            let export_log_clone = export_log.clone();
            let mut cache_clone = cache.clone();
            let cb_clone = self.progress_callback.clone();
            let path = path_map
                .get(&node.obj_token)
                .cloned()
                .unwrap_or_else(|| format!("{}/{}", node.depth, node.safe_filename()));
            let export_format = initial_format;
            let target_format = format; // Pass the original format to determine output location
            let total_clone = total;

            let handle: JoinHandle<Result<()>> = tokio::spawn(async move {
                let _permit = sem_clone.acquire().await
                    .map_err(|e| Error::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;

                let result = Self::export_single_document(
                    &client,
                    &token,
                    &node,
                    export_format,
                    target_format,
                    &temp_dir_clone,
                    &path,
                )
                .await;

                let mut prog = progress_clone.lock()
                    .map_err(|e| Error::IoError(std::io::Error::new(std::io::ErrorKind::Other, format!("lock: {}", e))))?;

                match result {
                    Ok(local_path) => {
                        prog.increment_completed();
                        cache_clone.mark_completed(node_obj_token_saved.clone());
                        let _ = export_log_clone.append_success(&node_title, &node_obj_token_saved, &node_type, &local_path);
                        if let Some(ref cb) = cb_clone {
                            let current = prog.completed;
                            cb(node, Some(local_path), "success", current, total_clone);
                        }
                    }
                    Err(e) => {
                        prog.increment_failed();
                        cache_clone.mark_failed(node_obj_token_saved.clone());
                        let err_msg = e.to_string();
                        let _ = export_log_clone.append_failed(&node_title, &node_obj_token_saved, &node_type, &err_msg);
                        if let Some(ref cb) = cb_clone {
                            let current = prog.completed + prog.failed;
                            cb(node, None, &err_msg, current, total_clone);
                        }
                    }
                }
                Ok(())
            });

            handles.push((handle, node_obj_token));
        }

        // Wait for all tasks
        for (handle, obj_token) in handles {
            if let Err(_) = handle.await {
                cache.mark_failed(obj_token);
            }
        }

        // Save cache
        let _ = self.cache_store.save(&cache).await;

        // If md format, convert all documents and generate README
        let final_dir = if is_md_export {
            println!("\n📝 转换为 Markdown 格式...");
            let converted_dir = self.convert_to_md_and_generate_readme(&temp_dir, &nodes, space_id, &path_map, &cache)?;
            converted_dir
        } else {
            temp_dir.clone()
        };

        // Package into ZIP
        let zip_path = self.create_zip(&final_dir, output_dir, space_id)?;
        fs::remove_dir_all(&temp_dir)
            .map_err(Error::IoError)?;

        Ok(zip_path)
    }

    /// Convert all exported documents to Markdown and generate README.md
    fn convert_to_md_and_generate_readme(
        &self,
        temp_dir: &Path,
        nodes: &[Node],
        space_id: &str,
        path_map: &std::collections::HashMap<String, String>,
        cache: &ExportCache,
    ) -> Result<PathBuf> {
        use std::collections::HashMap;

        // Build a map of obj_token -> node for quick lookup
        let node_map: HashMap<&str, &Node> = nodes.iter()
            .map(|n| (n.obj_token.as_str(), n))
            .collect();

        // Origin directory contains original format files
        let origin_dir = temp_dir.join("origin");
        if !origin_dir.exists() {
            return Ok(temp_dir.to_path_buf());
        }

        // Collect all exported files and convert them
        let mut conversion_results: HashMap<String, (String, bool)> = HashMap::new();

        for entry in walkdir::WalkDir::new(&origin_dir)
            .min_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_dir() {
                continue;
            }

            let ext = path.extension().and_then(|s| s.to_str());
            if ext == Some("docx") {
                // Get relative path from origin directory
                let relative = path.strip_prefix(&origin_dir)
                    .map_err(Error::StripPrefixError)?
                    .to_string_lossy()
                    .replace('\\', "/");

                // Convert docx to md in root directory
                let md_path = temp_dir.join(format!("{}.md", relative.trim_end_matches(".docx")));
                if let Some(parent) = md_path.parent() {
                    fs::create_dir_all(parent)
                        .map_err(Error::IoError)?;
                }

                let filename = path.file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("?");
                println!("  转换: {} -> {}.md", filename, relative.trim_end_matches(".docx"));

                match crate::engine::MdConverter::docx_to_md(path, &md_path) {
                    Ok(()) => {
                        // Store conversion result (relative path, success)
                        let md_relative = format!("{}.md", relative.trim_end_matches(".docx"));
                        conversion_results.insert(relative.clone(), (md_relative, true));
                    }
                    Err(e) => {
                        println!("    转换失败: {}", e);
                        conversion_results.insert(relative.clone(), (relative, false));
                    }
                }
            } else if ext == Some("xlsx") || ext == Some("csv") {
                // Copy spreadsheets to root directory
                let relative = path.strip_prefix(&origin_dir)
                    .map_err(Error::StripPrefixError)?
                    .to_string_lossy()
                    .replace('\\', "/");

                let dest_path = temp_dir.join(&relative);
                if let Some(parent) = dest_path.parent() {
                    fs::create_dir_all(parent)
                        .map_err(Error::IoError)?;
                }

                fs::copy(path, &dest_path)
                    .map_err(Error::IoError)?;

                conversion_results.insert(relative.clone(), (relative.clone(), true));
            }
        }

        // Generate README.md
        let readme_path = temp_dir.join("README.md");
        let mut readme_content = String::new();

        readme_content.push_str(&format!("# 飞书知识库导出\n\n"));
        readme_content.push_str(&format!("**空间 ID:** `{}`\n\n", space_id));
        readme_content.push_str(&format!("**导出时间:** {}\n\n", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")));
        readme_content.push_str(&format!("**文档数量:** {}\n\n", nodes.len()));
        readme_content.push_str("---\n\n");
        readme_content.push_str("## 📂 目录说明\n\n");
        readme_content.push_str("- **根目录** - Markdown 格式文档（推荐阅读）\n");
        readme_content.push_str("- **`origin/`** - 原始格式文档（docx/xlsx 等）\n\n");
        readme_content.push_str("---\n\n");

        // Build tree structure
        readme_content.push_str("## 📁 文档结构\n\n");

        // Group nodes by depth and sort by original path
        let mut sorted_nodes: Vec<&Node> = nodes.iter().collect();
        sorted_nodes.sort_by_key(|n| {
            path_map.get(&n.obj_token)
                .cloned()
                .unwrap_or_else(|| format!("{}/{}", n.depth, n.safe_filename()))
        });

        for node in &sorted_nodes {
            let indent = "  ".repeat((node.depth.saturating_sub(1).min(10)) as usize);
            let relative_path = path_map.get(&node.obj_token)
                .cloned()
                .unwrap_or_else(|| format!("{}/{}", node.depth, node.safe_filename()));

            // Determine file type and paths
            let obj_type_lower = node.obj_type.to_lowercase();
            let (md_path, origin_path) = if obj_type_lower.contains("sheet") || obj_type_lower.contains("bitable") {
                // Spreadsheet files
                let xlsx_path = format!("{}.xlsx", relative_path);
                (None, xlsx_path)
            } else {
                // Document files
                let md = format!("{}.md", relative_path);
                let origin = format!("{}.docx", relative_path);
                (Some(md), origin)
            };

            let is_converted = md_path.as_ref()
                .and_then(|p| conversion_results.get(p))
                .is_some();
            let is_failed = cache.failed.contains(&node.obj_token);

            let status_icon = if is_failed {
                "❌"
            } else if is_converted {
                "✅"
            } else {
                "⚠️"
            };

            // Build file links
            let file_links = if let Some(md) = md_path {
                if is_converted {
                    format!("[`{}`]({}) [原格式]({})", node.title, md, format!("origin/{}", origin_path))
                } else {
                    format!("[`{}`]({})", node.title, format!("origin/{}", origin_path))
                }
            } else {
                format!("[`{}`]({})", node.title, format!("origin/{}", origin_path))
            };

            readme_content.push_str(&format!("{}{} {}\n", indent, status_icon, file_links));
        }

        readme_content.push_str("\n---\n\n");
        readme_content.push_str("## 📊 导出统计\n\n");
        readme_content.push_str(&format!("- **总计:** {} 个文档\n", nodes.len()));
        readme_content.push_str(&format!("- **成功:** {} 个\n", cache.completed.len()));
        readme_content.push_str(&format!("- **失败:** {} 个\n", cache.failed.len()));

        if !cache.failed.is_empty() {
            readme_content.push_str("\n### ❌ 失败列表\n\n");
            for obj_token in &cache.failed {
                if let Some(node) = node_map.get(obj_token.as_str()) {
                    readme_content.push_str(&format!("- `{}` ({})\n", node.title, node.obj_type));
                }
            }
        }

        readme_content.push_str("\n---\n\n");
        readme_content.push_str("*本文件由 [feishu-export](https://github.com/xxx/feishu-export) 自动生成*\n");

        fs::write(&readme_path, readme_content)
            .map_err(Error::IoError)?;

        println!("\n  ✅ README.md 生成完成");

        Ok(temp_dir.to_path_buf())
    }

    /// Export a single document
    pub async fn export_single_document(
        client: &FeishuClient,
        token: &str,
        node: &Node,
        format: ExportFormat,
        target_format: ExportFormat,
        temp_dir: &Path,
        relative_path: &str,
    ) -> Result<PathBuf> {
        // Step 1: Determine the expected file extension (before any network requests)
        let expected_ext: String = if node.obj_type == "file" {
            // Extract file extension from title
            if let Some(ext) = std::path::Path::new(&node.title).extension() {
                ext.to_string_lossy().to_lowercase()
            } else {
                // Default to bin if no extension in title
                "bin".to_string()
            }
        } else {
            // For docx/doc/sheet/bitable: resolve format first
            Self::resolve_format(node, format).extension().to_string()
        };

        // Step 2: Build the file path and check if it already exists
        let file_path = if target_format == ExportFormat::Md {
            temp_dir.join("origin").join(format!("{}.{}", relative_path, expected_ext))
        } else {
            temp_dir.join(format!("{}.{}", relative_path, expected_ext))
        };

        // Early return if file already exists (no network request needed!)
        if file_path.exists() {
            return Ok(file_path);
        }

        // Step 3: File doesn't exist, proceed with download
        let (response, file_ext_from_title): (reqwest::Response, Option<String>) = if node.obj_type == "file" {
            // For file type: use the file download API directly
            println!("    📤 使用文件下载 API...");
            let resp = client
                .download_file(token, &node.obj_token)
                .await?;
            // Extract actual extension from response header for file types too
            (resp, None) // Don't use expected_ext, let response header determine it
        } else {
            // For docx/doc/sheet/bitable: use export API
            let resolved_format = Self::resolve_format(node, format);

            // Try with the resolved format first
            let resp = match client
                .export_document(token, &node.obj_token, &node.obj_type, resolved_format)
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    // If we get FileTokenInvalid, try with PDF as fallback
                    if e.is_file_token_invalid() {
                        println!("    ⚠️ 文件 token 无效，尝试降级到 PDF 格式...");
                        match client
                            .export_document(token, &node.obj_token, &node.obj_type, ExportFormat::Pdf)
                            .await
                        {
                            Ok(resp) => resp,
                            Err(_e2) => {
                                // If PDF also fails, return the original error
                                return Err(e);
                            }
                        }
                    } else {
                        // Return other errors as-is
                        return Err(e);
                    }
                }
            };

            (resp, None)
        };

        // Extract actual file extension from Content-Disposition header
        // (API may fall back to a different format, so header reflects the truth)
        let final_ext: String = if let Some(ext) = file_ext_from_title {
            // Use extension from file title for file types
            ext
        } else {
            Self::extract_extension_from_response(&response)
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    // Fallback to the expected extension we determined earlier
                    expected_ext.clone()
                })
        };

        // Rebuild file path with actual extension (may differ from expected)
        let actual_file_path = if target_format == ExportFormat::Md {
            temp_dir.join("origin").join(format!("{}.{}", relative_path, final_ext))
        } else {
            temp_dir.join(format!("{}.{}", relative_path, final_ext))
        };

        // Check again with actual extension (in case it differs)
        if actual_file_path != file_path && actual_file_path.exists() {
            return Ok(actual_file_path);
        }

        if let Some(parent) = actual_file_path.parent() {
            fs::create_dir_all(parent)
                .map_err(Error::IoError)?;
        }

        let bytes = response.bytes().await
            .map_err(Error::NetworkError)?;
        fs::write(&actual_file_path, &bytes)
            .map_err(Error::IoError)?;

        Ok(actual_file_path)
    }

    /// Extract file extension from HTTP Content-Disposition header
    fn extract_extension_from_response(response: &reqwest::Response) -> Option<&'static str> {
        let headers = response.headers();
        if let Some(content_disposition) = headers.get("Content-Disposition") {
            if let Ok(cd_str) = content_disposition.to_str() {
                // Content-Disposition: attachment; filename="xxx.docx" or filename*=UTF-8''xxx.docx
                if let Some(filename) = Self::extract_filename_from_cd(cd_str) {
                    if let Some(ext) = Path::new(filename.as_str()).extension() {
                        let ext_str = ext.to_string_lossy().to_lowercase();
                        // Map common extensions to our ExportFormat extensions
                        return Some(match ext_str.as_str() {
                            "docx" => "docx",
                            "pdf" => "pdf",
                            "md" => "md",
                            "xlsx" => "xlsx",
                            "csv" => "csv",
                            _ => return None,
                        });
                    }
                }
            }
        }
        None
    }

    /// Parse filename from Content-Disposition header (handles both plain and RFC 5987 encoded)
    fn extract_filename_from_cd(cd: &str) -> Option<String> {
        // Try filename*=UTF-8''... (RFC 5987 encoding)
        if let Some(pos) = cd.find("filename*=UTF-8''") {
            let after_prefix = &cd[pos + 17..];
            // Decode percent-encoded characters
            let decoded = Self::decode_percent(after_prefix);
            return Some(decoded);
        }
        // Try simple filename="..."
        if let Some(pos) = cd.find("filename=\"") {
            let start = pos + 10;
            if let Some(end) = cd[start..].find('"') {
                return Some(cd[start..start + end].to_string());
            }
        }
        None
    }

    /// Decode RFC 5987 percent-encoded strings
    fn decode_percent(s: &str) -> String {
        let mut result = String::new();
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '%' {
                let hex: String = chars.by_ref().take(2).collect();
                if hex.len() == 2 {
                    if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                        result.push(byte as char);
                    } else {
                        result.push('%');
                        result.push_str(&hex);
                    }
                } else {
                    result.push('%');
                    result.push_str(&hex);
                    break;
                }
            } else {
                result.push(c);
            }
        }
        result
    }

    /// Resolve the format for API call: Auto → node type default, Docx → Xlsx for sheets
    pub fn resolve_format(node: &Node, format: ExportFormat) -> ExportFormat {
        if format == ExportFormat::Auto {
            ExportFormat::for_node_type(&node.obj_type)
        } else {
            // For docx nodes requesting docx, Feishu API may fall back internally
            // For sheet/bitable requesting docx, force to xlsx
            if format == ExportFormat::Docx
                && (node.obj_type == "sheet" || node.obj_type == "bitable")
            {
                ExportFormat::Xlsx
            } else if format == ExportFormat::Xlsx
                // If user specifically requested xlsx but the node is not a spreadsheet,
                // fallback to the appropriate format for the node type
                && !matches!(node.obj_type.as_str(), "sheet" | "bitable")
            {
                // For non-spreadsheet documents, use the default format for the node type
                ExportFormat::for_node_type(&node.obj_type)
            } else {
                format
            }
        }
    }

    /// Package temp directory into a ZIP file
    fn create_zip(&self, temp_dir: &Path, output_dir: &Path, space_id: &str) -> Result<PathBuf> {
        let zip_name = format!(
            "{}_{}.zip",
            space_id,
            chrono::Utc::now().format("%Y%m%d_%H%M%S")
        );
        let zip_path = output_dir.join(zip_name);

        let file = fs::File::create(&zip_path)
            .map_err(Error::IoError)?;
        let mut zip = ZipWriter::new(file);
        let opts: SimpleFileOptions = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        for entry in walkdir::WalkDir::new(temp_dir)
            .min_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_dir() {
                continue;
            }
            let relative = path.strip_prefix(temp_dir)
                .map_err(Error::StripPrefixError)?;
            let name = relative.to_string_lossy().replace("\\", "/");

            zip.start_file(&name, opts)
                .map_err(|e| Error::ZipError(e.into()))?;
            let contents = fs::read(path)
                .map_err(Error::IoError)?;
            zip.write_all(&contents)
                .map_err(|e| Error::ZipError(e.into()))?;
        }

        zip.finish()
            .map_err(|e| Error::ZipError(e.into()))?;

        Ok(zip_path)
    }
}
