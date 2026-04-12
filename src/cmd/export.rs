use crate::api::FeishuClient;
use crate::engine::ExportEngine;
use crate::error::{FeishuError, Result};
use crate::models::export::ExportFormat;
use crate::models::wiki::Node;
use crate::storage::{ConfigStore, TokenStore};
use colored::Colorize;
use std::path::PathBuf;

/// Export 子命令
pub struct ExportCommand {
    client: FeishuClient,
    config_store: ConfigStore,
    token_store: TokenStore,
}

impl ExportCommand {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: FeishuClient::new(),
            config_store: ConfigStore::new()?,
            token_store: TokenStore::new()?,
        })
    }

    /// 导出知识空间
    pub async fn run(
        &self,
        space_id: &str,
        format: &str,
        output: Option<PathBuf>,
        _node: Option<String>,
        concurrency: Option<usize>,
        resume: bool,
    ) -> Result<()> {
        let token = self.get_valid_token().await?;

        // 解析格式
        let export_format = ExportFormat::from_str(format).ok_or_else(|| {
            FeishuError::ConfigError(format!(
                "Unsupported export format: {}. Supported: auto, docx, pdf, md, xlsx, csv",
                format
            ))
        })?;

        // 输出目录
        let output_dir = output.unwrap_or_else(|| {
            dirs::download_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("feishu-exports")
        });

        std::fs::create_dir_all(&output_dir)?;

        println!("{}", "🔵 飞书文档批量导出工具 v0.1.0".cyan());
        println!(
            "{}",
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
        );
        println!("{}", format!("空间 ID:    {}", space_id).dimmed());
        println!("{}", format!("导出格式:    {:?}", export_format).dimmed());
        println!(
            "{}",
            format!("输出目录:    {}", output_dir.display()).dimmed()
        );
        if resume {
            println!("{}", "断点续导:   是".dimmed());
        }
        println!(
            "{}",
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
        );

        // 创建导出引擎
        let mut engine = ExportEngine::new(self.client.clone(), token);
        if let Some(c) = concurrency {
            engine = engine.with_concurrency(c);
        }

        // 执行导出
        let start_time = std::time::Instant::now();
        let _zip_path = engine
            .export_space(space_id, export_format, &output_dir, resume)
            .await?;
        let duration = start_time.elapsed();

        println!(
            "\n{}",
            format!("📊 耗时: {}", format_duration(duration)).cyan()
        );

        Ok(())
    }

    /// 获取有效的访问令牌
    async fn get_valid_token(&self) -> Result<String> {
        let mut token_data = self.token_store.load()?;

        if token_data.is_expired() {
            println!("{}", "🔵 访问令牌已过期，正在刷新...".yellow());
            token_data = self
                .client
                .refresh_user_token(&self.config_store, &token_data.refresh_token)
                .await?;
            self.token_store.save(&token_data)?;
            println!("{}", "✅ 令牌刷新成功".green());
        }

        Ok(token_data.access_token)
    }
}

/// 格式化持续时间
fn format_duration(duration: std::time::Duration) -> String {
    let secs = duration.as_secs();
    let mins = secs / 60;
    let secs = secs % 60;
    format!("{} 分 {} 秒", mins, secs)
}

// ─────────────────────────────────────────────────────────────────────────────
// 单篇导出（调试用）
// ─────────────────────────────────────────────────────────────────────────────

impl ExportCommand {
    /// 导出单篇文档，带详细日志
    pub async fn run_one(
        &self,
        obj_token: &str,
        obj_type: &str,
        format: &str,
        output: Option<PathBuf>,
    ) -> Result<()> {
        println!();
        println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
        println!("{}", "🔵 单篇导出调试模式".cyan().bold());
        println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
        println!("obj_token: {}", obj_token);
        println!("obj_type:  {}", obj_type);
        println!("format:    {}", format);
        println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());

        // 1. 获取 token
        print!("\n[Step 1/5] 获取访问令牌... ");
        let token = self.get_valid_token().await?;
        println!("OK ({} chars)", token.len().min(20));

        // 2. 解析格式
        print!("[Step 2/5] 解析导出格式... ");
        let export_format = ExportFormat::from_str(format).ok_or_else(|| {
            FeishuError::ConfigError(format!(
                "Unsupported format: {}. Supported: auto, docx, pdf, md, xlsx, csv",
                format
            ))
        })?;
        println!("OK ({:?})", export_format);

        // 3. 构造 Node
        print!("[Step 3/5] 构造 Node 对象... ");
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
        println!("OK (title={})", node.title);

        // 4. 创建临时目录
        let output_dir = output.unwrap_or_else(|| {
            dirs::download_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("feishu-exports")
        });
        std::fs::create_dir_all(&output_dir)?;
        let temp_dir = output_dir.join(format!("temp_debug_{}", obj_token));
        std::fs::create_dir_all(&temp_dir)?;
        println!("[Step 4/5] 临时目录: {}", temp_dir.display());
        println!("             (导出完成后会清理此目录)");

        // 5. 调用导出引擎
        println!("\n[Step 5/5] 开始导出...");
        let client = FeishuClient::new();
        let file_path = ExportEngine::export_single_document(
            &client,
            &token,
            &node,
            export_format,
            &temp_dir,
            &format!("1_{}", node.title),
        )
        .await;

        match file_path {
            Ok(path) => {
                // 复制到输出目录
                let final_path = output_dir.join(format!(
                    "{}.{}",
                    node.title,
                    path.extension().and_then(|s| s.to_str()).unwrap_or("bin")
                ));
                std::fs::copy(&path, &final_path)?;
                println!();
                println!("{}", "✅ 导出成功！".green().bold());
                println!("  保存位置: {}", final_path.display());
                // 清理临时目录
                let _ = std::fs::remove_dir_all(&temp_dir);
            }
            Err(e) => {
                println!();
                println!("{}", "❌ 导出失败".red().bold());
                println!("  错误类型: {:?}", std::any::type_name_of_val(&e));
                println!("  错误信息: {}", e);
                // 打印临时目录里的内容
                println!();
                println!("  临时目录内容:");
                for entry in std::fs::read_dir(&temp_dir).unwrap_or_else(|_| {
                    panic!("无法读取临时目录: {}", temp_dir.display())
                }) {
                    let entry = entry.unwrap();
                    println!("    - {} ({} bytes)", entry.path().display(), entry.metadata().map(|m| m.len()).unwrap_or(0));
                }
            }
        }

        Ok(())
    }
}
