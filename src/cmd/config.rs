use colored::Colorize;
use feishu_core::storage::ConfigStore;
use feishu_core::FeishuCoreError;
use rpassword::read_password;
use std::io::{self, Write};

type Result<T> = std::result::Result<T, FeishuCoreError>;

/// 配置子命令
pub struct ConfigCommand {
    config_store: ConfigStore,
}

impl ConfigCommand {
    pub fn new() -> Result<Self> {
        Ok(Self {
            config_store: ConfigStore::new(),
        })
    }

    /// 设置应用凭证
    pub fn set(&self, app_id: Option<String>, app_secret: Option<String>) -> Result<()> {
        let (final_id, final_secret): (String, String) = match (app_id, app_secret) {
            (Some(id), Some(secret)) => (id, secret),
            (Some(id), None) => {
                eprint!("请输入 App Secret: ");
                let secret = read_password()
                    .map_err(|e| FeishuCoreError::StorageError(format!("read password: {}", e)))?;
                (id, secret)
            }
            (None, _) => {
                println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
                println!("{}", "🔑 飞书应用凭证配置".cyan());
                println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
                println!("请前往飞书开发者后台创建自建应用：");
                println!("  {}", "https://open.feishu.cn/app".blue().underline());
                println!();
                let id = self.read_input("App ID")?;
                eprint!("App Secret (输入不回显): ");
                let secret = read_password()
                    .map_err(|e| FeishuCoreError::StorageError(format!("read password: {}", e)))?;
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

        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| FeishuCoreError::StorageError(format!("tokio runtime: {}", e)))?;
        rt.block_on(self.config_store.set_credentials(final_id.trim(), final_secret.trim().to_string()))?;

        println!();
        println!("{}", "✅ 配置已保存".green());
        println!("   配置文件: {}", self.config_store.config_path().display());
        println!();
        println!("{}", "下一步: 运行 feishu-export login 完成授权".cyan());
        Ok(())
    }

    /// 显示当前配置
    pub fn show(&self) -> Result<()> {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| FeishuCoreError::StorageError(format!("tokio runtime: {}", e)))?;
        let config: feishu_core::storage::AppConfig = rt.block_on(self.config_store.load())
            .map_err(|e| FeishuCoreError::StorageError(e.to_string()))?;

        println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
        println!("{}", "📋 当前配置".cyan());
        println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
        println!("App ID:       {}", config.app_id);
        println!("App Secret:   {}", "*".repeat(config.app_secret.len().min(8)));
        println!("Redirect URI: {}", config.redirect_uri);
        println!("数据目录:     {}", config.data_dir.display());
        println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
        Ok(())
    }

    /// 清除配置
    pub fn clear(&self) -> Result<()> {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| FeishuCoreError::StorageError(format!("tokio runtime: {}", e)))?;
        rt.block_on(self.config_store.clear())
            .map_err(|e| FeishuCoreError::StorageError(e.to_string()))?;
        println!("{}", "✅ 配置已清除".green());
        Ok(())
    }

    fn read_input(&self, prompt: &str) -> std::result::Result<String, FeishuCoreError> {
        eprint!("{}: ", prompt);
        io::stdout().flush()
            .map_err(|e| FeishuCoreError::StorageError(format!("flush: {}", e)))?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)
            .map_err(|e| FeishuCoreError::StorageError(format!("read input: {}", e)))?;
        Ok(input.trim().to_string())
    }
}
