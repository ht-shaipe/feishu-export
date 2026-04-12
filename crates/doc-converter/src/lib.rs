//! Pure Rust docx → Markdown converter (no external tools).
//!
//! Uses the `docx` crate for structured parsing with graceful fallback
//! to regex-based plain-text extraction when the file has non-conformant XML.
//!
//! # Image embedding
//! When `embed_images` is enabled (default: **true**), images found inside
//! the `.docx` package are extracted and embedded as Base64 data URIs:
//! ```markdown
//! ![image](data:image/png;base64,iVBORw0...)
//! ```

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};

use regex::Regex;
use std::collections::HashMap;
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
// Image map: rId → (data_uri_string)
// ─────────────────────────────────────────────────────────────────────────────

/// Maps relationship IDs (e.g. `"rId5"`) → base64 data URIs.
type ImageMap = HashMap<String, String>;

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Markdown converter.
#[derive(Debug, Clone)]
pub struct Converter {
    pub overwrite: bool,
    /// When true (default), images are extracted and embedded as Base64 data URIs.
    /// Set `output_images_dir` to save images to disk instead.
    pub embed_images: bool,
    /// When set, images are saved to this directory and referenced by relative path.
    /// When None, images are embedded as Base64 data URIs.
    pub output_images_dir: Option<PathBuf>,
}

impl Default for Converter {
    fn default() -> Self {
        Self { overwrite: false, embed_images: true, output_images_dir: None }
    }
}

impl Converter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn overwrite(mut self) -> Self {
        self.overwrite = true;
        self
    }

    /// Disable image embedding (images will be silently skipped).
    pub fn no_images(mut self) -> Self {
        self.embed_images = false;
        self
    }

    /// Save extracted images to the given directory instead of embedding as Base64.
    /// The directory will be created if it does not exist.
    /// Relative paths are resolved from the current working directory.
    pub fn output_images_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.output_images_dir = Some(dir.into());
        self
    }

    /// Convert a `.docx` file to a Markdown string.
    ///
    /// On parse failure, falls back to plain-text extraction from XML.
    pub fn convert_file<P: AsRef<Path>>(&self, path: P) -> Result<String, Error> {
        let path = path.as_ref();
        self.validate_input(path)?;

        let bytes = std::fs::read(path)?;
        self.convert_bytes_inner(&bytes)
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
        self.convert_bytes_inner(&bytes.into())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Internal conversion entry point
    // ─────────────────────────────────────────────────────────────────────────

    fn convert_bytes_inner(&self, bytes: &[u8]) -> Result<String, Error> {
        // Build image map from the ZIP (always attempt, even if embed disabled)
        let image_map = if self.embed_images {
            self.extract_image_map(bytes).unwrap_or_default()
        } else {
            ImageMap::new()
        };

        match self.parse_document(bytes, &image_map) {
            Ok(md) => Ok(md),
            Err(_e) => {
                // Fallback: extract plain text from XML with image support
                let fallback = self.extract_plain_text(bytes, &image_map)?;
                if fallback.trim().is_empty() {
                    Err(Error::Parse("无法解析 docx 内容".to_string()))
                } else {
                    Ok(fallback)
                }
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Image extraction: build rId → data-URI map
    // ─────────────────────────────────────────────────────────────────────────

    /// Extract all images from the docx ZIP.
    ///
    /// Two modes:
    /// - `output_images_dir` is set → save files to disk, return `rId → relative/path.ext`
    /// - otherwise → return `rId → data:image/…;base64,…`
    fn extract_image_map(&self, bytes: &[u8]) -> Result<ImageMap, Error> {
        let mut zip = ZipArchive::new(std::io::Cursor::new(bytes))
            .map_err(|e| Error::Parse(format!("zip error: {:?}", e)))?;

        let rels = self.parse_relationships(&mut zip)?;

        let mut map = ImageMap::new();

        for (rid, target) in &rels {
            let zip_path = normalise_media_path(target);
            if zip_path.is_none() {
                continue;
            }
            let zip_path = zip_path.unwrap();

            let mut img_bytes = Vec::new();
            match zip.by_name(&zip_path) {
                Ok(mut f) => {
                    f.read_to_end(&mut img_bytes)
                        .map_err(|e| Error::Parse(format!("read image: {}", e)))?;
                }
                Err(_) => continue,
            }

            if let Some(ref dir) = self.output_images_dir {
                // Save to disk and return relative path
                let ext = target.rsplit('.').next().unwrap_or("png");
                let filename = format!("{}_{}.{}", rid.replace(':', "_"), sanitize_filename(target), ext);
                let img_path = dir.join(&filename);
                std::fs::write(&img_path, &img_bytes)
                    .map_err(|e| Error::Parse(format!("write image {}: {}", filename, e)))?;
                // Relative path from MD output dir (dir is already absolute or cwd-relative)
                let rel = format!("./{}", filename);
                map.insert(rid.clone(), rel);
            } else {
                // Base64 data URI
                let mime = mime_from_path(&zip_path);
                let b64 = B64.encode(&img_bytes);
                let data_uri = format!("data:{};base64,{}", mime, b64);
                map.insert(rid.clone(), data_uri);
            }
        }

        Ok(map)
    }

    /// Parse `word/_rels/document.xml.rels` and return `rId → Target` map.
    fn parse_relationships(&self, zip: &mut ZipArchive<std::io::Cursor<&[u8]>>) -> Result<HashMap<String, String>, Error> {
        let mut xml = String::new();
        match zip.by_name("word/_rels/document.xml.rels") {
            Ok(mut f) => {
                f.read_to_string(&mut xml)
                    .map_err(|e| Error::Parse(format!("read rels: {}", e)))?;
            }
            Err(_) => return Ok(HashMap::new()),
        }

        // <Relationship Id="rId5" Type="...image..." Target="../media/image1.png"/>
        let re = Regex::new(r#"(?i)<Relationship[^>]+Id="([^"]+)"[^>]+Target="([^"]+)"[^>]*/>"#)
            .unwrap();

        let mut map = HashMap::new();
        for cap in re.captures_iter(&xml) {
            let id = cap[1].to_string();
            let target = cap[2].to_string();
            map.insert(id, target);
        }
        Ok(map)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Structured parsing via docx crate
    // ─────────────────────────────────────────────────────────────────────────

    fn parse_document(&self, bytes: &[u8], image_map: &ImageMap) -> Result<String, Error> {
        // The XML pass is the primary path because it correctly detects all
        // <w:drawing> elements regardless of how the docx crate surfaces them.
        let drawing_map = self.extract_drawing_map(bytes).unwrap_or_default();
        self.document_to_md_xml_pass(bytes, image_map, &drawing_map)
    }

    /// Extract a map of paragraph index → image markdown from raw XML <w:drawing> elements.
    ///
    /// Returns `HashMap<para_pos_hint, Vec<rId>>` – we use the paragraph XML offset
    /// as a rough positional key to associate images with surrounding paragraphs.
    fn extract_drawing_map(&self, bytes: &[u8]) -> Result<Vec<String>, Error> {
        let mut zip = ZipArchive::new(std::io::Cursor::new(bytes))
            .map_err(|e| Error::Parse(format!("zip error: {:?}", e)))?;

        let mut xml = String::new();
        {
            let mut f = zip.by_name("word/document.xml")
                .map_err(|e| Error::Parse(format!("missing document.xml: {:?}", e)))?;
            f.read_to_string(&mut xml)
                .map_err(|e| Error::Parse(format!("read document.xml: {}", e)))?;
        }

        // Extract all r:embed values inside <w:drawing> blocks (order preserved)
        // Pattern: <a:blip r:embed="rId5" ... />
        let drawing_re = Regex::new(r"(?s)<w:drawing\b.*?</w:drawing>").unwrap();
        let embed_re = Regex::new(r#"r:embed="([^"]+)""#).unwrap();
        let descr_re = Regex::new(r#"(?:descr|name)="([^"]+)""#).unwrap();

        let mut result = Vec::new();
        for drawing in drawing_re.find_iter(&xml) {
            let drawing_xml = drawing.as_str();
            if let Some(cap) = embed_re.captures(drawing_xml) {
                let rid = cap[1].to_string();
                // Try to get alt text from descr or name attribute
                let alt = descr_re.captures(drawing_xml)
                    .and_then(|c| c.get(1))
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_else(|| "image".to_string());
                result.push(format!("{}|{}", rid, alt));
            }
        }
        Ok(result)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Regex fallback: plain-text extraction from raw XML
    // ─────────────────────────────────────────────────────────────────────────

    /// Extract plain text from docx XML, preserving paragraph boundaries.
    /// Drawings are resolved to base64 data URIs using the provided image_map.
    fn extract_plain_text(&self, bytes: &[u8], image_map: &ImageMap) -> Result<String, Error> {
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
        // Run block: <w:r ...>...</w:r>
        let run_re = Regex::new(r"<w:r\b[^>]*>(.*?)</w:r>").unwrap();

        // Drawing / image reference
        let drawing_re = Regex::new(r"(?s)<w:drawing\b.*?</w:drawing>").unwrap();
        let embed_re = Regex::new(r#"r:embed="([^"]+)""#).unwrap();
        let descr_re = Regex::new(r#"(?:descr|name)="([^"]+)""#).unwrap();

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

            // ── Images inside this paragraph ──────────────────────────────────
            let mut img_parts: Vec<String> = Vec::new();
            if self.embed_images {
                for drawing in drawing_re.find_iter(para_xml) {
                    let dxml = drawing.as_str();
                    if let Some(cap) = embed_re.captures(dxml) {
                        let rid = &cap[1];
                        let alt = descr_re.captures(dxml)
                            .and_then(|c| c.get(1))
                            .map(|m| m.as_str())
                            .unwrap_or("image");
                        if let Some(data_uri) = image_map.get(rid) {
                            img_parts.push(format!("![{}]({})\n\n", alt, data_uri));
                        }
                    }
                }
            }

            // Extract runs with per-run formatting
            let mut para_parts: Vec<String> = Vec::new();
            for run_cap in run_re.captures_iter(para_xml) {
                let run_xml = run_cap.get(1).map(|m| m.as_str()).unwrap_or("");

                // Check run-level formatting (bold/italic/strike within this run only)
                let is_bold = bold_re.is_match(run_xml);
                let is_italic = italic_re.is_match(run_xml);
                let is_strike = strike_re.is_match(run_xml);

                // Extract text from this run
                let run_texts: Vec<&str> = text_re.captures_iter(run_xml)
                    .filter_map(|c| c.get(1).map(|m| m.as_str().trim()))
                    .filter(|s| !s.is_empty())
                    .collect();

                for raw in run_texts {
                    let mut t = raw.to_string();
                    if is_strike { t = format!("~~{}~~", t); }
                    if is_bold   { t = format!("**{}**", t); }
                    if is_italic { t = format!("_{}_", t); }
                    para_parts.push(t);
                }
            }

            let joined = para_parts.join("").trim().to_string();

            let has_text = !joined.is_empty();
            let has_img = !img_parts.is_empty();

            if !has_text && !has_img {
                continue;
            }

            if seen_para_start {
                out.push('\n');
            }
            seen_para_start = true;

            if has_text {
                if is_heading {
                    out.push_str(&"#".repeat(heading_level));
                    out.push(' ');
                }
                out.push_str(&joined);
                out.push_str("\n\n");
            }

            for img in img_parts {
                out.push_str(&img);
                out.push_str("\n\n");
            }
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

            // ── Pseudo-table detection: single-column code/directory-tree content ──
            if let Some((code_lang, skip_first)) = Self::detect_code_language(table_xml) {
                out.push_str(&format!("\n```{}\n", code_lang));
                let mut first = true;
                for rcap in row_re.captures_iter(table_xml) {
                    let row_xml = rcap.get(1).map(|m| m.as_str()).unwrap_or("");
                    // Skip the first row if it's just a language label (e.g. "Bash")
                    if first && skip_first {
                        first = false;
                        continue;
                    }
                    first = false;
                    for ccap in cell_re.captures_iter(row_xml) {
                        if let Some(cell_xml) = ccap.get(1).map(|m| m.as_str()) {
                            let texts: String = cell_text_re.captures_iter(cell_xml)
                                .filter_map(|tc| tc.get(1).map(|m| m.as_str().trim()))
                                .collect();
                            if !texts.trim().is_empty() {
                                out.push_str(texts.trim());
                                out.push('\n');
                            }
                        }
                    }
                }
                out.push_str("```\n\n");
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

    /// Process the raw XML paragraph list directly.
    ///
    /// This pass is the key to reliable image extraction. The `docx` crate's
    /// `ParagraphContent` does NOT expose `<w:drawing>` elements, so the primary
    /// pass cannot detect inline images that appear alongside text in the same
    /// paragraph. This XML pass correctly handles all three cases:
    ///   1. Image-only paragraph  (text empty, drawing present)
    ///   2. Text + inline image   (text present, drawing in same <w:p>)
    ///   3. Text-only paragraph   (no drawing)
    ///
    /// It processes every paragraph in document order and produces the final
    /// interleaved markdown.
    fn document_to_md_xml_pass(&self, bytes: &[u8], image_map: &ImageMap, drawing_order: &[String]) -> Result<String, Error> {
        let mut zip = ZipArchive::new(std::io::Cursor::new(bytes))
            .map_err(|e| Error::Parse(format!("zip error: {:?}", e)))?;

        let mut xml = String::new();
        {
            let mut docx_xml = zip.by_name("word/document.xml")
                .map_err(|e| Error::Parse(format!("missing document.xml: {:?}", e)))?;
            docx_xml.read_to_string(&mut xml)
                .map_err(|e| Error::Parse(format!("read xml: {}", e)))?;
        }

        // Remove tables from XML so paragraph regex doesn't pick up table-cell paragraphs.
        // Tables are processed separately in the "Tables" section below.
        let table_re = Regex::new(r"(?s)<w:tbl>.*?</w:tbl>").unwrap();
        let xml_no_tables = table_re.replace_all(&xml, "").to_string();

        let para_re = Regex::new(r"(?s)<w:p[ >].*?</w:p>").unwrap();
        let text_re  = Regex::new(r"<w:t[^>]*>([^<]*)</w:t>").unwrap();

        let heading1_re = Regex::new(r#"<w:pStyle[^>]+w:val="(Heading1|Title)"/>"#).unwrap();
        let heading2_re = Regex::new(r#"<w:pStyle[^>]+w:val="Heading2"/>"#).unwrap();
        let heading3_re = Regex::new(r#"<w:pStyle[^>]+w:val="Heading3"/>"#).unwrap();

        let bold_re   = Regex::new(r"<w:b[^/]*/>").unwrap();
        let italic_re = Regex::new(r"<w:i[^/]*/>").unwrap();
        let strike_re = Regex::new(r"<w:(?:d?)strike[^/]*/>").unwrap();

        let drawing_re = Regex::new(r"(?s)<w:drawing\b.*?</w:drawing>").unwrap();
        let embed_re   = Regex::new(r#"r:embed="([^"]+)""#).unwrap();
        let descr_re   = Regex::new(r#"(?:descr|name)="([^"]+)""#).unwrap();

        let mut out = String::new();
        let _drawing_order = drawing_order; // consumed by drawing_map in caller

        // ── Collect all top-level elements (paragraphs and tables) by position ──
        // Tables are matched in original xml; paragraphs are matched in xml_no_tables
        // (which has tables removed), so all matched paragraphs are body-level.

        let table_re  = Regex::new(r"(?s)<w:tbl>.*?</w:tbl>").unwrap();
        let row_re    = Regex::new(r"(?s)<w:tr[ >].*?</w:tr>").unwrap();
        let cell_re   = Regex::new(r"(?s)<w:tc[ >].*?</w:tc>").unwrap();
        let cell_txt  = Regex::new(r"<w:t[^>]*>([^<]*)</w:t>").unwrap();

        #[derive(Debug, Clone, Copy, PartialEq)]
        enum ElementKind { Para, Table }

        #[derive(Debug)]
        struct Element {
            kind: ElementKind,
            start: usize,
            content: String,
        }

        let mut elements: Vec<Element> = Vec::new();

        // Collect body paragraphs (from xml_no_tables, so they're definitely not in tables)
        for para_match in para_re.find_iter(&xml_no_tables) {
            elements.push(Element {
                kind: ElementKind::Para,
                start: para_match.start(),
                content: para_match.as_str().to_string(),
            });
        }

        // Collect tables
        for tbl_match in table_re.find_iter(&xml) {
            elements.push(Element {
                kind: ElementKind::Table,
                start: tbl_match.start(),
                content: tbl_match.as_str().to_string(),
            });
        }

        // Sort by original document position
        elements.sort_by_key(|e| e.start);

        // ── Process elements in order ────────────────────────────────────────────

        for elem in elements {
            match elem.kind {
                ElementKind::Para => {
                    let para_xml = &elem.content;
            let text_parts: Vec<String> = text_re.captures_iter(para_xml)
                .filter_map(|c| c.get(1).map(|m| m.as_str().trim().to_string()))
                .filter(|s| !s.is_empty())
                .collect();

            // ── Extract drawings in this paragraph ────────────────────────────────
            let para_drawings: Vec<&str> = drawing_re.find_iter(para_xml).map(|m| m.as_str()).collect();

            // ── Skip empty paragraphs with no drawings ─────────────────────────
            if text_parts.is_empty() && para_drawings.is_empty() {
                continue;
            }

            // ── Apply inline formatting across all text runs ───────────────────
            let raw_text = text_parts.join("");
            if !raw_text.trim().is_empty() {
                let formatted = Self::format_text_with_styles(para_xml, &text_re, &bold_re, &italic_re, &strike_re);
                
                // Check for code style (Code, InlineCode, etc.)
                let is_code_style = para_xml.contains(r#"w:val="Code""#) || 
                                    para_xml.contains(r#"w:val="InlineCode""#) ||
                                    para_xml.contains(r#"w:val="code""#) ||
                                    para_xml.contains(r#"w:val="inlinecode""#);
                
                let line = if !formatted.trim().is_empty() {
                    // Heading detection via style
                    if heading1_re.is_match(para_xml) {
                        format!("# {}\n\n", formatted.trim())
                    } else if heading2_re.is_match(para_xml) {
                        format!("## {}\n\n", formatted.trim())
                    } else if heading3_re.is_match(para_xml) {
                        format!("### {}\n\n", formatted.trim())
                    } else if is_code_style {
                        // Code block: wrap in triple backticks
                        format!("```\n{}\n```\n\n", formatted.trim())
                    } else {
                        // Plain paragraph
                        let mut para_out = formatted.trim().to_string();
                        para_out.push_str("\n\n");
                        para_out
                    }
                } else {
                    String::new()
                };
                out.push_str(&line);
            }

            // ── Emit images from drawings in this paragraph ───────────────────
            for drawing_xml in para_drawings {
                if let Some(cap) = embed_re.captures(drawing_xml) {
                    let rid = cap[1].to_string();
                    let alt = descr_re.captures(drawing_xml)
                        .and_then(|c| c.get(1))
                        .map(|m| m.as_str())
                        .unwrap_or("image");
                    if let Some(data_uri) = image_map.get(&rid) {
                        out.push_str(&format!("![{}]({})\n\n", alt, data_uri));
                    }
                }
            }
        }

                ElementKind::Table => {
                    let table_xml = &elem.content;

            // ── Pseudo-table detection: single-column code/directory-tree content ──
            if let Some((code_lang, skip_first)) = Self::detect_code_language(table_xml) {
                out.push_str(&format!("```{}\n", code_lang));
                let mut first = true;
                for rcap in row_re.find_iter(table_xml) {
                    let row_xml = rcap.as_str();
                    // Skip the first row if it's just a language label (e.g. pure "Bash" table)
                    if first && skip_first {
                        first = false;
                        continue;
                    }
                    let is_first_row = first;
                    first = false;

                    for ccap in cell_re.find_iter(row_xml) {
                        let cell_xml = ccap.as_str();
                        let texts: Vec<&str> = cell_txt.captures_iter(cell_xml)
                            .filter_map(|tc| tc.get(1).map(|m| m.as_str().trim()))
                            .collect();
                        let combined = texts.join(" ");
                        if combined.trim().is_empty() { continue; }

                        // If first non-skipped row starts with the language label, strip it
                        // (e.g. "Bash keytool..." → "keytool...")
                        let final_text = if is_first_row && !code_lang.is_empty() {
                            let lower = combined.to_ascii_lowercase();
                            let label = code_lang.to_lowercase();
                            if lower.starts_with(&label) {
                                let after = combined[label.len()..].trim_start();
                                if after.is_empty() { continue; }
                                after.to_string()
                            } else {
                                combined
                            }
                        } else {
                            combined
                        };

                        if !final_text.trim().is_empty() {
                            out.push_str(final_text.trim());
                            out.push('\n');
                        }
                    }
                }
                out.push_str("```\n\n");
                continue;
            }

            // ── Normal table: render as Markdown table ──────────────────────────
            let mut header_written = false;
            for rcap in row_re.captures_iter(table_xml) {
                let row_xml = rcap.get(0).map(|m| m.as_str()).unwrap_or("");
                let cells: Vec<String> = cell_re.captures_iter(row_xml)
                    .filter_map(|ccap| {
                        let cell_xml = ccap.get(0).map(|m| m.as_str()).unwrap_or("");
                        let texts: Vec<&str> = cell_txt.captures_iter(cell_xml)
                            .filter_map(|tc| tc.get(1).map(|m| m.as_str().trim()))
                            .filter(|s| !s.is_empty())
                            .collect();
                        if texts.is_empty() { None } else { Some(texts.join(" ")) }
                    })
                    .collect();

                if cells.is_empty() { continue; }
                let col_count = cells.len();

                out.push('|');
                out.push_str(&cells.iter().map(|c| format!(" {} |", c.replace('|', "\\|"))).collect::<String>());
                out.push('\n');

                if !header_written {
                    out.push('|');
                    for _ in 0..col_count { out.push_str(" --- |"); }
                    out.push('\n');
                    header_written = true;
                }
            }
            out.push('\n');
                } // end Table branch
            } // end match elem.kind
        } // end for elem in elements

        Ok(out.trim().to_string())
    }

    /// Apply bold/italic/strike formatting to raw text segments within a paragraph XML block.
    fn format_text_with_styles(
        para_xml: &str,
        text_re: &Regex,
        bold_re: &Regex,
        italic_re: &Regex,
        strike_re: &Regex,
    ) -> String {
        // Split by <w:r>...</w:r> blocks to handle per-run formatting
        let run_re = Regex::new(r"(?s)<w:r\b[^>]*>.*?</w:r>").unwrap();
        let mut parts: Vec<String> = Vec::new();

        for run_cap in run_re.captures_iter(para_xml) {
            let run_xml = run_cap.get(0).map(|m| m.as_str()).unwrap_or("");
            let is_bold   = bold_re.is_match(run_xml);
            let is_italic = italic_re.is_match(run_xml);
            let is_strike = strike_re.is_match(run_xml);

            for tc in text_re.captures_iter(run_xml) {
                let raw = tc.get(1).map(|m| m.as_str().trim()).unwrap_or("");
                if raw.is_empty() { continue; }
                let mut s = Self::escape_md(raw);
                if is_strike { s = format!("~~{}~~", s); }
                if is_bold   { s = format!("**{}**", s); }
                if is_italic { s = format!("_{}_", s); }
                parts.push(s);
            }
        }

        parts.join("")
    }

    // ─── Test-only helpers (keep API surface stable) ────────────────────────

    #[cfg(test)]
    fn apply_inline(text: &str, props: &docx::formatting::CharacterProperty<'_>) -> String {
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
            let s = if is_bold   { format!("**{}**", s) } else { s };
            let s = if is_italic { format!("_{}_", s) } else { s };
            let s = if is_strike { format!("~~{}~~", s) } else { s };
            s
        }
    }

    #[cfg(test)]
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

    // ─── End test-only helpers ───────────────────────────────────────────────

    // ─── Helpers ───────────────────────────────────────────────────────────────

    /// Detect if a table is a "pseudo-table" that should be rendered as a code block.
    ///
    /// Single-column tables containing lines that look like code/paths/directory trees
    /// (e.g. `@image:xxx`, `src/components/`, `├── foo`) are common in Word documents
    /// but should NOT be rendered as Markdown tables — they should be code blocks.
    ///
    /// Returns `Some((language, skip_first_row))` if it's a pseudo-table (code block).
    /// - `language`: the markdown code fence language (empty string for plain code block)
    /// - `skip_first_row`: true when the first row is just a label (e.g. "Bash", "JSON")
    ///   and should not appear as code content
    /// Returns `None` if it's a real table that should be rendered as Markdown table.
    fn detect_code_language(table_xml: &str) -> Option<(String, bool)> {
        let cell_re = Regex::new(r"(?s)<w:tc[ >].*?</w:tc>").unwrap();
        let cell_txt = Regex::new(r"<w:t[^>]*>([^<]*)</w:t>").unwrap();

        // Collect all row texts
        let row_re = Regex::new(r"(?s)<w:tr[ >].*?</w:tr>").unwrap();
        let rows: Vec<String> = row_re.find_iter(table_xml)
            .map(|rcap| {
                let row_xml = rcap.as_str();
                let mut all_texts = Vec::new();
                for ccap in cell_re.find_iter(row_xml) {
                    let cell_xml = ccap.as_str();
                    let texts: Vec<&str> = cell_txt.captures_iter(cell_xml)
                        .filter_map(|tc| tc.get(1).map(|m| m.as_str().trim()))
                        .collect();
                    let combined = texts.join(" ");
                    if !combined.trim().is_empty() {
                        all_texts.push(combined.trim().to_string());
                    }
                }
                all_texts.join(" ")
            })
            .collect();

        // Must have at least 1 row
        if rows.is_empty() {
            return None;
        }

        // Language label patterns → markdown language
        let lang_labels: Vec<(&str, &str)> = vec![
            ("plain text", "text"),
            ("code block", "text"),
            ("json", "json"),
            ("xml", "xml"),
            ("yaml", "yaml"),
            ("toml", "toml"),
            ("html", "html"),
            ("css", "css"),
            ("javascript", "javascript"),
            ("typescript", "typescript"),
            ("python", "python"),
            ("rust", "rust"),
            ("sql", "sql"),
            ("bash", "bash"),
            ("shell", "bash"),
            ("cmd", "cmd"),
            ("powershell", "powershell"),
            ("go", "go"),
            ("java", "java"),
            ("c++", "cpp"),
            ("c#", "csharp"),
            ("php", "php"),
            ("ruby", "ruby"),
            ("swift", "swift"),
            ("kotlin", "kotlin"),
            ("markdown", "markdown"),
            ("log", "log"),
            ("output", ""),
            ("result", ""),
            ("error", ""),
            ("warning", ""),
            ("info", ""),
        ];

        // Check if a line is a pure language label (e.g. "Bash", "JSON")
        let is_lang_label = |line: &str| -> Option<String> {
            let lower = line.to_ascii_lowercase();
            for (label, lang) in &lang_labels {
                if lower == *label {
                    return Some(lang.to_string());
                }
            }
            None
        };

        // Check if a line contains code command patterns
        let is_code_like = |line: &str| -> bool {
            if line.is_empty() { return false; }
            let lower = line.to_ascii_lowercase();

            // Pattern 1: @ prefix
            if line.starts_with('@') { return true; }
            // Pattern 2: path separators
            if lower.contains(":/") || line.contains('\\') || line.starts_with('/') { return true; }
            // Pattern 3: directory tree characters
            if lower.contains("├──") || lower.contains("└──") || lower.contains("│") { return true; }
            // Pattern 4: indented code with special chars
            if line.starts_with(' ') && line.contains(|c: char| c == ':' || c == '#' || c == '$') { return true; }
            // Pattern 5: starts with common CLI commands or keywords
            let cli_commands = [
                "sudo ", "npm ", "cargo ", "git ", "docker ", "kubectl ", "helm ",
                "curl ", "wget ", "openssl ", "keytool ", "javac ", "gradle ",
                "mvn ", "make ", "cmake ", "pip ", "pip3 ", "python ", "python3 ",
                "node ", "ruby ", "perl ", "go ", "rustc ", "gcc ", "clang ",
                "ls ", "cd ", "pwd ", "mkdir ", "rm ", "cp ", "mv ", "cat ",
                "grep ", "sed ", "awk ", "find ", "xargs ", "chmod ", "chown ",
                "tar ", "zip ", "unzip ", "ssh ", "scp ", "rsync ", "ping ",
                "netstat ", "ss ", "iptables ", "systemctl ", "journalctl ",
                "ps ", "top ", "htop ", "kill ", "killall ", "pkill ",
                "env ", "export ", "source ", "alias ", "echo ", "printf ",
                "if [", "for ", "while ", "case ", "function ", "=> {",
                "import ", "from ", "require(", "module.exports",
                "const ", "let ", "var ", "fn ", "def ", "func ",
                "public ", "private ", "static ", "class ", "struct ",
                "<!-- ", "-->", "/* ", "*/", "// ", "#!", "###",
                "$ ", "&& ", "|| ", "| ", ">> ", "<< ", "2>", "1>",
                "SELECT ", "INSERT ", "UPDATE ", "DELETE ", "CREATE ",
                "DROP ", "ALTER ", "TABLE ", "INDEX ", "WHERE ",
            ];
            for cmd in &cli_commands {
                if lower.starts_with(cmd) { return true; }
            }
            // Pattern 6: contains command+space+flag (e.g. "keytool -genkey")
            if line.contains(|c: char| c.is_whitespace()) {
                // Split and check for command patterns
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let first = parts[0].to_lowercase();
                    let second = parts[1];
                    // command followed by flag (starts with -)
                    if second.starts_with('-') { return true; }
                    // Common tool names followed by subcommands
                    let tools_with_subcmd = ["npm", "git", "docker", "cargo", "kubectl", "helm",
                        "openssl", "keytool", "curl", "wget", "ssh", "scp", "psql", "mongosh",
                        "make", "cmake", "mvn", "gradle", "javac", "ruby", "python", "node",
                        "go", "rustc", "gcc", "clang", "pip", "pip3", "bun", "pnpm", "yarn"];
                    if tools_with_subcmd.contains(&first.as_str()) { return true; }
                    // Pattern 6b: first word is a language label (e.g. "Bash sudo keytool")
                    if is_lang_label(&first).is_some() { return true; }
                }
            }
            // Pattern 7: is a pure language label
            if is_lang_label(line).is_some() { return true; }
            false
        };

        // For multi-row tables: first row is label, rest are code
        if rows.len() >= 2 {
            let first_lang = is_lang_label(&rows[0]);
            if first_lang.is_some() {
                let rest_all_code = rows[1..].iter().all(|r| is_code_like(r) || r.is_empty());
                if rest_all_code {
                    return Some((first_lang.unwrap_or_default(), true));
                }
            }
        }

        // For ALL tables (single or multi-row): check if content looks like code
        let all_code = rows.iter().all(|r| is_code_like(r) || r.is_empty());
        if all_code {
            // Try to detect language from content
            let combined = rows.join(" ");
            let lower = combined.to_ascii_lowercase();
            for (label, lang) in &lang_labels {
                if lower.starts_with(label) {
                    // Row(s) start with a language label
                    // - multi-row: first row is pure label → skip_first=true
                    // - single-row with label+code: strip label from content → skip_first=false (strip_in_render=true implied)
                    let after_label = lower[label.len()..].trim_start();
                    let is_pure_label_row = rows.len() == 1 && after_label.is_empty();
                    // skip_first=true: skip entire first row; strip_in_render=false
                    // skip_first=false + first_row_has_label=true: output content stripped of label
                    let skip_first = rows.len() > 1 || is_pure_label_row;
                    return Some((lang.to_string(), skip_first));
                }
                if lower.contains(&format!(" {} ", label)) {
                    return Some((lang.to_string(), false));
                }
            }
            // No specific language → plain code block
            return Some((String::new(), false));
        }

        None
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
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper functions
// ─────────────────────────────────────────────────────────────────────────────

/// Normalise a relationship Target to a ZIP entry path under `word/`.
/// Returns `None` if the target does not look like a media file.
fn normalise_media_path(target: &str) -> Option<String> {
    // Targets look like: "../media/image1.png" or "media/image1.png"
    let lower = target.to_ascii_lowercase();

    // Must contain "media/" somewhere
    if !lower.contains("media/") {
        return None;
    }

    // Must have a known image extension
    let known_ext = ["png", "jpg", "jpeg", "gif", "bmp", "webp", "tiff", "tif", "emf", "wmf", "svg"];
    let has_ext = known_ext.iter().any(|ext| lower.ends_with(ext));
    if !has_ext {
        return None;
    }

    // Resolve relative path: "../media/foo.png" -> "word/media/foo.png"
    if target.starts_with("../") {
        Some(format!("word/{}", &target[3..]))
    } else if target.starts_with('/') {
        // Absolute within ZIP — strip leading slash
        Some(target.trim_start_matches('/').to_string())
    } else {
        // Relative to word/ — "media/foo.png" -> "word/media/foo.png"
        Some(format!("word/{}", target))
    }
}

/// Guess MIME type from file path extension.
fn mime_from_path(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".png")  { return "image/png"; }
    if lower.ends_with(".jpg") || lower.ends_with(".jpeg") { return "image/jpeg"; }
    if lower.ends_with(".gif")  { return "image/gif"; }
    if lower.ends_with(".bmp")  { return "image/bmp"; }
    if lower.ends_with(".webp") { return "image/webp"; }
    if lower.ends_with(".tiff") || lower.ends_with(".tif") { return "image/tiff"; }
    if lower.ends_with(".svg")  { return "image/svg+xml"; }
    // EMF / WMF are Windows meta-files; embed as octet-stream so viewers can skip gracefully
    "image/x-emf"
}

/// Strip path components and unsafe characters to produce a safe filename.
/// e.g. "../media/photo%20(1).png" → "photo_1_"
fn sanitize_filename(target: &str) -> String {
    let base = std::path::Path::new(target)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("image");

    base.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use docx::formatting::CharacterProperty;

    #[test]
    fn test_escape_md() {
        assert_eq!(Converter::escape_md("hello world"), "hello world");
        assert_eq!(Converter::escape_md("**bold**"), "\\*\\*bold\\*\\*");
    }

    #[test]
    fn test_inline_bold() {
        let mut props = CharacterProperty::default();
        props.bold = Some(docx::formatting::Bold::from(true));
        assert_eq!(Converter::apply_inline("hello", &props), "**hello**");
    }

    #[test]
    fn test_inline_bold_italic() {
        let mut props = CharacterProperty::default();
        props.bold = Some(docx::formatting::Bold::from(true));
        props.italics = Some(docx::formatting::Italics::from(true));
        assert_eq!(Converter::apply_inline("world", &props), "_**world**_");
    }

    #[test]
    fn test_heading_from_style() {
        let c = Converter::new();
        assert_eq!(c.heading_from_style("Title", "Hello"), Some("# Hello".to_string()));
        assert_eq!(c.heading_from_style("Heading2", "Section"), Some("## Section".to_string()));
        assert_eq!(c.heading_from_style("Normal", "text"), None);
    }

    #[test]
    fn test_normalise_media_path() {
        assert_eq!(normalise_media_path("../media/image1.png"), Some("word/media/image1.png".to_string()));
        assert_eq!(normalise_media_path("media/image1.jpg"), Some("word/media/image1.jpg".to_string()));
        assert_eq!(normalise_media_path("../embeddings/sheet.xlsx"), None);
        assert_eq!(normalise_media_path("hyperlink_target"), None);
    }

    #[test]
    fn test_mime_from_path() {
        assert_eq!(mime_from_path("word/media/img.png"), "image/png");
        assert_eq!(mime_from_path("word/media/photo.JPEG"), "image/jpeg");
        assert_eq!(mime_from_path("word/media/anim.gif"), "image/gif");
    }

    #[test]
    fn test_converter_no_images_flag() {
        let c = Converter::new().no_images();
        assert!(!c.embed_images);
    }
}
