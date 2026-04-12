use serde::{Deserialize, Serialize};
use std::fmt;

/// 知识空间列表响应
#[derive(Debug, Deserialize)]
pub struct SpacesResponse {
    pub code: i32,
    pub data: SpacesData,
}

#[derive(Debug, Deserialize)]
pub struct SpacesData {
    pub items: Vec<Space>,
    #[serde(default)]
    pub page_token: String,
    #[serde(default)]
    pub has_more: bool,
}

/// 知识空间
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Space {
    pub space_id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub space_type: String,
    #[serde(default)]
    pub visibility: String,
    #[serde(default)]
    pub open_sharing: String,
}

impl fmt::Display for Space {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({}, {})",
            self.name,
            self.space_id,
            if self.visibility == "public" { "公开" } else { "私有" }
        )
    }
}

/// 知识库节点列表响应
#[derive(Debug, Deserialize)]
pub struct NodesResponse {
    pub code: i32,
    pub data: NodesData,
}

#[derive(Debug, Deserialize)]
pub struct NodesData {
    pub items: Vec<Node>,
    #[serde(default)]
    pub page_token: String,
    #[serde(default)]
    pub has_more: bool,
}

/// 知识库节点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub space_id: String,
    pub node_token: String,
    pub obj_token: String,
    pub obj_type: String,
    pub node_type: String,
    pub title: String,
    pub has_child: bool,
    #[serde(default)]
    pub parent_node_token: Option<String>,
    #[serde(default)]
    pub depth: u32,
}

impl Node {
    pub fn is_folder(&self) -> bool {
        self.obj_type == "folder"
    }

    /// 飞书 export API 支持的类型
    pub fn is_exportable(&self) -> bool {
        matches!(self.obj_type.as_str(), "docx" | "doc" | "sheet" | "bitable" | "file")
    }

    pub fn is_shortcut(&self) -> bool {
        self.node_type == "shortcut"
    }

    pub fn safe_filename(&self) -> String {
        sanitize_filename::sanitize(&self.title)
    }
}

#[derive(Debug, Deserialize)]
pub struct NodeInfoResponse {
    pub code: i32,
    pub data: NodeInfo,
}

#[derive(Debug, Deserialize)]
pub struct NodeInfo {
    pub node_token: String,
    pub obj_token: String,
    pub obj_type: String,
    pub title: String,
    #[serde(default)]
    pub link: String,
}
