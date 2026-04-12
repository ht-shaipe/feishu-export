//! Node tree utilities

use crate::api::FeishuClient;
use crate::models::wiki::Node;
use std::collections::HashMap;

/// Node tree utilities: build paths, filter exportable nodes
pub struct NodeTreeManager {
    #[allow(dead_code)]
    client: FeishuClient,
}

impl NodeTreeManager {
    pub fn new(client: FeishuClient) -> Self {
        Self { client }
    }

    /// Build a path map: obj_token → relative path string (e.g. "1_title/2_sub")
    pub fn build_path_map(&self, nodes: &[Node]) -> HashMap<String, String> {
        let mut token_to_parent: HashMap<String, String> = HashMap::new();
        let mut token_to_node: HashMap<String, &Node> = HashMap::new();
        let mut result: HashMap<String, String> = HashMap::new();

        for node in nodes {
            token_to_parent.insert(node.node_token.clone(), node.parent_node_token.clone().unwrap_or_default());
            token_to_node.insert(node.node_token.clone(), node);
        }

        for node in nodes {
            let path = self.build_node_path(node, &token_to_parent, &token_to_node);
            result.insert(node.obj_token.clone(), path);
        }

        result
    }

    fn build_node_path(
        &self,
        node: &Node,
        _token_to_parent: &HashMap<String, String>,
        token_to_node: &HashMap<String, &Node>,
    ) -> String {
        let mut parts = Vec::new();
        let mut current = node.parent_node_token.clone();

        while let Some(parent) = current {
            if parent.is_empty() {
                break;
            }
            if let Some(parent_node) = token_to_node.get(&parent) {
                parts.push(format!("{}_{}", parent_node.depth, parent_node.safe_filename()));
                current = parent_node.parent_node_token.clone();
            } else {
                break;
            }
        }

        parts.reverse();
        let base = format!("{}_{}", node.depth, node.safe_filename());
        if parts.is_empty() {
            base
        } else {
            format!("{}/{}", parts.join("/"), base)
        }
    }

    /// Filter to only exportable nodes, sorted by depth
    pub fn filter_exportable(&self, mut nodes: Vec<Node>) -> Vec<Node> {
        nodes.retain(|n| n.is_exportable() && !n.is_shortcut());
        nodes.sort_by_key(|n| n.depth);
        nodes
    }

    /// Sort nodes by depth (ascending)
    #[allow(dead_code)]
    pub fn sort_by_depth(&self, nodes: &mut Vec<Node>) {
        nodes.sort_by_key(|n| n.depth);
    }
}
