//! Markdown converter — thin wrapper around the `doc-md` crate

use crate::error::{FeishuCoreError as Error, Result};
use std::path::Path;
use std::process::Command;

/// Convert docx to markdown via pandoc
pub struct MdConverter;

impl MdConverter {
    /// Convert a docx file to markdown using pandoc
    ///
    /// Requires pandoc to be installed: https://pandoc.org/installing.html
    pub fn docx_to_md(docx_path: &Path, md_path: &Path) -> Result<()> {
        if !Self::check_pandoc() {
            return Err(Error::ConversionError(
                "pandoc not found. Please install from https://pandoc.org/installing.html".to_string(),
            ));
        }

        let output = Command::new("pandoc")
            .arg(docx_path)
            .arg("-f").arg("docx")
            .arg("-t").arg("markdown")
            .arg("-o").arg(md_path)
            .output()
            .map_err(|e| Error::ConversionError(format!("failed to run pandoc: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::ConversionError(format!("pandoc conversion failed: {}", stderr)));
        }

        Ok(())
    }

    fn check_pandoc() -> bool {
        Command::new("pandoc")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}
