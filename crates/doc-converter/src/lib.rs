//! Pure Rust docx → Markdown converter (no external tools).
//!
//! Uses the `docx` crate for structured parsing with graceful fallback
//! to regex-based plain-text extraction when the file has non-conformant XML.

use docx::document::{BodyContent, Document, Paragraph, ParagraphContent, RunContent, Table, TableCellContent};
use docx::formatting::CharacterProperty;
use docx::DocxFile;
use regex::Regex;
use std::io::Read;
use std::path::{Path, PathBuf};
use thiserror::Error;
use zip::ZipArchive;

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
    pub overwrite: bool,
}

impl Converter {
    pub fn new() -> Self {
        Self { overwrite: false }
    }

    pub fn overwrite(mut self) -> Self {
        self.overwrite = true;
        self
    }

    /// Convert a `.docx` file to a Markdown string.
    ///
    /// On parse failure, falls back to plain-text extraction from XML.
    pub fn convert_file<P: AsRef<Path>>(&self, path: P) -> Result<String, Error> {
        let path = path.as_ref();
        self.validate_input(path)?;

        let bytes = std::fs::read(path)?;

        match self.parse_document(&bytes) {
            Ok(md) => Ok(md),
            Err(e) => {
                // Fallback: extract plain text from XML
                let fallback = self.extract_plain_text(&bytes)?;
                if fallback.trim().is_empty() {
                    Err(e) // no fallback content, return original error
                } else {
                    Ok(fallback)
                }
            }
        }
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

    /// Convert docx bytes to Markdown.
    pub fn convert_bytes(&self, bytes: impl Into<Vec<u8>>) -> Result<String, Error> {
        let bytes = bytes.into();

        match self.parse_document(&bytes) {
            Ok(md) => Ok(md),
            Err(_e) => {
                let fallback = self.extract_plain_text(&bytes)?;
                if fallback.trim().is_empty() {
                    Err(Error::Parse("无法解析 docx 内容".to_string()))
                } else {
                    Ok(fallback)
                }
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Structured parsing via docx crate
    // ─────────────────────────────────────────────────────────────────────────

    fn parse_document(&self, bytes: &[u8]) -> Result<String, Error> {
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
    // Regex fallback: plain-text extraction from raw XML
    // ─────────────────────────────────────────────────────────────────────────

    /// Extract plain text from docx XML, preserving paragraph boundaries.
    fn extract_plain_text(&self, bytes: &[u8]) -> Result<String, Error> {
        let mut zip = ZipArchive::new(std::io::Cursor::new(bytes))
            .map_err(|e| Error::Parse(format!("zip error: {:?}", e)))?;

        let mut xml = String::new();
        {
            let mut docx_xml = zip
                .by_name("word/document.xml")
                .map_err(|e| Error::Parse(format!("missing document.xml: {:?}", e)))?;
            docx_xml.read_to_string(&mut xml)
                .map_err(|e| Error::Parse(format!("read xml: {}", e)))?;
        }

        // Paragraph boundary markers (preserve structure)
        let para_re = Regex::new(r#"<w:p[ >]"#).unwrap();
        let end_para_re = Regex::new(r#"</w:p>"#).unwrap();

        // Extract text content: <w:t>...</w:t>
        let text_re = Regex::new(r"<w:t[^>]*>([^<]*)</w:t>").unwrap();

        // Extract heading styles
        let heading1_re = Regex::new(r#"<w:pStyle w:val="Heading1"/>|<w:pStyle w:val="Title"/>"#).unwrap();
        let heading2_re = Regex::new(r#"<w:pStyle w:val="Heading2"/>"#).unwrap();
        let heading3_re = Regex::new(r#"<w:pStyle w:val="Heading3"/>"#).unwrap();

        // Bold: <w:b/> or <w:b .../>
        let bold_re = Regex::new(r"<w:b[^/]*/>").unwrap();
        // Italic: <w:i/> or <w:i .../>
        let italic_re = Regex::new(r"<w:i[^/]*/>").unwrap();
        // Strike: <w:strike/> or <w:dstrike/>
        let strike_re = Regex::new(r"<w:(?:d?)strike[^/]*/>").unwrap();

        let mut out = String::new();
        let mut seen_para_start = false;

        let xml_lower = xml.to_lowercase();

        for para_start in para_re.find_iter(&xml_lower) {
            let pos = para_start.start();
            let para_slice = &xml[pos..];

            // Detect heading
            let (is_heading, heading_level) = if heading1_re.is_match(para_slice) {
                (true, 1)
            } else if heading2_re.is_match(para_slice) {
                (true, 2)
            } else if heading3_re.is_match(para_slice) {
                (true, 3)
            } else {
                (false, 0)
            };

            // Find end of this paragraph
            let para_end = end_para_re.find(para_slice)
                .map(|m| m.start())
                .unwrap_or(para_slice.len());
            let para_xml = &para_slice[..para_end];

            // Extract all <w:t> texts
            let mut para_parts: Vec<String> = Vec::new();
            for cap in text_re.captures_iter(para_xml) {
                if let Some(text) = cap.get(1) {
                    let raw = text.as_str().trim();
                    if !raw.is_empty() {
                        let mut t = raw.to_string();

                        if strike_re.is_match(para_xml) {
                            t = format!("~~{}~~", t);
                        }
                        if bold_re.is_match(para_xml) {
                            t = format!("**{}**", t);
                        }
                        if italic_re.is_match(para_xml) {
                            t = format!("_{}_", t);
                        }

                        para_parts.push(t);
                    }
                }
            }

            let joined = para_parts.join("").trim().to_string();
            if joined.is_empty() {
                continue;
            }

            if seen_para_start {
                out.push('\n');
            }
            seen_para_start = true;

            if is_heading {
                out.push_str(&"#".repeat(heading_level));
                out.push(' ');
            }

            out.push_str(&joined);
            out.push_str("\n\n");
        }

        // Table fallback: extract simple text tables
        let table_re = Regex::new(r"<w:tbl>(.*?)</w:tbl>").unwrap();
        let row_re = Regex::new(r"<w:tr[ >](.*?)</w:tr>").unwrap();
        let cell_re = Regex::new(r"<w:tc[ >](.*?)</w:tc>").unwrap();
        let cell_text_re = Regex::new(r"<w:t[^>]*>([^<]*)</w:t>").unwrap();

        for tcap in table_re.captures_iter(&xml_lower) {
            let table_xml = tcap.get(1).map(|m| m.as_str()).unwrap_or("");

            // Check if already processed (in paragraph text)
            if out.contains(&Self::table_preview(table_xml)) {
                continue;
            }

            out.push('\n');
            let mut header_written = false;

            for rcap in row_re.captures_iter(table_xml) {
                let row_xml = rcap.get(1).map(|m| m.as_str()).unwrap_or("");
                let cells: Vec<String> = cell_re.captures_iter(row_xml)
                    .filter_map(|ccap| {
                        let cell_xml = ccap.get(1)?.as_str();
                        let texts: Vec<&str> = cell_text_re.captures_iter(cell_xml)
                            .filter_map(|tc| tc.get(1).map(|m| m.as_str().trim()))
                            .filter(|s| !s.is_empty())
                            .collect();
                        if texts.is_empty() { None } else { Some(texts.join(" ")) }
                    })
                    .collect();

                if cells.is_empty() { continue; }

                if !header_written {
                    let col_count = cells.len();
                    out.push('|');
                    out.push_str(&cells.iter().map(|c| format!(" {} |", c)).collect::<String>());
                    out.push('\n');
                    out.push('|');
                    out.push_str(&"| --- |".repeat(col_count));
                    out.push('\n');
                    header_written = true;
                } else {
                    out.push('|');
                    out.push_str(&cells.iter().map(|c| format!(" {} |", c.replace('|', "\\|"))).collect::<String>());
                    out.push('\n');
                }
            }
            out.push('\n');
        }

        Ok(out.trim().to_string())
    }

    fn table_preview(table_xml: &str) -> String {
        let cell_re = Regex::new(r"<w:tc[ >](.*?)</w:tc>").unwrap();
        let cell_text_re = Regex::new(r"<w:t[^>]*>([^<]*)</w:t>").unwrap();
        let row_re = Regex::new(r"<w:tr[ >](.*?)</w:tr>").unwrap();

        let first_row = row_re.captures_iter(table_xml).next()
            .and_then(|r| r.get(1))
            .map(|m| m.as_str())
            .unwrap_or("");

        cell_re.captures_iter(first_row)
            .filter_map(|c| {
                let txt = c.get(1)?.as_str();
                let first_text = cell_text_re.captures(txt)?.get(1)?.as_str().trim();
                Some(first_text)
            })
            .collect::<Vec<_>>()
            .join(" ")
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

        if let Some(sid) = self.style_id(para) {
            if let Some(heading) = self.heading_from_style(&sid, &text) {
                out.push_str(&heading);
                out.push_str("\n\n");
                return;
            }
        }

        if let Some(heading) = self.detect_heading_from_content(&text) {
            out.push_str(&heading);
            out.push_str("\n\n");
            return;
        }

        let rendered = self.render_paragraph(para);
        out.push_str(&rendered);
        out.push('\n');
    }

    fn style_id<'a>(&self, para: &'a Paragraph) -> Option<String> {
        para.property.style_id.as_ref().and_then(|sid| {
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
        Some(format!("{} {}", "#".repeat(level), text))
    }

    fn detect_heading_from_content(&self, text: &str) -> Option<String> {
        let trimmed = text.trim();
        if trimmed.len() <= 80
            && trimmed.chars().filter(|c| c.is_ascii_alphabetic()).count() > 3
            && trimmed == trimmed.to_uppercase()
        {
            return Some(format!("# {}", trimmed));
        }
        if trimmed.len() <= 120
            && (trimmed.starts_with("1.") || trimmed.starts_with("2.") || trimmed.starts_with("3."))
        {
            return Some(format!("## {}", trimmed));
        }
        None
    }

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

                    if run_text.is_empty() { continue; }
                    parts.push(Self::apply_inline(&run_text, &run.property));
                }
                ParagraphContent::Link(link) => {
                    let run_text = link.content.content.iter()
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
        if joined.is_empty() { String::new() } else { joined }
    }

    fn apply_inline(text: &str, props: &CharacterProperty<'_>) -> String {
        if text.is_empty() { return String::new(); }

        let is_bold = props.bold.is_some();
        let is_italic = props.italics.is_some();
        let is_strike = props.strike.is_some();
        let is_code = props.style_id.as_ref().is_some_and(|s| {
            matches!(s.value.as_ref().to_ascii_lowercase().as_str(), "code" | "inlinecode")
        });

        let escaped = if is_code { text.to_string() } else { Self::escape_md(text) };

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
        if table.rows.is_empty() { return; }

        let col_count = table.rows.first().map(|r| r.cells.len()).unwrap_or(0);
        if col_count == 0 { return; }

        for (i, row) in table.rows.iter().enumerate() {
            let mut cells: Vec<String> = row.cells.iter()
                .map(|cell| {
                    cell.content.iter()
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

            while cells.len() < col_count { cells.push(String::new()); }
            cells.truncate(col_count);

            out.push('|');
            for c in &cells {
                out.push_str(" ");
                out.push_str(&c.replace('|', "\\|"));
                out.push('|');
            }
            out.push('\n');

            if i == 0 {
                out.push('|');
                for _ in 0..col_count { out.push_str(" --- |"); }
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
