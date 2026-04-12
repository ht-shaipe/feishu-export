use clap::{Parser, Subcommand};
use colored::Colorize;
use feishu_export::cmd;

type Result<T> = std::result::Result<T, feishu_core::FeishuCoreError>;

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
        space_id: String,
        #[arg(short, long, default_value = "docx")]
        format: String,
        #[arg(short, long)]
        output: Option<std::path::PathBuf>,
        #[arg(long)]
        node: Option<String>,
        #[arg(long, default_value = "5")]
        concurrency: usize,
        #[arg(long)]
        resume: bool,
    },
    /// 导出单篇（调试用）
    ExportOne {
        obj_token: String,
        #[arg(long, default_value = "docx")]
        obj_type: String,
        #[arg(short, long, default_value = "docx")]
        format: String,
        #[arg(short, long)]
        output: Option<std::path::PathBuf>,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    Set {
        #[arg(long)]
        app_id: Option<String>,
        #[arg(long)]
        app_secret: Option<String>,
    },
    Show,
    Clear,
}

#[derive(Subcommand)]
enum SpacesAction {
    List,
    Tree {
        space_id: String,
    },
    Info {
        space_id: String,
    },
    ListDocs {
        space_id: String,
        #[arg(long)]
        filter_type: Option<String>,
        #[arg(long)]
        csv: bool,
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
            SpacesAction::ListDocs { space_id, filter_type, csv } => {
                cmd::SpacesCommand::new()?
                    .list_docs(&space_id, filter_type.as_deref(), csv)
                    .await?;
            }
        },
        Commands::Export { space_id, format, output, node, concurrency, resume } => {
            if node.is_some() {
                eprintln!("{}", "⚠️ --node 参数暂未实现".yellow());
            }
            cmd::ExportCommand::new()?
                .run(&space_id, &format, output, node, Some(concurrency), resume)
                .await?;
        }
        Commands::ExportOne { obj_token, obj_type, format, output } => {
            cmd::ExportCommand::new()?
                .run_one(&obj_token, &obj_type, &format, output)
                .await?;
        }
    }

    Ok(())
}
