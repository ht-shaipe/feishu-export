use colored::Colorize;
use feishu_core::error::FeishuCoreError as Error;
use feishu_core::models::wiki::Node;
use feishu_core::storage::{ConfigStore, TokenStore};
use feishu_core::FeishuClient;

/// Spaces 子命令
pub struct SpacesCommand {
    client: FeishuClient,
    config_store: ConfigStore,
    token_store: TokenStore,
}

impl SpacesCommand {
    pub fn new() -> std::result::Result<Self, Error> {
        Ok(Self {
            client: FeishuClient::new(),
            config_store: ConfigStore::new(),
            token_store: TokenStore::new(),
        })
    }

    /// 列出知识空间
    pub async fn list(&self) -> std::result::Result<(), Error> {
        let token = self.get_valid_token().await?;

        let spaces = self.client.list_spaces(&token).await
            .map_err(map_err)?;

        println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
        println!("{}", format!("✅ 找到 {} 个知识空间:", spaces.len()).green());
        println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());

        for (i, space) in spaces.iter().enumerate() {
            let visibility_label = if space.visibility == "public" { "公开".green() } else { "私有".yellow() };
            println!("   {}. {} - ({}) [{}]", i + 1, space.name.cyan(), space.space_id.dimmed(), visibility_label);
        }

        println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
        Ok(())
    }

    /// 显示文档树
    pub async fn tree(&self, space_id: &str) -> std::result::Result<(), Error> {
        let token = self.get_valid_token().await?;

        let nodes = self.client.get_node_tree(&token, space_id).await
            .map_err(map_err)?;

        if nodes.is_empty() {
            println!("{}", "⚠️ 该空间没有可访问的文档".yellow());
            return Ok(());
        }

        print_tree(&nodes);

        println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
        println!("{}", format!("共 {} 个节点", nodes.len()).dimmed());
        Ok(())
    }

    /// 显示空间详情
    pub async fn info(&self, space_id: &str) -> std::result::Result<(), Error> {
        let token = self.get_valid_token().await?;

        let spaces = self.client.list_spaces(&token).await
            .map_err(map_err)?;

        let space = spaces.iter()
            .find(|s| s.space_id == space_id)
            .ok_or_else(|| Error::ApiError { code: 404, msg: format!("Space not found: {}", space_id) })?;

        println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
        println!("{}", "📋 空间信息".cyan());
        println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
        println!("ID:          {}", space.space_id);
        println!("名称:        {}", space.name);
        println!("类型:        {}", space.space_type);
        println!("可见性:      {}", space.visibility);
        println!("分享设置:   {}", space.open_sharing);
        if !space.description.is_empty() {
            println!("描述:        {}", space.description);
        }
        println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
        Ok(())
    }

    /// 列出知识库所有文档（平铺）
    pub async fn list_docs(
        &self,
        space_id: &str,
        filter_type: Option<&str>,
        csv: bool,
    ) -> std::result::Result<(), Error> {
        let token = self.get_valid_token().await?;

        let nodes = self.client.get_node_tree(&token, space_id).await
            .map_err(map_err)?;

        let docs: Vec<&Node> = nodes.iter()
            .filter(|n| !n.is_folder())
            .filter(|n| filter_type.map_or(true, |ft| n.obj_type == ft))
            .collect();

        if docs.is_empty() {
            println!("{}", "⚠️ 没有找到符合条件的文档".yellow());
            return Ok(());
        }

        if csv {
            println!("obj_token,title,obj_type,parent_path");
            for node in &docs {
                let parent_path = build_parent_path(&nodes, node)
                    .map(|p| p.replace('"', "\"\""))
                    .unwrap_or_default();
                println!("\"{}\",\"{}\",\"{}\",\"{}\"",
                    node.obj_token, node.title.replace('"', "\"\""), node.obj_type, parent_path);
            }
        } else {
            println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
            println!("{}", format!("✅ 找到 {} 个文档:", docs.len()).green());
            println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
            println!("{:36}  {:10}  {}", "token".dimmed(), "type".dimmed(), "title".dimmed());
            println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());

            for node in &docs {
                let type_label = node.obj_type.as_str();
                println!("{}  {:10}  {}", node.obj_token.dimmed(), type_label, node.title);
            }

            println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
            println!("{}", format!("共 {} 个文档", docs.len()).dimmed());
        }

        Ok(())
    }

    async fn get_valid_token(&self) -> std::result::Result<String, Error> {
        let mut token_data = self.token_store.load().await
            .map_err(map_err)?;

        if token_data.is_expired() {
            println!("{}", "🔵 访问令牌已过期，正在刷新...".yellow());
            token_data = self.client.refresh_user_token(&self.config_store, &token_data.refresh_token)
                .await
                .map_err(map_err)?;
            self.token_store.save(&token_data).await
                .map_err(map_err)?;
            println!("{}", "✅ 令牌刷新成功".green());
        }

        Ok(token_data.access_token)
    }
}

/// 打印文档树
fn print_tree(nodes: &[Node]) {
    let roots: Vec<&Node> = nodes.iter()
        .filter(|n| n.parent_node_token.is_none() || n.parent_node_token.as_ref().is_some_and(|p| p.is_empty()))
        .collect();
    for root in roots {
        print_node_tree(root, nodes, 0);
    }
}

fn print_node_tree(node: &Node, all_nodes: &[Node], depth: usize) {
    let indent = "    ".repeat(depth);
    let icon = match node.obj_type.as_str() {
        "folder" => "📁",
        "docx" | "doc" => "📄",
        "sheet" => "📊",
        "bitable" => "🗃",
        "shortcut" => "🔗",
        _ => "📎",
    };
    let tag = if node.is_exportable() { "" } else { " [不可导出]" };

    println!("{}{}{} {}", indent, icon, node.title.dimmed(), tag.dimmed());

    let children: Vec<&Node> = all_nodes.iter()
        .filter(|n| n.parent_node_token.as_ref() == Some(&node.node_token))
        .collect();
    for child in children {
        print_node_tree(child, all_nodes, depth + 1);
    }
}

fn build_parent_path(all_nodes: &[Node], node: &Node) -> Option<String> {
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

fn map_err(e: feishu_core::FeishuCoreError) -> Error { e }
