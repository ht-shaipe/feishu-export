use clap::{Parser, Subcommand};
use colored::Colorize;
use feishu_export::cmd;
use std::process::ExitCode;

/// 飞书文档批量导出工具
#[derive(Parser)]
#[command(name = "feishu-export")]
#[command(author = "Feishu Export Team")]
#[command(version = "0.1.0")]
#[command(about = "飞书文档导出工具", long_about = None)]
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
    /// 导出知识空间（需登录）
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
    /// 本地 docx 文件转 Markdown（无需登录）
    Convert {
        /// .docx 文件路径，或包含 .docx 文件的目录
        input: std::path::PathBuf,
        /// 输出路径（单文件时可选；目录时忽略）
        #[arg(short, long)]
        output: Option<std::path::PathBuf>,
        /// 递归处理子目录
        #[arg(short, long)]
        recursive: bool,
        /// 仅显示将要转换的文件，不实际写入
        #[arg(long)]
        dry_run: bool,
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
    Tree { space_id: String },
    Info { space_id: String },
    ListDocs {
        space_id: String,
        #[arg(long)]
        filter_type: Option<String>,
        #[arg(long)]
        csv: bool,
    },
}

// ─────────────────────────────────────────────────────────────────────────────
// Sync commands (config, logout, convert)
// ─────────────────────────────────────────────────────────────────────────────

fn run_config(action: ConfigAction) -> ExitCode {
    match action {
        ConfigAction::Set { app_id, app_secret } => {
            match cmd::ConfigCommand::new() {
                Ok(c) => match c.set(app_id, app_secret) {
                    Ok(_) => ExitCode::SUCCESS,
                    Err(e) => { eprintln!("{} {}", "❌".red(), e); ExitCode::FAILURE }
                },
                Err(e) => { eprintln!("{} {}", "❌".red(), e); ExitCode::FAILURE }
            }
        }
        ConfigAction::Show => {
            match cmd::ConfigCommand::new() {
                Ok(c) => match c.show() {
                    Ok(_) => ExitCode::SUCCESS,
                    Err(e) => { eprintln!("{} {}", "❌".red(), e); ExitCode::FAILURE }
                },
                Err(e) => { eprintln!("{} {}", "❌".red(), e); ExitCode::FAILURE }
            }
        }
        ConfigAction::Clear => {
            match cmd::ConfigCommand::new() {
                Ok(c) => match c.clear() {
                    Ok(_) => ExitCode::SUCCESS,
                    Err(e) => { eprintln!("{} {}", "❌".red(), e); ExitCode::FAILURE }
                },
                Err(e) => { eprintln!("{} {}", "❌".red(), e); ExitCode::FAILURE }
            }
        }
    }
}

fn run_logout() -> ExitCode {
    match cmd::LoginCommand::new() {
        Ok(c) => match c.logout() {
            Ok(_) => ExitCode::SUCCESS,
            Err(e) => { eprintln!("{} {}", "❌".red(), e); ExitCode::FAILURE }
        },
        Err(e) => { eprintln!("{} {}", "❌".red(), e); ExitCode::FAILURE }
    }
}

fn run_convert(input: std::path::PathBuf, output: Option<std::path::PathBuf>, recursive: bool, dry_run: bool) -> ExitCode {
    match cmd::ConvertCommand::new().run(&input, output.as_deref(), recursive, dry_run) {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => { eprintln!("{} {}", "❌".red(), e); ExitCode::FAILURE }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Async commands (login, spaces, export) — use a dedicated runtime
// ─────────────────────────────────────────────────────────────────────────────

fn run_async<F>(future_fn: F) -> ExitCode
where
    F: FnOnce() -> std::pin::Pin<Box<dyn std::future::Future<Output = ExitCode>>>,
{
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("{} Runtime error: {}", "❌".red(), e);
            return ExitCode::FAILURE;
        }
    };
    rt.block_on(future_fn())
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Commands::Config { action } => run_config(action),

        Commands::Login { no_browser } => {
            run_async(|| {
                let no_browser = no_browser;
                Box::pin(async move {
                    match cmd::LoginCommand::new() {
                        Ok(c) => match c.run(no_browser).await {
                            Ok(_) => ExitCode::SUCCESS,
                            Err(e) => { eprintln!("{} {}", "❌".red(), e); ExitCode::FAILURE }
                        },
                        Err(e) => { eprintln!("{} {}", "❌".red(), e); ExitCode::FAILURE }
                    }
                })
            })
        }

        Commands::Logout => run_logout(),

        Commands::Spaces { action } => {
            run_async(|| {
                Box::pin(async {
                    let c = match cmd::SpacesCommand::new() {
                        Ok(c) => c,
                        Err(e) => { eprintln!("{} {}", "❌".red(), e); return ExitCode::FAILURE; }
                    };
                    let result = match action {
                        SpacesAction::List => c.list().await,
                        SpacesAction::Tree { space_id } => c.tree(&space_id).await,
                        SpacesAction::Info { space_id } => c.info(&space_id).await,
                        SpacesAction::ListDocs { space_id, filter_type, csv } => {
                            c.list_docs(&space_id, filter_type.as_deref(), csv).await
                        }
                    };
                    match result {
                        Ok(_) => ExitCode::SUCCESS,
                        Err(e) => { eprintln!("{} {}", "❌".red(), e); ExitCode::FAILURE }
                    }
                })
            })
        }

        Commands::Export { space_id, format, output, node, concurrency, resume } => {
            if node.is_some() {
                eprintln!("{}", "⚠️ --node 参数暂未实现".yellow());
            }
            run_async(|| {
                let space_id = space_id;
                let format = format;
                let output = output;
                let node = node;
                let concurrency = concurrency;
                let resume = resume;
                Box::pin(async move {
                    let c = match cmd::ExportCommand::new() {
                        Ok(c) => c,
                        Err(e) => { eprintln!("{} {}", "❌".red(), e); return ExitCode::FAILURE; }
                    };
                    match c.run(&space_id, &format, output, node, Some(concurrency), resume).await {
                        Ok(_) => ExitCode::SUCCESS,
                        Err(e) => { eprintln!("{} {}", "❌".red(), e); ExitCode::FAILURE }
                    }
                })
            })
        }

        Commands::ExportOne { obj_token, obj_type, format, output } => {
            run_async(|| {
                let obj_token = obj_token;
                let obj_type = obj_type;
                let format = format;
                let output = output;
                Box::pin(async move {
                    let c = match cmd::ExportCommand::new() {
                        Ok(c) => c,
                        Err(e) => { eprintln!("{} {}", "❌".red(), e); return ExitCode::FAILURE; }
                    };
                    match c.run_one(&obj_token, &obj_type, &format, output).await {
                        Ok(_) => ExitCode::SUCCESS,
                        Err(e) => { eprintln!("{} {}", "❌".red(), e); ExitCode::FAILURE }
                    }
                })
            })
        }

        Commands::Convert { input, output, recursive, dry_run } => {
            run_convert(input, output, recursive, dry_run)
        }
    }
}
