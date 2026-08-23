use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

#[derive(Debug)]
pub struct PdfRasterizer;

impl PdfRasterizer {
    /// Check if pdftoppm CLI is available
    pub fn is_available() -> bool {
        Command::new("pdftoppm")
            .arg("-v")
            .output()
            .is_ok()
    }

    /// Rasterize PDF file into PNG images in temp directory.
    /// Returns TempDir (holding the images) and a sorted list of page image PathBufs.
    pub fn rasterize(pdf_path: &Path) -> Result<(TempDir, Vec<PathBuf>), String> {
        if !pdf_path.exists() {
            return Err(format!("PDF file not found: {}", pdf_path.display()));
        }

        let temp_dir = TempDir::new().map_err(|e| format!("Failed to create temp dir: {}", e))?;
        let output_prefix = temp_dir.path().join("page");

        let output = Command::new("pdftoppm")
            .arg("-png")
            .arg("-r")
            .arg("200")
            .arg(pdf_path)
            .arg(&output_prefix)
            .output()
            .map_err(|e| format!("Failed to run pdftoppm (poppler-utils installed?): {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("pdftoppm failed: {}", stderr));
        }

        let mut entries: Vec<(usize, PathBuf)> = Vec::new();
        let read_dir = std::fs::read_dir(temp_dir.path())
            .map_err(|e| format!("Failed to read temp directory: {}", e))?;

        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("png") {
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    let page_num = file_name
                        .trim_start_matches("page-")
                        .trim_end_matches(".png")
                        .parse::<usize>()
                        .unwrap_or(0);
                    entries.push((page_num, path));
                }
            }
        }

        entries.sort_by_key(|(page_num, _)| *page_num);
        let paths: Vec<PathBuf> = entries.into_iter().map(|(_, path)| path).collect();

        if paths.is_empty() {
            return Err("No pages were rasterized from PDF".to_string());
        }

        Ok((temp_dir, paths))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pdf_rasterizer_availability() {
        assert!(PdfRasterizer::is_available());
    }

    #[test]
    fn test_nonexistent_pdf() {
        let result = PdfRasterizer::rasterize(Path::new("nonexistent.pdf"));
        assert!(result.is_err());
    }
}
