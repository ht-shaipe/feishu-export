use crate::api::FeishuClient;
use crate::engine::NodeTreeManager;
use crate::error::{FeishuError, Result};
use crate::models::wiki::Space;
use crate::storage::{ConfigStore, TokenStore};
use colored::Colorize;

/// Spaces 子命令
pub struct SpacesCommand {
    client: FeishuClient,
    config_store: ConfigStore,
    token_store: TokenStore,
}

impl SpacesCommand {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: FeishuClient::new(),
            config_store: ConfigStore::new()?,
            token_store: TokenStore::new()?,
        })
    }

    /// 列出知识空间
    pub async fn list(&self) -> Result<()> {
        let token = self.get_valid_token().await?;

        println!("{}", "🔵 正在获取知识空间列表...".blue());

        let spaces = self.client.list_spaces(&token).await?;

        println!(
            "{}",
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
        );
        println!(
            "{}",
            format!("✅ 找到 {} 个知识空间:", spaces.len()).green()
        );
        println!(
            "{}",
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
        );

        for (i, space) in spaces.iter().enumerate() {
            println!(
                "   {}. {} - ({}) [{}]",
                i + 1,
                space.name.cyan(),
                space.space_id.dimmed(),
                if space.visibility == "public" {
                    "公开".green()
                } else {
                    "私有".yellow()
                }
            );
        }

        println!(
            "{}",
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
        );

        Ok(())
    }

    /// 显示文档树
    pub async fn tree(&self, space_id: &str) -> Result<()> {
        let token = self.get_valid_token().await?;

        println!(
            "{}",
            format!("🔵 正在获取空间 {} 的文档树...", space_id).blue()
        );

        let nodes = self.client.get_node_tree(&token, space_id).await?;

        if nodes.is_empty() {
            println!("{}", "⚠️ 该空间没有可访问的文档".yellow());
            return Ok(());
        }

        println!(
            "{}",
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
        );

        let tree_manager = NodeTreeManager::new(self.client.clone());
        tree_manager.print_tree(&nodes);

        println!(
            "{}",
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
        );
        println!("{}", format!("共 {} 个节点", nodes.len()).dimmed());

        Ok(())
    }

    /// 显示空间详情
    pub async fn info(&self, space_id: &str) -> Result<()> {
        let token = self.get_valid_token().await?;

        println!(
            "{}",
            format!("🔵 正在获取空间 {} 的信息...", space_id).blue()
        );

        let spaces = self.client.list_spaces(&token).await?;
        let space = spaces
            .iter()
            .find(|s| s.space_id == space_id)
            .ok_or_else(|| FeishuError::ApiError {
                code: 404,
                msg: format!("Space not found: {}", space_id),
            })?;

        println!(
            "{}",
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
        );
        println!("{}", "📋 空间信息".cyan());
        println!(
            "{}",
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
        );
        println!("ID:          {}", space.space_id);
        println!("名称:        {}", space.name);
        println!("类型:        {}", space.space_type);
        println!("可见性:      {}", space.visibility);
        println!("分享设置:   {}", space.open_sharing);
        if !space.description.is_empty() {
            println!("描述:        {}", space.description);
        }
        println!(
            "{}",
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
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
