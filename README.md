# feishu-export

> 飞书知识库文档批量导出工具，基于 Rust 构建，支持 docx / pdf / md / xlsx 等格式。

## 功能特性

| 功能 | 说明 |
|------|------|
| 🔐 **OAuth 授权** | 浏览器扫码登录，自动管理 Token |
| 📂 **文档树浏览** | 列出知识空间、查看完整目录结构 |
| 📦 **批量导出** | 支持 docx / pdf / md / xlsx / csv 格式 |
| 🗂 **保留结构** | 按原始目录结构打包为 zip 文件 |
| ⚡ **并发下载** | 可调节并发数（默认 5），支持限流控制 |
| 🔄 **断点续导** | 中断后可继续，已下载文件自动跳过 |
| 📝 **格式转换** | docx 本地转换为 Markdown（无需登录） |
| 🎯 **智能降级** | 格式不支持时自动降级（如 sheet 转 xlsx） |

---

## 前置准备

### 1. 创建飞书自建应用

1. 访问 [飞书开发者后台](https://open.feishu.cn/app)
2. 点击「创建企业自建应用」
3. 填写应用名称和描述，创建完成

### 2. 配置应用权限

在应用详情页 → **权限管理** → 开通以下权限：

| 权限名称 | 权限标识 | 用途 |
|----------|----------|------|
| 获取知识库空间列表 | `wiki:space:readonly` | 列出知识空间 |
| 获取知识库节点 | `wiki:node:readonly` | 浏览文档树 |
| 获取文档内容 | `wiki:node:retrieve` | 获取文档详情 |
| 获取空间信息 | `wiki:wiki:readonly` | 获取空间详情 |
| 获取文件元信息 | `drive:drive:readonly` | 获取文件信息 |
| 导出文件 | `drive:export:export:readonly` | 批量导出文档 |
| 获取文档内容（新版） | `docx:document:readonly` | 读取文档内容 |
| 识别用户身份 | `contact:user.base:readonly` | 获取用户信息 |

> ⚠️ **重要**：应用权限开通后需要「发布」才能生效，请参考飞书文档进行版本发布。

### 3. 添加应用到知识库

导出他人文档时，需要将应用添加为知识库管理员：

1. 打开目标知识空间
2. 点击「···」→「空间设置」
3. 「管理员」→ 添加应用

---

## 安装

### 从源码编译

```bash
# 克隆项目
git clone https://github.com/xxx/feishu-export.git
cd feishu-export

# 编译（需要 Rust 环境）
cargo build --release

# 二进制文件位置: target/release/feishu-export
```

### 下载预编译版本

前往 [Releases](https://github.com/xxx/feishu-export/releases) 下载对应平台的二进制文件。

---

## 快速开始

### 1. 配置应用凭证

```bash
# 方式一：交互式配置
feishu-export config

# 方式二：命令行参数
feishu-export config set --app-id <YOUR_APP_ID> --app-secret <YOUR_APP_SECRET>

# 查看当前配置
feishu-export config show
```

### 2. 授权登录

```bash
feishu-export login
```

浏览器会自动打开飞书授权页面，扫码确认后自动完成登录。

```
🚀 正在启动本地服务器...
📱 请在浏览器中完成授权
🔐 授权地址: https://accounts.feishu.cn/open-apis/authen/v1/authorize?...
✅ 授权成功！Token 已保存。
```

### 3. 列出知识空间

```bash
feishu-export spaces list
```

输出示例：
```
📚 您的知识空间：

  [1] 沟通中的项目          (ID: <YOUR_SPACE_ID>)
  [2] 飞客航旅              (ID: <YOUR_SPACE_ID>)
  [3] 模板                  (ID: <YOUR_SPACE_ID>)
  [4] 客户项目              (ID: <YOUR_SPACE_ID>)

请输入空间 ID 进行导出...
```

### 4. 查看文档树

```bash
# 查看完整目录结构
feishu-export spaces tree <YOUR_SPACE_ID>
```

```
📁 沟通中的项目/
├── 📄 产品需求文档.docx
├── 📊 数据分析/
│   ├── 📄 周报模板.xlsx
│   └── 📄 KPI 报表.xlsx
├── 📝 会议纪要/
│   └── 📄 2024-01-15 周会.md
└── 📑 项目方案.pdf
```

### 5. 导出文档

```bash
# 导出整个知识空间（默认导出为 xlsx，支持 sheet/bitable）
feishu-export export <YOUR_SPACE_ID>

# 导出为 Markdown（仅 docx/doc 支持）
feishu-export export <YOUR_SPACE_ID> --format md

# 导出为 PDF（仅 docx/doc 支持）
feishu-export export <YOUR_SPACE_ID> --format pdf

# 导出为 CSV（仅 sheet/bitable 支持）
feishu-export export <YOUR_SPACE_ID> --format csv

# 导出为 XLSX（电子表格/多维表格推荐格式）
feishu-export export <YOUR_SPACE_ID> --format xlsx

# 指定输出目录
feishu-export export <YOUR_SPACE_ID> --output ~/Downloads/feishu-exports

# 调节并发数（加快下载速度）
feishu-export export <YOUR_SPACE_ID> --concurrency 10

# 断点续导（跳过已下载文件）
feishu-export export <YOUR_SPACE_ID> --resume
```

导出进度示例：
```
📋 总文档数: 183
⏳ 开始下载...

░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ [5  %] 10/183 | ✅ 产品需求文档
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ [6  %] 11/183 | ✅ 周报模板
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ [6  %] 12/183 | ⚠️ 思维导图 (暂不支持)
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ [7  %] 13/183 | ❌ 权限不足

⏱️  已用: 2分15秒 | 预计剩余: 5分30秒

✅ 导出完成！
📦 文件位置: ~/Downloads/feishu-exports/沟通中的项目.zip
📊 统计: 成功 178 | 失败 3 | 跳过 2
```

---

## 离线转换工具

无需登录，可将本地 docx 文件转换为 Markdown：

```bash
# 单个文件转换
feishu-export convert document.docx

# 指定输出文件
feishu-export convert document.docx -o output.md

# 批量转换目录
feishu-export convert ./docs

# 含子目录递归
feishu-export convert ./docs -r

# 预览模式（不生成文件）
feishu-export convert document.docx --dry-run
```

支持的转换元素：
- ✅ 标题（H1-H6）
- ✅ 段落文本
- ✅ 粗体、斜体、下划线
- ✅ 有序/无序列表
- ✅ 表格
- ✅ 代码块（保留语法高亮标记）
- ✅ 内联图片（Base64 嵌入）
- ✅ 引用块

---

## 支持的文档类型

| 飞书类型 | 导出格式 | 说明 |
|----------|----------|------|
| 新版文档 (docx) | docx / pdf / md | 完整支持 |
| 旧版文档 (doc) | docx / pdf | 完整支持 |
| 电子表格 (sheet) | xlsx / csv | **仅支持 xlsx/csv**，自动格式转换 |
| 多维表格 (bitable) | xlsx / csv | **仅支持 xlsx/csv**，自动格式转换 |
| 文件 (file) | 原样下载 | .xlsx/.docx 等直接下载 |
| 思维导图 (mindnote) | - | 暂不支持 API 导出 |
| 幻灯片 (slides) | - | 暂不支持 API 导出 |

> ⚠️ **注意**：电子表格和多维表格**不支持**导出为 docx / pdf / md，飞书 API 仅支持 xlsx 和 csv 两种格式。工具会自动按 xlsx 导出（`--format xlsx` 为默认值），如需 CSV 请使用 `--format csv`。

### 格式自动降级

当请求的格式不被支持时，工具会自动降级：

| 请求格式 | 实际格式 | 说明 |
|----------|----------|------|
| docx / pdf / md | xlsx | 电子表格/多维表格 |
| docx | pdf | 格式不兼容的文档 |
| md | pdf | 无法转换为 Markdown |

---

## 错误处理

### 常见错误码

| 错误码 | 说明 | 解决方案 |
|--------|------|----------|
| `1069918` | 文件扩展名与类型不匹配 | 自动降级到兼容格式 |
| `99992402` | API 参数验证失败 | 检查 App 权限配置 |
| `131006` | 权限不足 | 添加应用为知识库管理员 |
| `230001` | 文档不存在或已删除 | 跳过该文档 |
| `99991663` | Token 过期 | 运行 `feishu-export login` 重新授权 |

### 网络错误

```
Network error: error decoding response body
```

可能原因：
- 网络连接不稳定
- 飞书 API 服务器暂时不可用
- 下载链接过期

解决方案：使用 `--resume` 参数重试，已下载的文件会被跳过。

---

## 配置说明

### 配置文件位置

| 平台 | 路径 |
|------|------|
| macOS | `~/Library/Application Support/feishu-export/` |
| Linux | `~/.config/feishu-export/` |
| Windows | `%APPDATA%\feishu-export\` |

### 存储文件

| 文件 | 内容 |
|------|------|
| `config.toml` | App ID / App Secret |
| `token.json` | OAuth Token（自动刷新） |
| `progress/` | 导出进度（断点续导用） |

### 管理命令

```bash
# 查看配置
feishu-export config show

# 修改配置
feishu-export config set --app-id <ID> --app-secret <SECRET>

# 清除配置（退出登录）
feishu-export config clear

# 清除 Token（重新登录）
feishu-export logout
```

---

## 高级用法

### 仅导出特定节点

```bash
# 只导出某个子文件夹
feishu-export export <space_id> --node <node_token>
```

### 排除特定文档

```bash
# 排除不需要的节点（多个用逗号分隔）
feishu-export export <space_id> --exclude token1,token2,token3
```

### 静默模式

```bash
# 减少输出，只显示进度条
feishu-export export <space_id> --quiet
```

### 查看帮助

```bash
# 全局帮助
feishu-export --help

# 子命令帮助
feishu-export export --help
feishu-export convert --help
feishu-export config --help
```

---

## 常见问题

### Q: 授权失败，提示 App 权限不足？

**A**: 
1. 确认已在飞书开发者后台开通所有必需权限
2. 确认应用已「发布」（权限修改后需要重新发布版本）
3. 确认应用已被添加为目标知识空间的管理员

### Q: 部分文档导出失败，显示「权限不足」？

**A**: 这是因为当前登录用户不是这些文档的协作者。请联系文档所有者，将应用添加为知识库管理员。

### Q: Token 过期需要重新登录？

**A**: 工具会自动刷新 Token。如果提示 Token 失效，运行 `feishu-export login` 重新授权。

### Q: 导出速度太慢？

**A**: 
1. 增加并发数：`--concurrency 10`（不建议超过 15）
2. 使用断点续导：`--resume`，跳过已下载的文件
3. 排除不需要的文件夹：`--exclude <tokens>`

### Q: 如何导出整个企业空间？

**A**: 使用根空间 ID 导出所有内容，或使用 `--include` 参数指定多个节点。

---

## 技术栈

- **语言**: Rust
- **HTTP 客户端**: reqwest
- **异步运行时**: tokio
- **CLI 框架**: clap
- **进度条**: indicatif
- **docx 解析**: docx crate（纯 Rust）
- **zip 打包**: zip crate

---

## License

MIT License
