use clap::{Parser, Subcommand};
use colored::Colorize;

mod api;
mod cmd;
mod engine;
mod error;
mod models;
mod storage;

use error::Result;

/// 飞书文档批量导出工具
#[derive(Parser)]
#[command(name = "feishu-export")]
#[command(author = "Feishu Export Team")]
#[command(version = "0.1.0")]
#[command(about = "飞书知识库文档批量导出 CLI 工具", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 管理应用配置
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// 用户授权登录
    Login {
        /// 不自动打开浏览器，手动复制 URL
        #[arg(long)]
        no_browser: bool,
    },
    /// 注销
    Logout,
    /// 知识空间管理
    Spaces {
        #[command(subcommand)]
        action: SpacesAction,
    },
    /// 导出知识空间
    Export {
        /// 知识空间 ID
        space_id: String,
        /// 导出格式 (docx, pdf, md, xlsx, csv)
        #[arg(short, long, default_value = "docx")]
        format: String,
        /// 输出目录
        #[arg(short, long)]
        output: Option<std::path::PathBuf>,
        /// 仅导出指定节点
        #[arg(long)]
        node: Option<String>,
        /// 并发数
        #[arg(long, default_value = "5")]
        concurrency: usize,
        /// 断点续导
        #[arg(long)]
        resume: bool,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// 设置应用凭证（快捷方式：feishu-export config set --app-id X --app-secret X）
    Set {
        /// App ID
        #[arg(long)]
        app_id: Option<String>,
        /// App Secret（不传则交互式输入，隐藏回显）
        #[arg(long)]
        app_secret: Option<String>,
    },
    /// 显示当前配置
    Show,
    /// 清除配置
    Clear,
}

#[derive(Subcommand)]
enum SpacesAction {
    /// 列出知识空间
    List,
    /// 显示文档树
    Tree {
        /// 知识空间 ID
        space_id: String,
    },
    /// 显示空间详情
    Info {
        /// 知识空间 ID
        space_id: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Config { action } => match action {
            ConfigAction::Set { app_id, app_secret } => {
                cmd::ConfigCommand::new()?.set(app_id, app_secret)?;
            }
            ConfigAction::Show => {
                cmd::ConfigCommand::new()?.show()?;
            }
            ConfigAction::Clear => {
                cmd::ConfigCommand::new()?.clear()?;
            }
        },
        Commands::Login { no_browser } => {
            cmd::LoginCommand::new()?.run(no_browser).await?;
        }
        Commands::Logout => {
            cmd::LoginCommand::new()?.logout()?;
        }
        Commands::Spaces { action } => match action {
            SpacesAction::List => {
                cmd::SpacesCommand::new()?.list().await?;
            }
            SpacesAction::Tree { space_id } => {
                cmd::SpacesCommand::new()?.tree(&space_id).await?;
            }
            SpacesAction::Info { space_id } => {
                cmd::SpacesCommand::new()?.info(&space_id).await?;
            }
        },
        Commands::Export {
            space_id,
            format,
            output,
            node,
            concurrency,
            resume,
        } => {
            if node.is_some() {
                eprintln!("{}", "⚠️ --node 参数暂未实现".yellow());
            }
            cmd::ExportCommand::new()?
                .run(&space_id, &format, output, node, Some(concurrency), resume)
                .await?;
        }
    }

    Ok(())
}
