---
name: feishu-export
description: 飞书知识库文档批量导出 CLI 工具 - 多智能体协作指南
version: 0.1.0
last_updated: 2026-04-12
---

# feishu-export 项目 SKILL

> **本文档专为多智能体协作设计**，提供项目架构、模块职责、接口契约和开发约定的完整描述。

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

## 多智能体协作指南

### 智能体角色分工

| 角色 | 职责范围 | 负责模块 |
|------|---------|---------|
| **CLI 智能体** | 命令行解析、用户交互、进度展示 | `src/main.rs`, `src/cmd/*` |
| **API 智能体** | 飞书 API 封装、认证、请求调度 | `crates/feishu-core/src/api/*` |
| **引擎智能体** | 导出任务调度、并发控制、状态管理 | `crates/feishu-core/src/engine/*` |
| **存储智能体** | 配置文件、Token、缓存持久化 | `crates/feishu-core/src/storage/*` |
| **转换智能体** | 文档格式转换（docx → md） | `crates/doc-converter/*` |
| **模型智能体** | 数据结构定义、序列化/反序列化 | `crates/feishu-core/src/models/*` |

### 任务分解模式

当接收到复杂任务时，按以下流程分解：

```
用户需求 → CLI 解析 → 任务路由 → 并行执行 → 结果聚合
    ↓          ↓          ↓           ↓           ↓
 export     识别命令   分发到     各模块      汇总进度
 命令       和参数     对应模块    协同工作    输出报告
```

### 模块间接口契约

#### 1. CLI → Core 接口

```rust
// 所有核心操作通过 FeishuClient 暴露
pub struct FeishuClient {
    pub auth: AuthManager,
    pub wiki: WikiApi,
    pub export: ExportApi,
}

// 统一错误类型
pub enum FeishuError {
    ApiError { code: i32, msg: String },
    AuthError(String),
    NetworkError(reqwest::Error),
    IoError(std::io::Error),
    ExportError(String),
    ConversionError(String),
}
```

#### 2. Engine → API 接口

```rust
// 导出引擎使用 trait 抽象，便于测试和替换
pub trait ExportEngine {
    async fn export_node(&self, node: &WikiNode, format: ExportFormat) -> Result<ExportResult>;
    async fn export_batch(&self, nodes: Vec<WikiNode>, format: ExportFormat, concurrency: usize) -> Result<BatchResult>;
}
```

#### 3. Storage 接口

```rust
// 所有存储操作实现统一 trait
pub trait Storage {
    fn load_config(&self) -> Result<AppConfig>;
    fn save_config(&self, config: &AppConfig) -> Result<()>;
    fn load_token(&self) -> Result<TokenData>;
    fn save_token(&self, token: &TokenData) -> Result<()>;
    fn load_progress(&self, space_id: &str) -> Result<ExportProgress>;
    fn save_progress(&self, space_id: &str, progress: &ExportProgress) -> Result<()>;
}
```

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

| 模块                   | 职责                                        | 智能体角色 |
| ---------------------- | ------------------------------------------- | ---------- |
| `feishu-core::api`     | 飞书 Open API 封装（OAuth、Wiki、导出任务） | API 智能体 |
| `feishu-core::models`  | WikiNode、ExportTask、TokenData 等模型      | 模型智能体 |
| `feishu-core::storage` | 配置文件、Token 存储管理                    | 存储智能体 |
| `feishu-core::engine`  | 导出任务调度、并发控制、进度跟踪            | 引擎智能体 |
| `doc-converter`        | 使用 docx crate 纯 Rust 实现 docx → md 转换 | 转换智能体 |
| `src/cmd/*`            | CLI 命令解析和用户交互                      | CLI 智能体 |

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

## 开发约定

### 代码风格

- **命名规范**: Rust 标准 snake_case for functions/variables, PascalCase for types
- **错误处理**: 使用 `Result<T, FeishuError>`，避免 unwrap()
- **异步编程**: 所有 IO 操作使用 async/await
- **日志输出**: 使用 `console` 和 `colored` 库保持输出一致性

### Git 提交规范

```
feat: 新功能
fix: 修复 bug
docs: 文档更新
style: 代码格式调整
refactor: 重构
test: 测试相关
chore: 构建/工具链相关
```

### 模块依赖规则

```
CLI (src/) → feishu-core (公开 API)
feishu-core::engine → feishu-core::api
feishu-core::api → feishu-core::models
feishu-core::storage → feishu-core::models
doc-converter → 独立，无依赖
```

### 智能体协作协议

1. **职责边界**: 每个智能体只修改自己负责的模块
2. **接口变更**: 修改公共接口前需在其他模块中验证兼容性
3. **错误传递**: 底层错误必须转换为 `FeishuError` 向上传递
4. **并发安全**: 共享状态使用 `Arc<Mutex<T>>` 或 `Arc<Semaphore>`
5. **测试覆盖**: 新增功能必须包含单元测试

---

## 关键数据模型

### WikiNode（文档树节点）

**负责智能体**: 模型智能体

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

**负责智能体**: 引擎智能体、模型智能体

```rust
pub struct ExportTask {
    pub node: WikiNode,
    pub format: ExportFormat,
    pub status: ExportStatus,
    pub progress: ExportProgress,
}
```

### TokenData（认证凭证）

**负责智能体**: 存储智能体、API 智能体

```rust
pub struct TokenData {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: DateTime<Utc>,
    pub user_id: Option<String>,
}
```

### AppConfig（应用配置）

**负责智能体**: 存储智能体、CLI 智能体

```rust
pub struct AppConfig {
    pub app_id: String,
    pub app_secret: String,
    pub redirect_uri: String,
    pub data_dir: PathBuf,
}
```

---

## 飞书 API 关键点

**负责智能体**: API 智能体

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

**负责智能体**: 转换智能体

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

**负责智能体**: 所有智能体（统一错误类型）

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

### 错误处理约定

1. **底层模块**: 将具体错误转换为 `FeishuError`
2. **中间层**: 使用 `?` 操作符向上传递错误
3. **CLI 层**: 捕获错误并格式化输出给用户

---

## 配置存储位置

**负责智能体**: 存储智能体

| 平台    | 配置目录                                       |
| ------- | ---------------------------------------------- |
| macOS   | `~/Library/Application Support/feishu-export/` |
| Linux   | `~/.config/feishu-export/`                     |
| Windows | `%APPDATA%\feishu-export\`                     |

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

| 文件 | 说明 | 主要读者 |
| ------------- | -------------------------------- | --------- |
| `SKILL.md` | 多智能体协作指南（本文档） | 所有智能体 |
| `设计文档.md` | 产品设计、数据模型、API 权限清单 | 模型智能体、API 智能体 |
| `技术方案.md` | 技术实现细节、架构设计 | 引擎智能体、API 智能体 |
| `README.md` | 用户使用指南 | CLI 智能体 |

---

## 快速参考卡片

### 智能体任务分发表

| 任务类型 | 主导智能体 | 协作智能体 |
|---------|-----------|-----------|
| 新增 CLI 命令 | CLI 智能体 | 引擎智能体、存储智能体 |
| 新增 API 接口 | API 智能体 | 模型智能体 |
| 优化导出性能 | 引擎智能体 | API 智能体 |
| 修改数据结构 | 模型智能体 | 所有智能体 |
| 修复认证问题 | API 智能体 | 存储智能体 |
| 格式转换优化 | 转换智能体 | - |

### 关键路径

```
export 命令执行路径:
CLI 智能体 → 引擎智能体 → API 智能体 → 存储智能体
                ↓
           转换智能体 (可选)
```
