use crate::api::FeishuClient;
use crate::error::Result;
use crate::models::wiki::Node;
use std::collections::HashMap;

/// 节点树管理器
pub struct NodeTreeManager {
    client: FeishuClient,
}

impl NodeTreeManager {
    pub fn new(client: FeishuClient) -> Self {
        Self { client }
    }

    /// 构建节点路径映射（用于保留目录结构）
    pub fn build_path_map(&self, nodes: &[Node]) -> HashMap<String, String> {
        let mut path_map = HashMap::new();

        // 首先按 node_token 建立索引
        let mut node_index: HashMap<String, &Node> = HashMap::new();
        for node in nodes {
            node_index.insert(node.node_token.clone(), node);
        }

        // 为每个节点构建完整路径
        for node in nodes {
            if node.is_folder() {
                continue;
            }

            let path = self.build_node_path(node, &node_index);
            path_map.insert(node.obj_token.clone(), path);
        }

        path_map
    }

    /// 递归构建节点路径
    fn build_node_path(&self, node: &Node, node_index: &HashMap<String, &Node>) -> String {
        let mut parts = Vec::new();
        parts.push(node.safe_filename());

        let mut current = node;
        while let Some(parent_token) = &current.parent_node_token {
            if let Some(parent) = node_index.get(parent_token) {
                parts.push(parent.safe_filename());
                current = parent;
            } else {
                break;
            }
        }

        // 反转并拼接
        parts.reverse();
        parts.join("/")
    }

    /// 过滤可导出节点
    pub fn filter_exportable(&self, nodes: Vec<Node>) -> Vec<Node> {
        nodes.into_iter().filter(|n| n.is_exportable()).collect()
    }

    /// 按深度排序节点（确保父节点先于子节点处理）
    pub fn sort_by_depth(&self, nodes: &mut Vec<Node>) {
        nodes.sort_by(|a, b| a.depth.cmp(&b.depth));
    }

    /// 打印节点树（用于调试和展示）
    pub fn print_tree(&self, nodes: &[Node]) {
        for node in nodes {
            let indent = "  ".repeat(node.depth as usize);
            let icon = if node.is_folder() {
                "📁"
            } else if node.obj_type == "docx" {
                "📄"
            } else if node.obj_type == "sheet" {
                "📊"
            } else if node.obj_type == "bitable" {
                "📋"
            } else {
                "📎"
            };

            println!("{}{} {}", indent, icon, node.title);
        }
    }
}
