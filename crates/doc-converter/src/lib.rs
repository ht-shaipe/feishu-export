//! Pure Rust docx → Markdown converter (no external tools).

use docx::document::{
    BodyContent, Document, Paragraph, ParagraphContent, RunContent, Table,
    TableCellContent,
};
use docx::formatting::{CharacterProperty, ParagraphStyleId};
use docx::DocxFile;
use std::path::{Path, PathBuf};
use thiserror::Error;

// ─────────────────────────────────────────────────────────────────────────────
// Error types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Error, Debug)]
pub enum Error {
    #[error("failed to open docx file: {0}")]
    OpenFile(#[from] std::io::Error),

    #[error("failed to parse docx: {0}")]
    Parse(String),

    #[error("unsupported extension: {0} (only .docx is supported)")]
    UnsupportedExtension(String),

    #[error("output file already exists: {0}")]
    OutputExists(PathBuf),
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Markdown converter.
#[derive(Debug, Clone, Default)]
pub struct Converter {
    /// Allow overwriting existing output files.
    pub overwrite: bool,
}

impl Converter {
    pub fn new() -> Self {
        Self { overwrite: false }
    }

    /// Enable overwriting output files.
    pub fn overwrite(mut self) -> Self {
        self.overwrite = true;
        self
    }

    /// Convert a `.docx` file to a Markdown string.
    pub fn convert_file<P: AsRef<Path>>(&self, path: P) -> Result<String, Error> {
        let path = path.as_ref();
        self.validate_input(path)?;

        let docx_file = DocxFile::from_file(path)
            .map_err(Self::docx_err_to_string)?;
        let docx = docx_file
            .parse()
            .map_err(Self::docx_err_to_string)?;

        Ok(self.document_to_md(&docx.document))
    }

    /// Convert a `.docx` file and write to an output Markdown file.
    pub fn convert<P: AsRef<Path>>(&self, input: P, output: P) -> Result<PathBuf, Error> {
        let input = input.as_ref();
        let output = output.as_ref().to_path_buf();

        if output.exists() && !self.overwrite {
            return Err(Error::OutputExists(output));
        }

        let md = self.convert_file(input)?;
        std::fs::write(&output, &md).map_err(Error::OpenFile)?;
        Ok(output)
    }

    /// Convert docx bytes (e.g. from a network download) to Markdown.
    pub fn convert_bytes(&self, bytes: impl Into<Vec<u8>>) -> Result<String, Error> {
        let bytes = bytes.into();
        let cursor = std::io::Cursor::new(bytes);
        let docx_file = DocxFile::from_reader(cursor)
            .map_err(Self::docx_err_to_string)?;
        let docx = docx_file
            .parse()
            .map_err(Self::docx_err_to_string)?;
        Ok(self.document_to_md(&docx.document))
    }

    fn docx_err_to_string(e: impl std::fmt::Debug) -> Error {
        Error::Parse(format!("{:?}", e))
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Internals
    // ─────────────────────────────────────────────────────────────────────────

    fn validate_input(&self, path: &Path) -> Result<(), Error> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !ext.eq_ignore_ascii_case("docx") {
            return Err(Error::UnsupportedExtension(ext.to_string()));
        }
        Ok(())
    }

    fn document_to_md(&self, doc: &Document<'_>) -> String {
        let mut out = String::new();

        for content in &doc.body.content {
            match content {
                BodyContent::Paragraph(p) => self.append_paragraph(&mut out, p),
                BodyContent::Table(t) => self.append_table(&mut out, t),
            }
        }

        out.trim_end().to_string()
    }

    fn append_paragraph(&self, out: &mut String, para: &Paragraph<'_>) {
        let text = para.iter_text().map(|s| s.as_ref()).collect::<String>();

        if text.trim().is_empty() {
            if !out.ends_with('\n') {
                out.push('\n');
            }
            return;
        }

        // Heading from style ID (e.g. "Heading1", "Title")
        if let Some(sid) = self.style_id(para) {
            if let Some(heading) = self.heading_from_style(&sid, &text) {
                out.push_str(&heading);
                out.push_str("\n\n");
                return;
            }
        }

        // Content-based heading heuristics
        if let Some(heading) = self.detect_heading_from_content(&text) {
            out.push_str(&heading);
            out.push_str("\n\n");
            return;
        }

        // Regular paragraph
        let rendered = self.render_paragraph(para);
        out.push_str(&rendered);
        out.push('\n');
    }

    fn style_id<'a>(&self, para: &'a Paragraph) -> Option<String> {
        para.property
            .style_id
            .as_ref()
            .and_then(|sid: &'a ParagraphStyleId| {
                let v = sid.value.as_ref();
                if v.is_empty() { None } else { Some(v.to_string()) }
            })
    }

    fn heading_from_style(&self, style_id: &str, text: &str) -> Option<String> {
        let level = match style_id.to_ascii_lowercase().as_str() {
            "title" | "heading1" => 1,
            "heading2" => 2,
            "heading3" => 3,
            "heading4" => 4,
            "heading5" => 5,
            "heading6" => 6,
            _ => return None,
        };
        let hashes = "#".repeat(level);
        Some(format!("{} {}", hashes, text))
    }

    fn detect_heading_from_content(&self, text: &str) -> Option<String> {
        let trimmed = text.trim();
        // All caps, reasonably short → likely a title
        if trimmed.len() <= 80
            && trimmed.chars().filter(|c| c.is_ascii_alphabetic()).count() > 3
            && trimmed == trimmed.to_uppercase()
        {
            return Some(format!("# {}", trimmed));
        }
        // Numbered heading: "1. Topic"
        if trimmed.len() <= 120
            && (trimmed.starts_with("1.") || trimmed.starts_with("2.") || trimmed.starts_with("3."))
        {
            return Some(format!("## {}", trimmed));
        }
        None
    }

    /// Render all runs in a paragraph to a Markdown string.
    fn render_paragraph(&self, para: &Paragraph<'_>) -> String {
        let mut parts: Vec<String> = Vec::new();

        for content in &para.content {
            match content {
                ParagraphContent::Run(run) => {
                    let run_text = run
                        .content
                        .iter()
                        .filter_map(|c| match c {
                            RunContent::Text(t) => {
                                let s = t.text.as_ref();
                                if s.is_empty() { None } else { Some(s.to_string()) }
                            }
                            RunContent::Break(_) => Some(" ".to_string()),
                        })
                        .collect::<String>();

                    if run_text.is_empty() {
                        continue;
                    }

                    let rendered = Self::apply_inline(&run_text, &run.property);
                    parts.push(rendered);
                }
                ParagraphContent::Link(link) => {
                    // Hyperlink.content is a single Run
                    let run_text = link
                        .content
                        .content
                        .iter()
                        .filter_map(|c| match c {
                            RunContent::Text(t) => {
                                let s = t.text.as_ref();
                                if s.is_empty() { None } else { Some(s.to_string()) }
                            }
                            RunContent::Break(_) => Some(" ".to_string()),
                        })
                        .collect::<String>();

                    let rendered = Self::apply_inline(&run_text, &link.content.property);
                    parts.push(format!("[{}]({})", rendered.trim(), "#"));
                }
                ParagraphContent::BookmarkStart(_) | ParagraphContent::BookmarkEnd(_) => {}
            }
        }

        let joined = parts.join("").trim().to_string();
        if joined.is_empty() {
            String::new()
        } else {
            joined
        }
    }

    /// Apply inline formatting (bold, italic, strike, code).
    fn apply_inline(text: &str, props: &CharacterProperty<'_>) -> String {
        if text.is_empty() {
            return String::new();
        }

        let is_bold = props.bold.is_some();
        let is_italic = props.italics.is_some();
        let is_strike = props.strike.is_some();
        let is_code = props.style_id.as_ref().is_some_and(|s| {
            let v = s.value.as_ref().to_ascii_lowercase();
            v == "code" || v == "inlinecode"
        });

        let escaped = if is_code {
            text.to_string()
        } else {
            Self::escape_md(text)
        };

        if is_code {
            format!("`{}`", escaped.trim())
        } else {
            let s = escaped;
            let s = if is_bold { format!("**{}**", s) } else { s };
            let s = if is_italic { format!("_{}_", s) } else { s };
            let s = if is_strike { format!("~~{}~~", s) } else { s };
            s
        }
    }

    fn escape_md(text: &str) -> String {
        text.replace('\\', "\\\\")
            .replace('*', "\\*")
            .replace('_', "\\_")
            .replace('[', "\\[")
            .replace(']', "\\]")
            .replace('(', "\\(")
            .replace(')', "\\)")
            .replace('#', "\\#")
            .replace('+', "\\+")
            .replace('!', "\\!")
    }

    fn append_table(&self, out: &mut String, table: &Table<'_>) {
        if table.rows.is_empty() {
            return;
        }

        let col_count = table.rows.first().map(|r| r.cells.len()).unwrap_or(0);
        if col_count == 0 {
            return;
        }

        for (i, row) in table.rows.iter().enumerate() {
            // Build cells vec
            let mut cells: Vec<String> = row
                .cells
                .iter()
                .map(|cell| {
                    cell.content
                        .iter()
                        .filter_map(|c| match c {
                            TableCellContent::Paragraph(p) => {
                                Some(p.iter_text().map(|t| t.as_ref()).collect::<String>())
                            }
                        })
                        .collect::<String>()
                        .trim()
                        .to_string()
                })
                .collect();

            // Pad to col_count
            while cells.len() < col_count {
                cells.push(String::new());
            }
            cells.truncate(col_count);

            // Escape pipes and write row
            out.push('|');
            for c in &cells {
                out.push_str(" ");
                out.push_str(&c.replace('|', "\\|"));
                out.push('|');
            }
            out.push('\n');

            // Separator after first (header) row
            if i == 0 {
                out.push('|');
                for _ in 0..col_count {
                    out.push_str(" --- |");
                }
                out.push('\n');
            }
        }

        out.push('\n');
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_md() {
        assert_eq!(Converter::escape_md("hello world"), "hello world");
        assert_eq!(Converter::escape_md("**bold**"), "\\*\\*bold\\*\\*");
        assert_eq!(Converter::escape_md("foo|bar"), "foo\\|bar");
    }

    #[test]
    fn test_inline_bold() {
        let mut props = CharacterProperty::default();
        props.bold = Some(docx::formatting::Bold);
        assert_eq!(Converter::apply_inline("hello", &props), "**hello**");
    }

    #[test]
    fn test_inline_bold_italic() {
        let mut props = CharacterProperty::default();
        props.bold = Some(docx::formatting::Bold);
        props.italics = Some(docx::formatting::Italics);
        assert_eq!(Converter::apply_inline("world", &props), "_**world**_");
    }

    #[test]
    fn test_heading_from_style() {
        let c = Converter::new();
        assert_eq!(c.heading_from_style("Title", "Hello"), Some("# Hello".to_string()));
        assert_eq!(c.heading_from_style("Heading2", "Section"), Some("## Section".to_string()));
        assert_eq!(c.heading_from_style("Normal", "text"), None);
    }
}
