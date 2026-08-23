//! Tesseract CLI implementation of OcrEngine

use ocr_api::{OcrEngine, OcrError, OcrInput, OcrOptions, OcrOutput};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;
use tempfile::NamedTempFile;

pub struct TesseractEngine {
    tesseract_path: PathBuf,
}

impl TesseractEngine {
    pub fn new() -> Self {
        Self {
            tesseract_path: PathBuf::from("tesseract"),
        }
    }

    pub fn with_path(path: impl Into<PathBuf>) -> Self {
        Self {
            tesseract_path: path.into(),
        }
    }

    pub fn get_version(&self) -> Option<String> {
        let output = Command::new(&self.tesseract_path)
            .arg("--version")
            .output()
            .ok()?;

        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            None
        }
    }

    fn run_tesseract(&self, image_path: &Path, opts: &OcrOptions) -> Result<OcrOutput, OcrError> {
        if !image_path.exists() {
            return Err(OcrError::InvalidInput(format!(
                "Image file not found: {}",
                image_path.display()
            )));
        }

        let start = Instant::now();
        let output = Command::new(&self.tesseract_path)
            .arg(image_path)
            .arg("stdout")
            .arg("-l")
            .arg(&opts.language)
            .arg("--psm")
            .arg(opts.psm.to_string())
            .output();

        let elapsed = start.elapsed().as_millis();

        match output {
            Ok(out) => {
                if out.status.success() {
                    let text = String::from_utf8_lossy(&out.stdout).to_string();
                    Ok(OcrOutput {
                        text,
                        confidence: None,
                        duration_ms: elapsed,
                    })
                } else {
                    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                    Err(OcrError::ExecutionFailed(format!(
                        "Tesseract error: {}",
                        stderr
                    )))
                }
            }
            Err(e) => Err(OcrError::ExecutionFailed(format!(
                "Failed to execute tesseract: {}",
                e
            ))),
        }
    }
}

impl Default for TesseractEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl OcrEngine for TesseractEngine {
    fn id(&self) -> &'static str {
        "tesseract"
    }

    fn name(&self) -> &str {
        "Tesseract 5 CLI"
    }

    fn available(&self) -> bool {
        Command::new(&self.tesseract_path)
            .arg("--version")
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false)
    }

    fn recognize(&self, input: &OcrInput, opts: &OcrOptions) -> Result<OcrOutput, OcrError> {
        if !self.available() {
            return Err(OcrError::EngineNotAvailable(
                "Tesseract executable not found or failed --version check".to_string(),
            ));
        }

        match input {
            OcrInput::Path(path) => self.run_tesseract(path, opts),
            OcrInput::Bytes(bytes) => {
                let mut temp_file = NamedTempFile::new().map_err(|e| {
                    OcrError::ExecutionFailed(format!("Failed to create temp file: {}", e))
                })?;
                temp_file.write_all(bytes).map_err(|e| {
                    OcrError::ExecutionFailed(format!("Failed to write to temp file: {}", e))
                })?;
                let temp_path = temp_file.path();
                self.run_tesseract(temp_path, opts)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tesseract_engine_new() {
        let engine = TesseractEngine::new();
        assert_eq!(engine.tesseract_path, PathBuf::from("tesseract"));
        assert_eq!(engine.id(), "tesseract");
        assert_eq!(engine.name(), "Tesseract 5 CLI");
    }

    #[test]
    fn test_tesseract_engine_with_path() {
        let engine = TesseractEngine::with_path("/usr/bin/tesseract");
        assert_eq!(engine.tesseract_path, PathBuf::from("/usr/bin/tesseract"));
    }
}
