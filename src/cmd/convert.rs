//! 本地 docx 文件转 Markdown（不依赖飞书 API）

use colored::Colorize;
use doc_converter::Converter;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

type CmdResult = std::result::Result<(), doc_converter::Error>;

/// 文件或目录转 Markdown
pub struct ConvertCommand {
    converter: Converter,
}

impl ConvertCommand {
    pub fn new() -> Self {
        Self {
            converter: Converter::new(),
        }
    }

    /// Convert a single file or all .docx files under a directory.
    pub fn run(
        &self,
        input: &Path,
        output: Option<&Path>,
        recursive: bool,
        dry_run: bool,
    ) -> CmdResult {
        if input.is_file() {
            self.convert_file(input, output, dry_run)
        } else if input.is_dir() {
            self.convert_dir(input, recursive, dry_run)
        } else {
            Err(doc_converter::Error::UnsupportedExtension(
                input.to_string_lossy().into_owned(),
            ))
        }
    }

    /// Convert a single .docx file.
    fn convert_file(
        &self,
        input: &Path,
        output: Option<&Path>,
        dry_run: bool,
    ) -> CmdResult {
        let input = input.to_path_buf();
        let md_name = input
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| format!("{}.md", s))
            .unwrap_or_else(|| "output.md".to_string());

        let output_path = output
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| {
                input.parent().unwrap_or(Path::new(".")).join(&md_name)
            });

        if dry_run {
            println!(
                "{} {} → {}",
                "[DRY]".dimmed(),
                input.display().to_string().dimmed(),
                output_path.display().to_string().dimmed()
            );
            return Ok(());
        }

        let md = self.converter.convert_file(&input)?;
        std::fs::write(&output_path, &md)
            .map_err(doc_converter::Error::OpenFile)?;

        println!("{} {}", "✓".green(), output_path.display());
        Ok(())
    }

    /// Recursively convert all .docx files in a directory.
    fn convert_dir(&self, dir: &Path, recursive: bool, dry_run: bool) -> CmdResult {
        let depth = if recursive { usize::MAX } else { 1 };

        let entries: Vec<PathBuf> = WalkDir::new(dir)
            .max_depth(depth)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().is_file()
                    && e.path()
                        .extension()
                        .and_then(|s| s.to_str())
                        .map(|s| s.eq_ignore_ascii_case("docx"))
                        .unwrap_or(false)
            })
            .map(|e| e.path().to_path_buf())
            .collect();

        if entries.is_empty() {
            println!("{}", "⚠️  目录中没有找到 .docx 文件".yellow());
            return Ok(());
        }

        let mut seen = HashSet::new();
        let unique: Vec<_> = entries
            .into_iter()
            .filter(|p| seen.insert(p.clone()))
            .collect();

        if dry_run {
            println!(
                "{} 找到 {} 个 .docx 文件（dry-run 模式）",
                "[DRY]".dimmed(),
                unique.len()
            );
        } else {
            println!("  找到 {} 个 .docx 文件", unique.len());
        }
        println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());

        let mut ok_count: usize = 0;
        for path in &unique {
            match self.convert_file(path, None, dry_run) {
                Ok(_) => ok_count += 1,
                Err(e) => {
                    println!("{} {}", "✗".red(), path.display());
                    println!("   错误: {}", e);
                }
            }
        }

        println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed());
        if !dry_run {
            println!(
                "{} 成功 {} / {}",
                "📊".cyan(),
                ok_count.to_string().green(),
                unique.len()
            );
        }
        Ok(())
    }
}
