use feishu_core::api::FeishuClient;
use feishu_core::models::export::ExportFormat;
use feishu_core::storage::ConfigStore;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 加载配置
    let config_store = ConfigStore::new();
    let config = config_store.load().await?;

    // 加载 token
    let token_store = feishu_core::storage::TokenStore::new();
    let token_data = token_store.load().await?;
    let access_token = &token_data.access_token;

    // 创建客户端
    let client = FeishuClient::new();

    // 测试导出 bitable
    let obj_token = "bascnMAprEkYytJNwvnmLJqhuST";
    let obj_type = "bitable";
    let format = ExportFormat::Xlsx;

    println!("开始导出测试...");
    println!("对象 token: {}", obj_token);
    println!("对象类型: {}", obj_type);
    println!("导出格式: {:?}", format);

    // 步骤 1: 创建导出任务
    let ticket = client.create_export_task(access_token, obj_token, obj_type, format).await?;
    println!("✅ 导出任务创建成功，ticket: {}", ticket);

    // 步骤 2: 轮询导出状态
    let file_token = client.poll_export_task(access_token, &ticket, obj_token).await?;
    println!("✅ 导出任务完成，file_token: {}", file_token);

    // 步骤 3: 下载文件
    let response = client.download_export_file(access_token, &file_token).await?;
    println!("✅ 文件下载成功");

    // 保存文件
    let bytes = response.bytes().await?;
    let output_path = format!("/tmp/test_bitable_export.xlsx");
    tokio::fs::write(&output_path, &bytes).await?;
    println!("✅ 文件已保存到: {}", output_path);
    println!("文件大小: {} bytes", bytes.len());

    Ok(())
}
