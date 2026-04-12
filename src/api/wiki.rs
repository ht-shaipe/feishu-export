use crate::api::FeishuClient;
use crate::error::{FeishuError, Result};
use crate::models::wiki::*;

/// 知识库 API
impl FeishuClient {
    /// 获取知识空间列表
    pub async fn list_spaces(&self, access_token: &str) -> Result<Vec<Space>> {
        let mut spaces = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let mut url = format!("/open-apis/wiki/v2/spaces?page_size=50");
            if let Some(token) = &page_token {
                url.push_str(&format!("&page_token={}", token));
            }

            let response = self.get(&url, access_token).await?;
            let data: SpacesResponse = response.json().await?;

            if data.code != 0 {
                return Err(FeishuError::from_api_response(
                    data.code,
                    "Failed to list spaces".to_string(),
                ));
            }

            spaces.extend(data.data.items);

            if !data.data.has_more {
                break;
            }
            page_token = Some(data.data.page_token);
        }

        Ok(spaces)
    }

    /// 获取空间下的子节点列表
    pub async fn list_nodes(
        &self,
        access_token: &str,
        space_id: &str,
        parent_node_token: Option<&str>,
    ) -> Result<Vec<Node>> {
        let mut nodes = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let mut url = format!("/open-apis/wiki/v2/spaces/{}/nodes?page_size=50", space_id);
            if let Some(token) = parent_node_token {
                url.push_str(&format!("&parent_node_token={}", token));
            }
            if let Some(token) = &page_token {
                url.push_str(&format!("&page_token={}", token));
            }

            let response = self.get(&url, access_token).await?;
            let data: NodesResponse = response.json().await?;

            if data.code != 0 {
                return Err(FeishuError::from_api_response(
                    data.code,
                    format!("Failed to list nodes for space {}", space_id),
                ));
            }

            nodes.extend(data.data.items);

            if !data.data.has_more {
                break;
            }
            page_token = Some(data.data.page_token);
        }

        Ok(nodes)
    }

    /// 获取节点详情
    pub async fn get_node_info(&self, access_token: &str, token: &str) -> Result<NodeInfo> {
        let url = format!("/open-apis/wiki/v2/spaces/get_node?token={}", token);
        let response = self.get(&url, access_token).await?;
        let data: NodeInfoResponse = response.json().await?;

        if data.code != 0 {
            return Err(FeishuError::from_api_response(
                data.code,
                format!("Failed to get node info for token {}", token),
            ));
        }

        Ok(data.data)
    }

    /// 递归获取节点树（DFS 遍历，使用迭代方式避免递归 async）
    pub async fn get_node_tree(&self, access_token: &str, space_id: &str) -> Result<Vec<Node>> {
        let mut all_nodes = Vec::new();
        let mut stack: Vec<(Option<String>, u32)> = vec![(None, 0)];

        while let Some((parent_token, depth)) = stack.pop() {
            let nodes = self
                .list_nodes(access_token, space_id, parent_token.as_deref())
                .await?;

            // 反向遍历，保证子节点按正确顺序处理
            for node in nodes.into_iter().rev() {
                let mut node = node;
                node.depth = depth;

                // 跳过快捷方式，避免重复
                if node.is_shortcut() {
                    continue;
                }

                let node_token = node.node_token.clone();
                let has_child = node.has_child;
                all_nodes.push(node);

                // 如果有子节点，加入栈中
                if has_child {
                    stack.push((Some(node_token), depth + 1));
                }
            }
        }

        Ok(all_nodes)
    }
}
