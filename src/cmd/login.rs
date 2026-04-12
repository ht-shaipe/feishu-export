use crate::api::FeishuClient;
use crate::error::{FeishuError, Result};
use crate::models::auth::{OAuthCallback, OAuthState};
use crate::storage::{ConfigStore, TokenStore};
use colored::Colorize;
use std::sync::Arc;
use tokio::sync::Mutex;

/// 登录子命令
pub struct LoginCommand {
    client: FeishuClient,
    config_store: ConfigStore,
    token_store: TokenStore,
}

impl LoginCommand {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: FeishuClient::new(),
            config_store: ConfigStore::new()?,
            token_store: TokenStore::new()?,
        })
    }

    /// 执行 OAuth 登录流程
    pub async fn run(&self, no_browser: bool) -> Result<()> {
        println!("{}", "🔵 正在启动 OAuth 授权流程...".blue());

        // 检查配置
        if !self.config_store.config_path().exists() {
            return Err(FeishuError::ConfigNotFound);
        }

        // 生成 state
        let state = OAuthState::new();
        let state_str = state.state.clone();

        // 构造授权 URL
        let auth_url = self.client.build_auth_url(&self.config_store, &state_str)?;

        // 启动本地 HTTP 服务器接收回调
        let (tx, rx) = std::sync::mpsc::channel();
        // 添加 ready 信号（sync_channel 确保服务器就绪后再打开浏览器）
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<()>(1);
        let tx = Arc::new(Mutex::new(tx));
        let tx_clone = Arc::clone(&tx);

        let server_state = state_str.clone();
        tokio::spawn(async move {
            if let Err(e) =
                Self::start_callback_server(server_state, tx_clone, ready_tx).await
            {
                eprintln!("服务器错误: {}", e);
            }
        });

        // 等待服务器就绪（关键：确保端口已监听再打开浏览器）
        let _ = ready_rx
            .recv()
            .map_err(|e| FeishuError::HttpServerError(format!("Server ready signal error: {}", e)));

        // 打开浏览器
        if no_browser {
            println!("{}", "请复制以下 URL 到浏览器中打开:".yellow());
            println!("{}", auth_url.cyan().underline());
        } else {
            println!("{}", "正在打开浏览器...".blue());
            if let Err(_) = webbrowser::open(&auth_url) {
                println!("{}", "⚠️ 无法自动打开浏览器，请手动打开上述 URL".yellow());
                println!("{}", "URL:".yellow());
                println!("{}", auth_url.cyan().underline());
            }
        }

        println!("{}", "⏳ 等待授权回调...".blue());
        println!("（授权完成后此页面会自动关闭）");

        // 等待回调
        let callback = rx.recv().map_err(|e| {
            FeishuError::HttpServerError(format!("Failed to receive callback: {}", e))
        })?;

        // 用 code 换取 token
        println!("{}", "🔵 正在获取访问令牌...".blue());
        let token_data = self
            .client
            .exchange_code_for_token(&self.config_store, &callback.code)
            .await?;

        // 保存 token
        self.token_store.save(&token_data)?;

        println!();
        println!("{}", "✅ 登录成功！".green());
        println!("访问令牌有效期: 约 6.5 小时");
        println!("刷新令牌有效期: 约 30 天");

        Ok(())
    }

    /// 注销
    pub fn logout(&self) -> Result<()> {
        self.token_store.clear()?;
        println!("{}", "✅ 已注销".green());
        Ok(())
    }

    /// 启动本地 HTTP 服务器接收回调
    async fn start_callback_server(
        expected_state: String,
        tx: Arc<Mutex<std::sync::mpsc::Sender<OAuthCallback>>>,
        ready_tx: std::sync::mpsc::SyncSender<()>,
    ) -> Result<()> {
        use std::net::SocketAddr;
        use tokio::net::TcpListener;

        let addr: SocketAddr = "127.0.0.1:8765".parse()?;
        let listener = TcpListener::bind(&addr).await?;
        println!("🔵 回调服务器监听: http://{}", addr);

        // 通知主线程：服务器已就绪，可以打开浏览器了
        let _ = ready_tx.send(());

        // 只接收一个请求
        loop {
            let (mut stream, _) = listener.accept().await?;
            let tx_clone = Arc::clone(&tx);
            let state_clone = expected_state.clone();

            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};

                let mut read_buf = [0; 4096];
                let n = stream.read(&mut read_buf).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&read_buf[..n]);

                // 解析回调参数
                if let Some(query_start) = request.find("GET /callback?") {
                    let query_part = &request[query_start + 15..];
                    if let Some(query_end) = query_part.find(' ') {
                        let query = &query_part[..query_end];
                        let params = url::form_urlencoded::parse(query.as_bytes());

                        let mut code = None;
                        let mut state = None;

                        for (key, value) in params {
                            if key == "code" {
                                code = Some(value.to_string());
                            } else if key == "state" {
                                state = Some(value.to_string());
                            }
                        }

                        if let (Some(c), Some(s)) = (code, state) {
                            // 验证 state
                            if s != state_clone {
                                let response = "HTTP/1.1 400 Bad Request\r\n\r\nState mismatch";
                                let _ = stream.write_all(response.as_bytes()).await;
                                return;
                            }

                            // 发送成功响应
                            let html = r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"><title>授权成功</title></head>
<body style="font-family:sans-serif;text-align:center;padding-top:80px;background:#f8f8f8">
<h1 style="color:#52c41a">✅ 授权成功</h1>
<p>您可以关闭此页面并返回终端。</p>
</body>
</html>"#;

                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
                                html.len(),
                                html
                            );
                            let _ = stream.write_all(response.as_bytes()).await;

                            // 发送回调数据
                            let _ = tx_clone
                                .lock()
                                .await
                                .send(OAuthCallback { code: c, state: s });
                        }
                    }
                }
            });

            break;
        }

        Ok(())
    }
}
