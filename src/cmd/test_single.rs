use colored::Colorize;
use feishu_core::error::FeishuCoreError as Error;
use feishu_core::models::export::ExportFormat;
use feishu_core::storage::{ConfigStore, TokenStore};
use feishu_core::{FeishuClient};

type CmdResult = std::result::Result<(), Error>;

/// 测试单个文档导出
pub async fn run(obj_token: &str, obj_type: &str, format: &str) -> CmdResult {
    let config_store = ConfigStore::new();
    let token_store = TokenStore::new();

    println!("{}", "=== 单个文档导出测试 ===".bright_blue());
    println!("对象 token: {}", obj_token.bright_cyan());
    println!("对象类型: {}", obj_type.bright_cyan());
    println!("导出格式: {}", format.bright_cyan());
    println!();

    // 加载 token
    let token = token_store.load().await?;
    let access_token = &token.access_token;

    // 创建客户端
    let client = FeishuClient::new();

    // 解析格式
    let export_format = ExportFormat::from_str(format).ok_or_else(|| {
        Error::ConfigError(format!(
            "Unsupported export format: {}. Supported: auto, docx, pdf, md, xlsx, csv",
            format
        ))
    })?;

    // 导出文档
    println!("{}", "开始导出...".bright_yellow());
    match client
        .export_document(access_token, obj_token, obj_type, export_format)
        .await
    {
        Ok(response) => {
            println!("{}", "✅ 导出成功！".bright_green());
            println!("  文件 token: {}", response.headers().get("content-disposition").and_then(|v| v.to_str().ok()).unwrap_or("N/A"));

            // 保存文件
            let bytes = response.bytes().await.map_err(Error::NetworkError)?;
            let output_path = format!("/tmp/{}_export.{}", obj_token, format);
            std::fs::write(&output_path, &bytes).map_err(Error::IoError)?;
            println!("  文件已保存到: {}", output_path.bright_green());
            println!("  文件大小: {} bytes", bytes.len().bright_green());
        }
        Err(e) => {
            println!("{}", "❌ 导出失败！".bright_red());
            println!("  错误: {}", e.to_string().bright_red());
            println!("  错误类型: {:?}", std::error::Error::source(&e));
            return Err(e);
        }
    }

    Ok(())
}
