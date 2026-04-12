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

/// Progress callback: called after each node is processed (success or failure)
pub type ProgressCallback = Arc<dyn Fn(Node, Option<PathBuf>, &str) + Send + Sync>;

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
            self.cache_store.load(space_id, format.extension()).await
                .unwrap_or_else(|_| ExportCache::new(space_id.to_string(), format))
        } else {
            ExportCache::new(space_id.to_string(), format)
        };

        let remaining: Vec<Node> = nodes
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
            let format = format;

            let handle: JoinHandle<Result<()>> = tokio::spawn(async move {
                let _permit = sem_clone.acquire().await
                    .map_err(|e| Error::IoError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;

                let result = Self::export_single_document(
                    &client,
                    &token,
                    &node,
                    format,
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
                            cb(node, Some(local_path), "success");
                        }
                    }
                    Err(e) => {
                        prog.increment_failed();
                        cache_clone.mark_failed(node_obj_token_saved.clone());
                        let err_msg = e.to_string();
                        let _ = export_log_clone.append_failed(&node_title, &node_obj_token_saved, &node_type, &err_msg);
                        if let Some(ref cb) = cb_clone {
                            cb(node, None, &err_msg);
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

        // Package into ZIP
        let zip_path = self.create_zip(&temp_dir, output_dir, space_id)?;
        fs::remove_dir_all(&temp_dir)
            .map_err(Error::IoError)?;

        Ok(zip_path)
    }

    /// Export a single document
    pub async fn export_single_document(
        client: &FeishuClient,
        token: &str,
        node: &Node,
        format: ExportFormat,
        temp_dir: &Path,
        relative_path: &str,
    ) -> Result<PathBuf> {
        let actual_format = Self::resolve_format(node, format);

        let response = client
            .export_document(token, &node.obj_token, &node.obj_type, actual_format)
            .await?;

        let final_ext = if actual_format.needs_conversion() { "md" } else { actual_format.extension() };
        let file_path = temp_dir.join(format!("{}.{}", relative_path, final_ext));

        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)
                .map_err(Error::IoError)?;
        }

        let bytes = response.bytes().await
            .map_err(Error::NetworkError)?;
        fs::write(&file_path, &bytes)
            .map_err(Error::IoError)?;

        if actual_format.needs_conversion() {
            let md_path = file_path.with_extension("md");
            crate::engine::MdConverter::docx_to_md(&file_path, &md_path)
                .map_err(|e| Error::ConversionError(e.to_string()))?;
            fs::remove_file(&file_path)
                .map_err(Error::IoError)?;
            return Ok(md_path);
        }

        Ok(file_path)
    }

    fn resolve_format(node: &Node, format: ExportFormat) -> ExportFormat {
        if format != ExportFormat::Auto {
            if format == ExportFormat::Docx
                && (node.obj_type == "sheet" || node.obj_type == "bitable")
            {
                return ExportFormat::Xlsx;
            }
            return format;
        }
        ExportFormat::for_node_type(&node.obj_type)
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
