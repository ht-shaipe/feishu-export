use crate::error::Result;
use crate::storage::ConfigStore;
use colored::Colorize;
use rpassword::read_password;
use std::io::{self, Write};

/// 配置子命令
pub struct ConfigCommand {
    config_store: ConfigStore,
}

impl ConfigCommand {
    pub fn new() -> Result<Self> {
        Ok(Self {
            config_store: ConfigStore::new()?,
        })
    }

    /// 设置应用凭证
    /// - 两个都传：直接设置
    /// - 只传 app_id：交互输入 secret
    /// - 都没传：交互引导（逐步输入）
    pub fn set(&self, app_id: Option<String>, app_secret: Option<String>) -> Result<()> {
        let (final_id, final_secret) = match (app_id, app_secret) {
            (Some(id), Some(secret)) => (id, secret),
            (Some(id), None) => {
                eprint!("请输入 App Secret: ");
                let secret = read_password().map_err(|e| {
                    crate::error::FeishuError::StorageError(format!(
                        "Failed to read password: {}",
                        e
                    ))
                })?;
                (id, secret)
            }
            (None, _) => {
                // 完全交互模式
                println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
                println!("{}", "🔑 飞书应用凭证配置".cyan());
                println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
                println!();
                println!("请前往飞书开发者后台创建自建应用：");
                println!(
                    "  {}",
                    "https://open.feishu.cn/app".blue().underline()
                );
                println!();
                println!("创建后获取 App ID 和 App Secret，填入下方：");
                println!();

                let id = self.read_input("App ID")?;
                eprint!("App Secret (输入不回显): ");
                let secret = read_password().map_err(|e| {
                    crate::error::FeishuError::StorageError(format!(
                        "Failed to read password: {}",
                        e
                    ))
                })?;
                (id, secret)
            }
        };

        if final_id.trim().is_empty() {
            println!("{}", "❌ App ID 不能为空".red());
            return Ok(());
        }
        if final_secret.trim().is_empty() {
            println!("{}", "❌ App Secret 不能为空".red());
            return Ok(());
        }

        self.config_store
            .set_credentials(final_id.trim(), final_secret.trim().to_string())?;

        println!();
        println!("{}", "✅ 配置已保存".green());
        println!(
            "   配置文件: {}",
            self.config_store.config_path().display()
        );
        println!();
        println!("{}", "下一步: 运行 feishu-export login 完成授权".cyan());
        Ok(())
    }

    /// 显示当前配置
    pub fn show(&self) -> Result<()> {
        let config = self.config_store.load()?;

        println!(
            "{}",
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
        );
        println!("{}", "📋 当前配置".cyan());
        println!(
            "{}",
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
        );
        println!("App ID:       {}", config.app_id);
        println!(
            "App Secret:   {}",
            "*".repeat(config.app_secret.len().min(8))
        );
        println!("Redirect URI: {}", config.redirect_uri);
        println!("数据目录:     {}", config.data_dir.display());
        println!(
            "{}",
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
        );

        Ok(())
    }

    /// 清除配置
    pub fn clear(&self) -> Result<()> {
        self.config_store.clear()?;
        println!("{}", "✅ 配置已清除".green());
        Ok(())
    }

    /// 交互式读取用户输入
    fn read_input(&self, prompt: &str) -> Result<String> {
        eprint!("{}: ", prompt);
        io::stdout().flush().map_err(|e| {
            crate::error::FeishuError::StorageError(format!("Flush failed: {}", e))
        })?;
        let mut input = String::new();
        io::stdin().read_line(&mut input).map_err(|e| {
            crate::error::FeishuError::StorageError(format!("Failed to read input: {}", e))
        })?;
        Ok(input.trim().to_string())
    }
}
