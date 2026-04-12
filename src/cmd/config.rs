use crate::api::FeishuClient;
use crate::error::Result;
use crate::storage::ConfigStore;
use colored::Colorize;
use rpassword::read_password;

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
    pub fn set(&self, app_id: &str, app_secret: Option<String>) -> Result<()> {
        let secret = if let Some(s) = app_secret {
            s
        } else {
            eprint!("请输入 App Secret: ");
            read_password().map_err(|e| {
                crate::error::FeishuError::StorageError(format!("Failed to read password: {}", e))
            })?
        };

        self.config_store.set_credentials(app_id, secret)?;
        println!("{}", "✅ 配置已保存".green());
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
        println!("App Secret:   {}", "*".repeat(config.app_secret.len()));
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
}
