use colored::Colorize;
use feishu_core::error::FeishuCoreError as Error;
use feishu_core::models::auth::{OAuthCallback, OAuthState};
use feishu_core::storage::{ConfigStore, TokenStore};
use feishu_core::FeishuClient;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;

/// 登录子命令
pub struct LoginCommand {
    client: FeishuClient,
    config_store: ConfigStore,
    token_store: TokenStore,
}

impl LoginCommand {
    pub fn new() -> std::result::Result<Self, Error> {
        Ok(Self {
            client: FeishuClient::new(),
            config_store: ConfigStore::new(),
            token_store: TokenStore::new(),
        })
    }

    pub async fn run(&self, no_browser: bool) -> std::result::Result<(), Error> {
        // 检查配置
        if !self.config_store.config_path().exists() {
            return Err(Error::ConfigNotFound);
        }
        self.config_store.load().await
            .map_err(|e| Error::StorageError(e.to_string()))?;

        // 生成 state
        let state = OAuthState::new();
        let state_str = state.state.clone();

        // 构造授权 URL
        let auth_url = self.client.build_auth_url(&self.config_store, &state_str)
            .await
            .map_err(|e| Error::InvalidUrl(e.to_string()))?;

        // 启动回调服务器
        let (tx, rx) = std::sync::mpsc::channel();
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<()>(1);
        let tx = Arc::new(TokioMutex::new(tx));

        let server_state = state_str.clone();
        let server_handle = tokio::spawn(async move {
            if let Err(e) = Self::start_callback_server(server_state, tx, ready_tx).await {
                eprintln!("[SERVER] Server error: {}", e);
            }
        });

        let _ = ready_rx.recv();

        // 打开浏览器
        if no_browser {
            println!("{}", "请复制以下 URL 到浏览器中打开:".yellow());
            println!("{}", auth_url.cyan().underline());
        } else {
            println!("{}", "正在打开浏览器...".blue());
            let _ = webbrowser::open(&auth_url);
        }

        println!("{}", "⏳ 等待授权回调...".blue());

        let callback = rx.recv()
            .map_err(|e| Error::HttpServerError(format!("receive callback: {}", e)))?;

        server_handle.abort();

        // 换取 token
        let token_data = self.client.exchange_code_for_token(&self.config_store, &callback.code)
            .await
            .map_err(|e| Error::AuthFailed(e.to_string()))?;

        self.token_store.save(&token_data).await
            .map_err(|e| Error::StorageError(e.to_string()))?;

        println!();
        println!("{}", "✅ 登录成功！".green());
        println!("访问令牌有效期: 约 6.5 小时");
        println!("刷新令牌有效期: 约 30 天");
        Ok(())
    }

    pub async fn logout(&self) -> std::result::Result<(), Error> {
        self.token_store.clear()
            .await
            .map_err(|e| Error::StorageError(e.to_string()))?;
        println!("{}", "✅ 已注销".green());
        Ok(())
    }

    async fn start_callback_server(
        expected_state: String,
        tx: Arc<TokioMutex<std::sync::mpsc::Sender<OAuthCallback>>>,
        ready_tx: std::sync::mpsc::SyncSender<()>,
    ) -> std::result::Result<(), Error> {
        use std::net::SocketAddrV4;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let addr: SocketAddrV4 = "127.0.0.1:8765".parse()
            .map_err(|e| Error::AddrParseError(e))?;
        let listener = TcpListener::bind(&addr).await
            .map_err(|e| Error::IoError(e))?;

        let _ = ready_tx.send(());

        let (mut stream, _) = listener.accept().await
            .map_err(|e| Error::IoError(e))?;

        let mut buf = [0u8; 8192];
        let n = stream.read(&mut buf).await.unwrap_or(0);
        let request = String::from_utf8_lossy(&buf[..n]).to_string();

        if let Some(qs_start) = request.find("/callback?") {
            let qs_end = request[qs_start..].find(' ').map(|i| qs_start + i);
            if let Some(end) = qs_end {
                let query = &request[qs_start + 10..end];
                let params = url::form_urlencoded::parse(query.as_bytes());
                let mut code: Option<String> = None;
                let mut state: Option<String> = None;
                for (k, v) in params {
                    if k == "code" { code = Some(v.to_string()); }
                    if k == "state" { state = Some(v.to_string()); }
                }

                if let (Some(c), Some(s)) = (code, state) {
                    if s != expected_state {
                        let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\nState mismatch").await;
                        return Ok(());
                    }

                    let html = r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>授权成功</title></head>
<body style="font-family:sans-serif;text-align:center;padding-top:80px;background:#f8f8f8">
<h1 style="color:#52c41a">✅ 授权成功</h1><p>您可以关闭此页面并返回终端。</p></body></html>"#;

                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
                        html.len(), html
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                    let _ = tx.lock().await.send(OAuthCallback { code: c, state: s });
                }
            }
        }

        Ok(())
    }
}
