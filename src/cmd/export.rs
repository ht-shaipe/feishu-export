use crate::api::FeishuClient;
use crate::engine::{ExportEngine, NodeTreeManager};
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
