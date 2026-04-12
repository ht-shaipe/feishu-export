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

    /// 列出知识库所有文档（平铺）
    pub async fn list_docs(
        &self,
        space_id: &str,
        filter_type: Option<&str>,
        csv: bool,
    ) -> Result<()> {
        let token = self.get_valid_token().await?;

        println!(
            "{}",
            format!("🔵 正在获取空间 {} 的文档列表...", space_id).blue()
        );

        let nodes = self.client.get_node_tree(&token, space_id).await?;

        // 过滤出非文件夹节点（可导出文档）
        let docs: Vec<_> = nodes
            .iter()
            .filter(|n| !n.is_folder())
            .filter(|n| {
                if let Some(ft) = filter_type {
                    n.obj_type == ft
                } else {
                    true
                }
            })
            .collect();

        if docs.is_empty() {
            println!("{}", "⚠️ 没有找到符合条件的文档".yellow());
            return Ok(());
        }

        if csv {
            // CSV 输出：obj_token, title, obj_type, parent_path
            println!("obj_token,title,obj_type,parent_path");
            for node in &docs {
                let parent_path = self
                    .build_parent_path(&nodes, node)
                    .map(|p| p.replace('"', "\"\""))
                    .unwrap_or_default();
                println!(
                    "\"{}\",\"{}\",\"{}\",\"{}\"",
                    node.obj_token,
                    node.title.replace('"', "\"\""),
                    node.obj_type,
                    parent_path
                );
            }
            println!(
                "{}",
                format!("共 {} 条记录（CSV 格式）", docs.len()).dimmed()
            );
        } else {
            // 表格输出
            let total = docs.len();
            println!(
                "{}",
                "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
                    .dimmed()
            );
            println!(
                "{}",
                format!(
                    "✅ 找到 {} 个文档:",
                    total
                )
                .green()
            );
            println!(
                "{}",
                "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
                    .dimmed()
            );
            println!(
                "{:36}  {:10}  {}",
                "token".dimmed(),
                "type".dimmed(),
                "title".dimmed()
            );
            println!(
                "{}",
                "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
                    .dimmed()
            );
            for node in &docs {
                let type_icon = match node.obj_type.as_str() {
                    "docx" => "docx".cyan(),
                    "sheet" => "sheet".yellow(),
                    "bitable" => "bitable".magenta(),
                    "doc" => "doc".blue(),
                    _ => node.obj_type.white(),
                };
                println!(
                    "{}  {:10}  {}",
                    node.obj_token.dimmed(),
                    type_icon,
                    node.title
                );
            }
            println!(
                "{}",
                "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
                    .dimmed()
            );
            println!("{}", format!("共 {} 个文档", total).dimmed());
        }

        Ok(())
    }

    /// 构建节点的父路径
    fn build_parent_path(&self, all_nodes: &[crate::models::wiki::Node], node: &crate::models::wiki::Node) -> Option<String> {
        let mut path = Vec::new();
        let mut current = node.parent_node_token.clone();

        while let Some(parent_token) = current {
            if let Some(parent) = all_nodes.iter().find(|n| n.node_token == parent_token) {
                path.push(parent.title.clone());
                current = parent.parent_node_token.clone();
            } else {
                break;
            }
        }

        if path.is_empty() {
            None
        } else {
            path.reverse();
            Some(path.join(" / "))
        }
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
