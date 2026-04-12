//! `doc-md` — Pure Rust docx → Markdown converter powered by pandoc
//!
//! ## Usage
//!
//! ```rust,no_run
//! use doc_md::Converter;
//!
//! let converter = Converter::new()?;
//! converter.convert("input.docx", "output.md")?;
//! # Ok::<(), doc_md::Error>(())
//! ```
//!
//! ## Feature flags
//!
//! - `async` — enables async conversion via `tokio::process::Command`

use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;

// ─────────────────────────────────────────────────────────────────────────────
// Error types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Error, Debug)]
pub enum Error {
    #[error("pandoc not found. Install from https://pandoc.org/installing.html")]
    PandocNotFound,

    #[error("pandoc execution failed: {0}")]
    PandocFailed(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("output file already exists: {0}")]
    OutputExists(PathBuf),

    #[error("unsupported extension: {0} (only .docx input is supported)")]
    UnsupportedExtension(String),
}

/// Result alias
pub type Result<T> = std::result::Result<T, Error>;

// ─────────────────────────────────────────────────────────────────────────────
// Core types
// ─────────────────────────────────────────────────────────────────────────────

/// Converter configuration
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// Pandoc markdown flavor. Default: "markdown".
    pub flavor: MarkdownFlavor,
    /// Extra pandoc arguments (inserted before -o).
    pub extra_args: Vec<String>,
    /// Don't fail if output file already exists (overwrite).
    pub overwrite: bool,
}

impl Config {
    /// Create a new config with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set markdown flavor.
    pub fn flavor(mut self, flavor: MarkdownFlavor) -> Self {
        self.flavor = flavor;
        self
    }

    /// Add an extra pandoc argument (e.g. "--wrap=none").
    pub fn extra_arg<S: Into<String>>(mut self, arg: S) -> Self {
        self.extra_args.push(arg.into());
        self
    }

    /// Allow overwriting the output file.
    pub fn overwrite(mut self) -> Self {
        self.overwrite = true;
        self
    }
}

/// Pandoc markdown output flavor.
#[derive(Debug, Clone, Copy, Default)]
pub enum MarkdownFlavor {
    /// Standard pandoc markdown (default).
    #[default]
    Markdown,
    /// MultiMarkdown.
    MarkdownMMD,
    /// CommonMark.
    CommonMark,
    /// GitHub Flavored Markdown.
    GFM,
    /// CommonMark + pipe tables.
    CommonMarkPipeTable,
}

impl MarkdownFlavor {
    fn as_arg(&self) -> &'static str {
        match self {
            MarkdownFlavor::Markdown => "markdown",
            MarkdownFlavor::MarkdownMMD => "markdown_mmd",
            MarkdownFlavor::CommonMark => "commonmark",
            MarkdownFlavor::GFM => "gfm",
            MarkdownFlavor::CommonMarkPipeTable => "commonmark+pipe_tables",
        }
    }
}

/// Markdown converter powered by pandoc.
#[derive(Debug, Clone)]
pub struct Converter {
    config: Config,
}

impl Default for Converter {
    fn default() -> Self {
        Self::new()
    }
}

impl Converter {
    /// Create a new converter with default config.
    pub fn new() -> Self {
        Self { config: Config::default() }
    }

    /// Create with custom config.
    pub fn with_config(config: Config) -> Self {
        Self { config }
    }

    /// Check if pandoc is available on this system.
    pub fn check_pandoc() -> bool {
        Command::new("pandoc")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Convert a `.docx` file to Markdown.
    ///
    /// Returns the path to the generated Markdown file.
    ///
    /// # Errors
    ///
    /// Returns `Error::PandocNotFound` if pandoc is not installed.
    /// Returns `Error::OutputExists` if the output file already exists and `overwrite` is not set.
    pub fn convert<P: AsRef<Path>>(&self, input: P, output: P) -> Result<PathBuf> {
        let input = input.as_ref();
        let output = output.as_ref().to_path_buf();

        if !input.exists() {
            return Err(Error::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("input file not found: {}", input.display()),
            )));
        }

        let ext = input
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        if !ext.eq_ignore_ascii_case("docx") {
            return Err(Error::UnsupportedExtension(ext.to_string()));
        }

        if output.exists() && !self.config.overwrite {
            return Err(Error::OutputExists(output));
        }

        if !Self::check_pandoc() {
            return Err(Error::PandocNotFound);
        }

        let mut cmd = Command::new("pandoc");
        cmd.arg(input)
            .arg("-f")
            .arg("docx")
            .arg("-t")
            .arg(self.config.flavor.as_arg());

        for extra in &self.config.extra_args {
            cmd.arg(extra.as_str());
        }

        cmd.arg("-o").arg(&output);

        let output_inner = cmd
            .output()
            .map_err(|e| Error::PandocFailed(format!("failed to spawn pandoc: {}", e)))?;

        if !output_inner.status.success() {
            let stderr = String::from_utf8_lossy(&output_inner.stderr);
            return Err(Error::PandocFailed(stderr.to_string()));
        }

        Ok(output)
    }

    /// Convert and return the markdown as a `String` instead of writing to a file.
    pub fn convert_to_string<P: AsRef<Path>>(&self, input: P) -> Result<String> {
        let input = input.as_ref();

        if !Self::check_pandoc() {
            return Err(Error::PandocNotFound);
        }

        let ext = input
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        if !ext.eq_ignore_ascii_case("docx") {
            return Err(Error::UnsupportedExtension(ext.to_string()));
        }

        let output = Command::new("pandoc")
            .arg(input)
            .arg("-f")
            .arg("docx")
            .arg("-t")
            .arg(self.config.flavor.as_arg())
            .output()
            .map_err(|e| Error::PandocFailed(format!("failed to spawn pandoc: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::PandocFailed(stderr.to_string()));
        }

        String::from_utf8(output.stdout)
            .map_err(|e| Error::PandocFailed(format!("output is not valid UTF-8: {}", e)))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Async support (feature = "async")
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "async")]
pub mod async_impl {
    //! Async conversion support via tokio.
    //!
    //! Enable with `features = ["async"]` in Cargo.toml.

    use std::path::Path;
    use tokio::process::Command;

    /// Configuration for async conversion.
    #[derive(Debug, Clone)]
    pub struct AsyncConverterConfig {
        pub flavor: crate::MarkdownFlavor,
        pub extra_args: Vec<String>,
        pub overwrite: bool,
    }

    impl Default for AsyncConverterConfig {
        fn default() -> Self {
            Self {
                flavor: crate::MarkdownFlavor::default(),
                extra_args: Vec::new(),
                overwrite: false,
            }
        }
    }

    impl AsyncConverterConfig {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn flavor(mut self, flavor: crate::MarkdownFlavor) -> Self {
            self.flavor = flavor;
            self
        }

        pub fn extra_arg<S: Into<String>>(mut self, arg: S) -> Self {
            self.extra_args.push(arg.into());
            self
        }

        pub fn overwrite(mut self) -> Self {
            self.overwrite = true;
            self
        }
    }

    /// Async converter backed by tokio's async Command.
    #[derive(Debug, Clone)]
    pub struct AsyncConverter {
        config: AsyncConverterConfig,
    }

    impl Default for AsyncConverter {
        fn default() -> Self {
            Self::new()
        }
    }

    impl AsyncConverter {
        pub fn new() -> Self {
            Self {
                config: AsyncConverterConfig::default(),
            }
        }

        pub fn with_config(config: AsyncConverterConfig) -> Self {
            Self { config }
        }

        pub async fn check_pandoc() -> bool {
            Command::new("pandoc")
                .arg("--version")
                .output()
                .await
                .map(|o| o.status.success())
                .unwrap_or(false)
        }

        pub async fn convert<P: AsRef<Path>>(
            &self,
            input: P,
            output: P,
        ) -> crate::Result<std::path::PathBuf> {
            let input = input.as_ref();
            let output = output.as_ref().to_path_buf();

            if !input.exists() {
                return Err(crate::Error::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("input file not found: {}", input.display()),
                )));
            }

            let ext = input
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");

            if !ext.eq_ignore_ascii_case("docx") {
                return Err(crate::Error::UnsupportedExtension(ext.to_string()));
            }

            if output.exists() && !self.config.overwrite {
                return Err(crate::Error::OutputExists(output));
            }

            if !Self::check_pandoc().await {
                return Err(crate::Error::PandocNotFound);
            }

            let mut cmd = Command::new("pandoc");
            cmd.arg(input)
                .arg("-f")
                .arg("docx")
                .arg("-t")
                .arg(self.config.flavor.as_arg());

            for extra in &self.config.extra_args {
                cmd.arg(extra.as_str());
            }

            cmd.arg("-o").arg(&output);

            let out = cmd.output().await.map_err(|e| {
                crate::Error::PandocFailed(format!("failed to spawn pandoc: {}", e))
            })?;

            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                return Err(crate::Error::PandocFailed(stderr.to_string()));
            }

            Ok(output)
        }

        pub async fn convert_to_string<P: AsRef<Path>>(
            &self,
            input: P,
        ) -> crate::Result<String> {
            let input = input.as_ref();

            if !Self::check_pandoc().await {
                return Err(crate::Error::PandocNotFound);
            }

            let ext = input
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");

            if !ext.eq_ignore_ascii_case("docx") {
                return Err(crate::Error::UnsupportedExtension(ext.to_string()));
            }

            let out = Command::new("pandoc")
                .arg(input)
                .arg("-f")
                .arg("docx")
                .arg("-t")
                .arg(self.config.flavor.as_arg())
                .output()
                .await
                .map_err(|e| {
                    crate::Error::PandocFailed(format!("failed to spawn pandoc: {}", e))
                })?;

            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                return Err(crate::Error::PandocFailed(stderr.to_string()));
            }

            String::from_utf8(out.stdout)
                .map_err(|e| crate::Error::PandocFailed(format!(
                    "output is not valid UTF-8: {}",
                    e
                )))
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_pandoc() {
        if Converter::check_pandoc() {
            let config = Config::new()
                .flavor(MarkdownFlavor::GFM)
                .extra_arg("--wrap=none");
            let conv = Converter::with_config(config);
            assert_eq!(conv.config.flavor.as_arg(), "gfm");
        }
    }
}
