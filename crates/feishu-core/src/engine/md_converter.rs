//! Markdown converter — pure Rust, no external tools required.

use crate::error::{FeishuCoreError as Error, Result};
use std::path::Path;

/// Convert docx bytes or file to Markdown using the `doc-converter` crate.
pub struct MdConverter;

impl MdConverter {
    /// Convert a docx file to Markdown.
    pub fn docx_to_md(docx_path: &Path, md_path: &Path) -> Result<()> {
        let md = doc_converter::Converter::new()
            .convert_file(docx_path)
            .map_err(|e| Error::ConversionError(e.to_string()))?;

        std::fs::write(md_path, &md)
            .map_err(Error::IoError)?;

        Ok(())
    }

    /// Convert docx bytes (e.g. from HTTP download) to Markdown string.
    pub fn docx_bytes_to_md(bytes: impl Into<Vec<u8>>) -> Result<String> {
        doc_converter::Converter::new()
            .convert_bytes(bytes)
            .map_err(|e| Error::ConversionError(e.to_string()))
    }
}
