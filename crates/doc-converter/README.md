# doc-converter

纯 Rust docx → Markdown 转换器，无外部依赖。

## 实现方式

使用纯 Rust 生态库解析 `.docx` 文件结构：
- `zip` — 解压 OOXML 包
- `docx` — 解析 Word 文档 XML 结构（段落、表格、图片等）
- `regex` — 提取纯文本回退（应对非标准 XML）
- `base64` — 图片 Base64 编码嵌入 Markdown

## 功能特性

- 纯 Rust，无须安装 pandoc 等外部工具
- 支持离线转换（无需网络）
- 图片自动提取并转为 Base64 data URI 内嵌 Markdown
- 优雅降级：XML 解析失败时回退到纯文本提取

## 支持的转换元素

| 元素 | 支持状态 |
|------|---------|
| 段落文本 | ✅ |
| 标题（H1-H6） | ✅ |
| 粗体、斜体、下划线 | ✅ |
| 有序/无序列表 | ✅ |
| 表格 | ✅ |
| 代码块 | ✅ |
| 内联图片 | ✅ Base64 嵌入 |
| 引用块 | ✅ |
| 脚注、尾注 | ⚠️ 部分支持 |

## 快速开始

```rust
use doc_converter::Converter;

let conv = Converter::new();
conv.convert("input.docx", "output.md")?;
```

## 依赖说明

本 crate 是 feishu-export 项目的一部分，不作为独立 crate 分发。
