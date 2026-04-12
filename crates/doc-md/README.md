# doc-md

Pure Rust docx → Markdown converter powered by [pandoc](https://pandoc.org).

## Why

Most Rust docx-to-markdown solutions are either incomplete or require building a full DOM parser from scratch.
This crate delegates the heavy lifting to pandoc, keeping the implementation small and correct.

## Features

- **Sync** and **async** (`tokio`) conversion
- Configurable markdown flavor: standard, GFM, CommonMark, MultiMarkdown
- Convert to file or to `String`
- Proper error types (`thiserror`)

## Quick Start

```toml
# Cargo.toml
[dependencies]
doc-md = { version = "0.1", features = ["async"] }
```

```rust
use doc_md::{Converter, Config, MarkdownFlavor};

// Sync
let conv = Converter::new();
conv.convert("input.docx", "output.md")?;

// Async (feature = "async")
let conv = doc_md::AsyncConverter::new();
conv.convert("input.docx", "output.md").await?;
```

## Feature Flags

| Flag | Description |
|------|-------------|
| `async` | Enable async conversion via tokio |

## Requirements

- [pandoc](https://pandoc.org/installing.html) must be installed and in `PATH`.
- Use `Converter::check_pandoc()` to verify availability at runtime.

## License

MIT
