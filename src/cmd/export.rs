use colored::Colorize;
use feishu_core::error::FeishuCoreError as Error;
use feishu_core::models::wiki::Node;
use feishu_core::storage::{ConfigStore, TokenStore};
use feishu_core::{ExportEngine, ExportFormat, FeishuClient};

type CmdResult = std::result::Result<(), Error>;

/// 导出子命令
pub struct ExportCommand {
    client: FeishuClient,
    config_store: ConfigStore,
    token_store: TokenStore,
}

impl ExportCommand {
    pub fn new() -> std::result::Result<Self, Error> {
        Ok(Self {
            client: FeishuClient::new(),
            config_store: ConfigStore::new(),
            token_store: TokenStore::new(),
        })
    }

    /// 导出知识空间
    pub async fn run(
        &self,
        space_id: &str,
        format: &str,
        output: Option<std::path::PathBuf>,
        _node: Option<String>,
        concurrency: Option<usize>,
        resume: bool,
    ) -> CmdResult {
        let token = self.get_valid_token().await?;

        let export_format = ExportFormat::from_str(format).ok_or_else(|| {
            Error::ConfigError(format!(
                "Unsupported export format: {}. Supported: auto, docx, pdf, md, xlsx, csv",
                format
            ))
        })?;

        let output_dir = output.unwrap_or_else(|| {
            dirs::download_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("feishu-exports")
        });

        std::fs::create_dir_all(&output_dir)
            .map_err(Error::IoError)?;

        println!("{}", "🔵 飞书文档批量导出工具 v0.1.0".cyan());
        println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
        println!("{}", format!("空间 ID:    {}", space_id).dimmed());
        println!("{}", format!("导出格式:    {:?}", export_format).dimmed());
        println!("{}", format!("输出目录:    {}", output_dir.display()).dimmed());
        if resume {
            println!("{}", "断点续导:   是".dimmed());
        }
        println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());

        let mut engine = ExportEngine::new(self.client.clone(), token);
        if let Some(c) = concurrency {
            engine = engine.with_concurrency(c);
        }

        let start_time = std::time::Instant::now();
        let _zip_path = engine
            .export_space(space_id, export_format, &output_dir, resume)
            .await
            .map_err(|e| Error::ConversionError(e.to_string()))?;

        let duration = start_time.elapsed();
        let secs = duration.as_secs();
        let mins = secs / 60;
        println!("\n{}", format!("📊 耗时: {} 分 {} 秒", mins, secs % 60).cyan());

        Ok(())
    }

    /// 导出单篇文档（调试用）
    pub async fn run_one(
        &self,
        obj_token: &str,
        obj_type: &str,
        format: &str,
        output: Option<std::path::PathBuf>,
    ) -> CmdResult {
        println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
        println!("{}", "🔵 单篇导出调试模式".cyan());
        println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
        println!("obj_token: {}", obj_token);
        println!("obj_type:  {}", obj_type);
        println!("format:    {}", format);
        println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());

        let token = self.get_valid_token().await?;

        let export_format = ExportFormat::from_str(format).ok_or_else(|| {
            Error::ConfigError(format!("Unsupported format: {}", format))
        })?;

        let output_dir = output.unwrap_or_else(|| {
            dirs::download_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("feishu-exports")
        });
        std::fs::create_dir_all(&output_dir)
            .map_err(Error::IoError)?;

        let temp_dir = output_dir.join(format!("temp_debug_{}", obj_token));
        std::fs::create_dir_all(&temp_dir)
            .map_err(Error::IoError)?;

        let node = Node {
            space_id: String::new(),
            node_token: obj_token.to_string(),
            obj_token: obj_token.to_string(),
            parent_node_token: None,
            obj_type: obj_type.to_string(),
            node_type: "origin".to_string(),
            title: format!("doc_{}", &obj_token[..8.min(obj_token.len())]),
            has_child: false,
            depth: 1,
        };

        let path = format!("1_{}", node.title);
        let result = ExportEngine::export_single_document(
            &self.client,
            &token,
            &node,
            export_format,
            &temp_dir,
            &path,
        )
        .await;

        match result {
            Ok(file_path) => {
                let final_ext = file_path.extension().and_then(|s| s.to_str()).unwrap_or("bin");
                let final_path = output_dir.join(format!("{}.{}", node.title, final_ext));
                std::fs::copy(&file_path, &final_path)
                    .map_err(Error::IoError)?;
                let _ = std::fs::remove_dir_all(&temp_dir);
                println!();
                println!("{}", "✅ 导出成功！".green());
                println!("  保存位置: {}", final_path.display());
            }
            Err(e) => {
                println!();
                println!("{}", "❌ 导出失败".red());
                println!("  错误信息: {}", e);
            }
        }

        Ok(())
    }

    async fn get_valid_token(&self) -> std::result::Result<String, Error> {
        let mut token_data = self.token_store.load().await
            .map_err(|e| Error::StorageError(e.to_string()))?;

        if token_data.is_expired() {
            println!("{}", "🔵 访问令牌已过期，正在刷新...".yellow());
            token_data = self.client.refresh_user_token(&self.config_store, &token_data.refresh_token)
                .await
                .map_err(|e| Error::AuthFailed(e.to_string()))?;
            self.token_store.save(&token_data).await
                .map_err(|e| Error::StorageError(e.to_string()))?;
            println!("{}", "✅ 令牌刷新成功".green());
        }

        Ok(token_data.access_token)
    }
}
