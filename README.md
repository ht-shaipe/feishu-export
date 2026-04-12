# feishu-export

> 飞书知识库文档批量导出 CLI 工具，基于 Rust 构建。

## 功能

- 🔐 OAuth 2.0 用户授权登录
- 📂 浏览知识空间列表 & 文档树
- 📦 批量导出文档（支持 docx / pdf / md / xlsx / csv）
- 🗂 保留原始目录结构，打包为 .zip
- ⚡ 并发导出 + 限流控制
- 🔄 断点续导（支持中断后继续）

## 快速开始

### 安装

```bash
cargo install feishu-export
```

或从 [Releases](https://github.com/xxx/feishu-export/releases) 下载预编译二进制。

### 配置

首次使用需配置飞书自建应用凭证：

```bash
feishu-export config --app-id <APP_ID> --app-secret <APP_SECRET>
```

### 授权

```bash
feishu-export login
# 浏览器自动打开飞书授权页，扫码后自动完成
```

### 使用

```bash
# 列出所有知识空间
feishu-export spaces list

# 查看某知识空间的文档树
feishu-export spaces tree <space_id>

# 导出整个知识空间（默认 docx 格式）
feishu-export export <space_id>

# 导出为 Markdown
feishu-export export <space_id> --format md

# 导出为 PDF
feishu-export export <space_id> --format pdf

# 仅导出指定节点
feishu-export export <space_id> --node <node_token>

# 指定导出目录和并发数
feishu-export export <space_id> --output ~/exports --concurrency 8

# 断点续导
feishu-export export <space_id> --resume
```

## 前置条件

- 飞书企业版用户
- 在[飞书开发者后台](https://open.feishu.cn/app)创建企业自建应用
- 开通所需 API 权限（详见技术方案文档）
- 将应用添加为知识库管理员

## 支持的文档类型

| 飞书文档类型 | obj_type | 可导出格式 |
|-------------|----------|-----------|
| 新版文档 | docx | docx, pdf, md |
| 旧版文档 | doc | docx, pdf, md |
| 电子表格 | sheet | xlsx, csv |
| 多维表格 | bitable | xlsx, csv |
| 文件 | file | 原样下载 |
| 思维导图 | mindnote | ⚠️ 暂不支持 API 导出 |
| 幻灯片 | slides | ⚠️ 暂不支持 API 导出 |

## License

MIT
