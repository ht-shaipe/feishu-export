use crate::error::{FeishuError, Result};
use std::path::Path;
use std::process::Command;

/// Markdown 转换器（thin wrapper around `doc-md`）
///
/// 飞书不直接支持导出 MD，使用 pandoc 将 docx 转换为 md
pub struct MdConverter;

impl MdConverter {
    /// 将 docx 文件转换为 Markdown
    ///
    /// 需要系统安装 pandoc: https://pandoc.org/installing.html
    pub fn docx_to_md(docx_path: &Path, md_path: &Path) -> Result<()> {
        if !Self::check_pandoc() {
            return Err(FeishuError::ConversionError(
                "pandoc not found. Please install pandoc from https://pandoc.org/installing.html"
                    .to_string(),
            ));
        }

        let output = Command::new("pandoc")
            .arg(docx_path)
            .arg("-f")
            .arg("docx")
            .arg("-t")
            .arg("markdown")
            .arg("-o")
            .arg(md_path)
            .output()
            .map_err(|e| {
                FeishuError::ConversionError(format!("Failed to run pandoc: {}", e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(FeishuError::ConversionError(format!(
                "pandoc conversion failed: {}",
                stderr
            )));
        }

        Ok(())
    }

    /// 检查 pandoc 是否可用
    fn check_pandoc() -> bool {
        Command::new("pandoc")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}
