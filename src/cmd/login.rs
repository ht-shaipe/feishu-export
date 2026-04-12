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
        eprintln!("[LOG] === Login flow started ===");

        // 检查配置
        if !self.config_store.config_path().exists() {
            eprintln!("[LOG] Config file not found: {:?}", self.config_store.config_path());
            return Err(FeishuError::ConfigNotFound);
        }
        eprintln!("[LOG] Config file exists, loading...");
        self.config_store.load()?;
        let loaded = self.config_store.load()?;
        eprintln!("[LOG] Config loaded: app_id={}", loaded.app_id);

        // 生成 state
        let state = OAuthState::new();
        let state_str = state.state.clone();
        eprintln!("[LOG] Generated state: {}", state_str);

        // 构造授权 URL
        let auth_url = self.client.build_auth_url(&self.config_store, &state_str)?;
        eprintln!("[LOG] Auth URL built: {}", auth_url);

        // 启动本地 HTTP 服务器接收回调
        let (tx, rx) = std::sync::mpsc::channel();
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<()>(1);
        let tx = Arc::new(Mutex::new(tx));
        let tx_clone = Arc::clone(&tx);

        let server_state = state_str.clone();
        eprintln!("[LOG] Spawning callback server...");
        let server_handle = tokio::spawn(async move {
            if let Err(e) = Self::start_callback_server(server_state, tx_clone, ready_tx).await {
                eprintln!("[SERVER] Server error: {}", e);
            }
        });

        eprintln!("[LOG] Waiting for server ready signal...");
        let ready_result = ready_rx.recv();
        match ready_result {
            Ok(()) => eprintln!("[LOG] ✅ Server ready signal received"),
            Err(e) => {
                eprintln!("[LOG] ❌ Server ready signal error: {}", e);
                // 即使 ready 信号失败，也继续尝试（端口可能已监听）
            }
        }

        // 打开浏览器
        if no_browser {
            println!("{}", "请复制以下 URL 到浏览器中打开:".yellow());
            println!("{}", auth_url.cyan().underline());
        } else {
            println!("{}", "正在打开浏览器...".blue());
            eprintln!("[LOG] Opening browser with webbrowser::open...");
            if let Err(e) = webbrowser::open(&auth_url) {
                eprintln!("[LOG] webbrowser::open failed: {}", e);
                println!("{}", "⚠️ 无法自动打开浏览器，请手动打开上述 URL".yellow());
                println!("{}", "URL:".yellow());
                println!("{}", auth_url.cyan().underline());
            } else {
                eprintln!("[LOG] webbrowser::open succeeded");
            }
        }

        println!("{}", "⏳ 等待授权回调...".blue());
        eprintln!("[LOG] Blocking on rx.recv() for callback...");

        // 等待回调
        let callback = rx.recv().map_err(|e| {
            eprintln!("[LOG] ❌ rx.recv() error: {}", e);
            FeishuError::HttpServerError(format!("Failed to receive callback: {}", e))
        })?;

        eprintln!("[LOG] ✅ Received callback: code={}, state={}", callback.code, callback.state);

        // 取消服务器任务（不再需要）
        server_handle.abort();

        // 用 code 换取 token
        eprintln!("[LOG] Exchanging code for token...");
        let token_data = self
            .client
            .exchange_code_for_token(&self.config_store, &callback.code)
            .await?;

        eprintln!("[LOG] Token received, saving...");
        // 保存 token
        self.token_store.save(&token_data)?;

        eprintln!("[LOG] ✅ Login complete");
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

        eprintln!("[SERVER] Starting callback server...");

        let addr: SocketAddr = "127.0.0.1:8765".parse()?;
        eprintln!("[SERVER] Binding to {}", addr);

        let listener = TcpListener::bind(&addr).await?;
        eprintln!("[SERVER] ✅ Bound successfully, listening on http://{}", addr);

        // 通知主线程：服务器已就绪
        eprintln!("[SERVER] Sending ready signal...");
        let send_result = ready_tx.send(());
        eprintln!("[SERVER] Ready signal send result: {:?}", send_result);

        eprintln!("[SERVER] Waiting for incoming connection...");
        let (mut stream, remote_addr) = listener.accept().await?;
        eprintln!("[SERVER] ✅ Connection from {}", remote_addr);

        let tx_clone = Arc::clone(&tx);
        let state_clone = expected_state.clone();

        // 处理请求
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut read_buf = [0; 8192];
        let n = stream.read(&mut read_buf).await.unwrap_or(0);
        let request = String::from_utf8_lossy(&read_buf[..n]).to_string();

        eprintln!("[SERVER] Request:\n{}", request.lines().take(5).collect::<Vec<_>>().join("\n"));

        // 解析回调参数
        if let Some(query_start) = request.find("/callback?") {
            let after_callback = &request[query_start + 10..];
            eprintln!("[SERVER] Path after /callback?: {}", after_callback.split_whitespace().next().unwrap_or(""));
            if let Some(query_end) = after_callback.find(' ') {
                let query = &after_callback[..query_end];
                eprintln!("[SERVER] Query string: {}", query);

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

                eprintln!("[SERVER] Parsed: code={:?}, state={:?}", code, state);

                if let (Some(c), Some(s)) = (code, state) {
                    // 验证 state
                    if s != state_clone {
                        eprintln!("[SERVER] ❌ State mismatch: expected={}, got={}", state_clone, s);
                        let response = "HTTP/1.1 400 Bad Request\r\n\r\nState mismatch";
                        let _ = stream.write_all(response.as_bytes()).await;
                        return Ok(());
                    }

                    eprintln!("[SERVER] ✅ State validated");

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
                    eprintln!("[SERVER] Sending 200 response...");
                    let _ = stream.write_all(response.as_bytes()).await;
                    eprintln!("[SERVER] ✅ Response sent");

                    // 发送回调数据到主线程
                    eprintln!("[SERVER] Sending callback to main thread...");
                    let send_result = tx_clone.lock().await.send(OAuthCallback { code: c.clone(), state: s });
                    eprintln!("[SERVER] Send result: {:?}", send_result);
                } else {
                    eprintln!("[SERVER] ❌ Missing code or state in callback");
                }
            }
        } else {
            eprintln!("[SERVER] ❌ Path doesn't contain /callback?: {}", request.lines().next().unwrap_or(""));
        }

        Ok(())
    }
}
