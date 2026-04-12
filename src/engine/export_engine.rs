use crate::api::FeishuClient;
use crate::engine::{MdConverter, NodeTreeManager};
use crate::error::{FeishuError, Result};
use crate::models::export::{ExportCache, ExportFormat, ExportProgress};
use crate::models::wiki::Node;
use crate::storage::CacheStore;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use zip::write::FileOptions;
use zip::ZipWriter;

/// 导出引擎
pub struct ExportEngine {
    client: FeishuClient,
    access_token: String,
    cache_store: CacheStore,
    concurrency: usize,
}

impl ExportEngine {
    pub fn new(client: FeishuClient, access_token: String) -> Self {
        Self {
            client,
            access_token,
            cache_store: CacheStore::new().expect("Failed to create cache store"),
            concurrency: 5,
        }
    }

    pub fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = concurrency.min(10); // 最多 10 并发
        self
    }

    /// 获取有效的 access_token
    async fn get_valid_token(&self) -> Result<String> {
        Ok(self.access_token.clone())
    }

    /// 导出知识空间
    pub async fn export_space(
        &self,
        space_id: &str,
        format: ExportFormat,
        output_dir: &Path,
        resume: bool,
    ) -> Result<PathBuf> {
        let token = self.get_valid_token().await?;

        // 获取节点树
        println!("{}", "🔵 正在获取文档树...".blue());
        let tree_manager = NodeTreeManager::new(self.client.clone());
        let mut nodes = self.client.get_node_tree(&token, space_id).await?;
        nodes = tree_manager.filter_exportable(nodes);

        if nodes.is_empty() {
            return Err(FeishuError::ApiError {
                code: -1,
                msg: "No exportable documents found".to_string(),
            });
        }

        println!(
            "{}",
            format!("✅ 找到 {} 个可导出文档", nodes.len()).green()
        );

        // 加载缓存
        let mut cache = if resume {
            self.cache_store.load(space_id, format.extension())?
        } else {
            ExportCache::new(space_id.to_string(), format)
        };

        // 过滤已完成的节点
        let nodes: Vec<Node> = nodes
            .into_iter()
            .filter(|n| !cache.is_completed(&n.obj_token))
            .collect();

        if nodes.is_empty() {
            println!("{}", "✅ 所有文档已导出完成".green());
            return Ok(output_dir.join("empty.zip"));
        }

        // 构建路径映射
        let path_map = tree_manager.build_path_map(&nodes);

        // 创建临时目录
        let temp_dir = output_dir.join(format!("temp_{}", space_id));
        fs::create_dir_all(&temp_dir)?;

        // 导出进度
        let progress = Arc::new(std::sync::Mutex::new(ExportProgress::new(nodes.len())));
        let pb = ProgressBar::new(nodes.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
                .unwrap(),
        );

        // 并发导出
        let sem = Arc::new(Semaphore::new(self.concurrency));
        let mut handles = Vec::new();

        for node in nodes {
            let token = token.clone();
            let client = self.client.clone();
            let sem_clone = Arc::clone(&sem);
            let pb_clone = pb.clone();
            let progress_clone = Arc::clone(&progress);
            let temp_dir_clone = temp_dir.clone();
            let obj_token_clone = node.obj_token.clone();
            let path = path_map
                .get(&node.obj_token)
                .cloned()
                .unwrap_or_else(|| format!("{}/{}", node.depth, node.safe_filename()));
            let mut cache_clone = cache.clone();
            let format = format;

            let handle: JoinHandle<Result<(Node, Option<PathBuf>)>> = tokio::spawn(async move {
                let _permit = sem_clone.acquire().await.unwrap();

                let result = Self::export_single_document(
                    &client,
                    &token,
                    &node,
                    format,
                    &temp_dir_clone,
                    &path,
                )
                .await;

                let mut prog = progress_clone.lock().unwrap();
                match result {
                    Ok(local_path) => {
                        pb_clone.inc(1);
                        pb_clone.set_message(format!("✅ {}", node.title));
                        prog.increment_completed();
                        cache_clone.mark_completed(node.obj_token.clone());
                        Ok((node, Some(local_path)))
                    }
                    Err(e) => {
                        pb_clone.inc(1);
                        let err_msg = e.to_string();
                        // 只对前 3 个失败打印详细错误
                        let is_first_failure = {
                            let count = prog.failed + prog.completed;
                            count < 3
                        };
                        if is_first_failure {
                            eprintln!("[ERR] ❌ {}: {}", node.title, err_msg);
                            eprintln!("[ERR]    obj_token={} obj_type={}", node.obj_token, node.obj_type);
                        }
                        if e.is_retryable() {
                            pb_clone.set_message(format!("⚠️ {} (重试)", node.title));
                        } else {
                            pb_clone.set_message(format!("❌ {}", node.title));
                        }
                        prog.increment_failed();
                        cache_clone.mark_failed(node.obj_token.clone());
                        Ok((node, None))
                    }
                }
            });

            handles.push((handle, obj_token_clone));
        }

        // 等待所有任务完成
        for (handle, obj_token) in handles {
            match handle.await {
                Ok(Ok((_node, _path))) => {
                    // 结果已处理
                }
                Ok(Err(_)) => {
                    cache.mark_failed(obj_token);
                }
                Err(_) => {
                    cache.mark_failed(obj_token);
                }
            }
        }

        pb.finish();

        // 保存缓存
        self.cache_store.save(&cache)?;

        // 打印统计
        let prog = progress.lock().unwrap();
        println!(
            "\n{}",
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
        );
        println!(
            "{}",
            format!(
                "✅ 导出完成: {} 成功 / {} 跳过 / {} 失败",
                prog.completed, prog.skipped, prog.failed
            )
            .green()
        );

        // 打包成 ZIP
        println!("{}", "🔵 正在打包...".blue());
        let zip_path = self.create_zip(&temp_dir, output_dir, space_id, format)?;
        println!("{}", format!("📦 打包文件: {}", zip_path.display()).green());

        // 清理临时文件
        fs::remove_dir_all(temp_dir)?;

        Ok(zip_path)
    }

    /// 导出单个文档
    async fn export_single_document(
        client: &FeishuClient,
        token: &str,
        node: &Node,
        format: ExportFormat,
        temp_dir: &Path,
        relative_path: &str,
    ) -> Result<PathBuf> {
        // 确定文件扩展名
        let ext = if format == ExportFormat::Md {
            "docx" // 先导出 docx，然后转换
        } else {
            format.extension()
        };

        // 导出文档
        let response = client
            .export_document(token, &node.obj_token, &node.obj_type, format)
            .await?;

        // 保存文件
        let file_path = temp_dir.join(format!("{}.{}", relative_path, ext));
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let bytes = response.bytes().await?;
        fs::write(&file_path, bytes)?;

        // 如果需要 MD 格式，进行转换
        if format == ExportFormat::Md {
            let md_path = file_path.with_extension("md");
            MdConverter::docx_to_md(&file_path, &md_path)?;
            fs::remove_file(file_path)?; // 删除临时 docx
            return Ok(md_path);
        }

        Ok(file_path)
    }

    /// 创建 ZIP 包
    fn create_zip(
        &self,
        temp_dir: &Path,
        output_dir: &Path,
        space_id: &str,
        format: ExportFormat,
    ) -> Result<PathBuf> {
        let zip_filename = format!(
            "{}_{}.zip",
            space_id,
            chrono::Utc::now().format("%Y%m%d_%H%M%S")
        );
        let zip_path = output_dir.join(zip_filename);

        let file = File::create(&zip_path)?;
        let mut zip = ZipWriter::new(file);

        for entry in walkdir::WalkDir::new(temp_dir)
            .min_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_dir() {
                continue;
            }

            let relative = path.strip_prefix(temp_dir)?;
            let name = relative.to_string_lossy().replace("\\", "/");

            zip.start_file::<_, ()>(
                &name,
                FileOptions::default().compression_method(zip::CompressionMethod::Deflated),
            )?;
            let contents = fs::read(path)?;
            zip.write_all(&contents)?;
        }

        zip.finish()?;

        Ok(zip_path)
    }
}
