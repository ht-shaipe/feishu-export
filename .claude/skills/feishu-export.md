---
name: feishu-export
description: 飞书知识库文档批量导出 CLI 工具 - 快速上手指南
---

# feishu-export 项目 SKILL

## 项目概述

**feishu-export** 是一个基于 Rust 构建的飞书知识库文档批量导出 CLI 工具。

### 核心功能
- 🔐 OAuth 2.0 用户授权登录
- 📂 浏览知识空间列表 & 文档树
- 📦 批量导出文档（支持 docx / pdf / md / xlsx / csv）
- 🗂 保留原始目录结构，打包为 .zip
- ⚡ 并发导出 + 限流控制
- 🔄 断点续导（支持中断后继续）
- 📝 docx 转 Markdown（本地转换，无需登录）

---

## 项目架构

### Workspace 结构

```
feishu-export/
├── Cargo.toml                 # workspace 根配置
├── src/                       # CLI 入口
│   ├── main.rs               # clap 命令定义
│   └── cmd/                  # 命令处理器
│       ├── config.rs         # 配置管理
│       ├── login.rs          # OAuth 登录
│       ├── spaces.rs         # 知识空间操作
│       ├── export.rs         # 导出命令
│       └── convert.rs        # 文档转换命令
└── crates/
    ├── feishu-core/          # 核心 API 客户端
    │   └── src/
    │       ├── api.rs        # 飞书 API 封装
    │       ├── models/       # 数据模型
    │       ├── storage/      # 本地存储
    │       ├── engine/       # 导出引擎
    │       └── error.rs      # 错误定义
    └── doc-converter/        # docx → md 转换
        └── src/
            └── lib.rs        # 纯 Rust docx 解析
```

### 核心模块说明

| 模块 | 职责 |
|------|------|
| `feishu-core::api` | 飞书 Open API 封装（OAuth、Wiki、导出任务） |
| `feishu-core::models` | WikiNode、ExportTask、TokenData 等模型 |
| `feishu-core::storage` | 配置文件、Token 存储管理 |
| `feishu-core::engine` | 导出任务调度、并发控制、进度跟踪 |
| `doc-converter` | 使用 docx crate 纯 Rust 实现 docx → md 转换 |

---

## 常用命令

### 开发构建

```bash
# 开发构建
cargo build

# 发布构建（优化二进制体积）
cargo build --release

# 运行
cargo run -- --help
```

### 测试

```bash
# 运行所有测试
cargo test

# 运行特定 crate 测试
cargo test -p feishu-core
cargo test -p doc-converter

# 带输出的测试
cargo test -- --nocapture
```

### 功能使用

```bash
# 配置 App 凭证
cargo run -- config set --app-id <APP_ID> --app-secret <APP_SECRET>

# 授权登录
cargo run -- login

# 列出知识空间
cargo run -- spaces list

# 查看文档树
cargo run -- spaces tree <space_id>

# 导出知识空间
cargo run -- export <space_id> --format md --output ~/exports

# 单个 docx 转 md
cargo run -- convert file.docx

# 批量转换目录
cargo run -- convert ./docs -r
```

---

## 关键数据模型

### WikiNode（文档树节点）

```rust
pub struct WikiNode {
    pub space_id: String,
    pub node_token: String,     // 知识库节点 token
    pub obj_token: String,      // 实际文档 token（用于导出）
    pub obj_type: ObjType,      // Docx/Doc/Sheet/Bitable/File
    pub node_type: NodeType,    // origin | shortcut
    pub title: String,
    pub has_child: bool,
    pub parent_node_token: Option<String>,
    pub depth: u32,
}
```

### ExportTask（导出任务）

```rust
pub struct ExportTask {
    pub node: WikiNode,
    pub format: ExportFormat,
    pub status: ExportStatus,
    pub progress: ExportProgress,
}
```

---

## 飞书 API 关键点

### 1. OAuth 授权流程

```
用户 → CLI: feishu-export login
CLI → 飞书: 获取授权 URL（包含 state 防 CSRF）
CLI → 用户: 打开浏览器扫码
飞书 → CLI: callback 回调（code + state）
CLI → 飞书: 用 code 换 token
CLI → 本地: 存储 access_token + refresh_token
```

### 2. 导出任务流程

```
1. 获取文档树（wiki:GetNodeTree）
2. 创建导出任务（drive:export:CreateExportTask）
3. 轮询任务状态（drive:export:GetExportTask）
4. 下载导出文件
5. 格式转换（如需要）
6. 打包为 zip
```

### 3. 断点续导机制

- 导出进度持久化到本地文件（`<data_dir>/progress/<space_id>.json`）
- 支持 `--resume` 参数从中断位置继续
- 已完成的文档不会重复下载

---

## doc-converter 转换器

**特点**: 纯 Rust 实现，无需 pandoc 等外部依赖

```rust
// 核心转换函数
pub fn convert_docx_to_md(input: &Path, output: &Path) -> Result<(), Error>

// 支持的元素
- 段落文本
- 标题（Heading 1-6）
- 粗体/斜体
- 列表（有序/无序）
- 内联图片（Base64 编码到 md）
- 表格（保留基本结构）
```

---

## 错误处理

```rust
// feishu-core/error.rs
pub enum FeishuError {
    ApiError { code: i32, msg: String },      // 飞书 API 错误
    AuthError(String),                         // 授权失败
    NetworkError(reqwest::Error),              // 网络错误
    IoError(std::io::Error),                   // IO 错误
    ExportError(String),                       // 导出失败
    ConversionError(String),                   // 转换失败
}
```

---

## 配置存储位置

| 平台 | 配置目录 |
|------|----------|
| macOS | `~/Library/Application Support/feishu-export/` |
| Linux | `~/.config/feishu-export/` |
| Windows | `%APPDATA%\feishu-export\` |

### 存储文件

- `config.toml` - App 配置
- `token.json` - OAuth Token
- `progress/<space_id>.json` - 导出进度

---

## 注意事项

1. **权限要求**: 应用需添加为知识库管理员才能导出全部文档
2. **速率限制**: 飞书 API 有 QPS 限制，默认并发数为 5
3. **Token 过期**: access_token 约 6.5 小时过期，会自动刷新
4. **不支持**: 思维导图 (mindnote) 和幻灯片 (slides) 暂不支持 API 导出

---

## 相关文件

| 文件 | 说明 |
|------|------|
| `设计文档.md` | 产品设计、数据模型、API 权限清单 |
| `技术方案.md` | 技术实现细节 |
| `README.md` | 用户使用指南 |
