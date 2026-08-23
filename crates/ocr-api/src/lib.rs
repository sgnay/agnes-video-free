//! OCR Engine API and Registry
//! Pure rust crate without UI dependencies.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

/// Input for OCR engine recognition
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OcrInput {
    Path(PathBuf),
    Bytes(Vec<u8>),
}

/// Options passed to the OCR engine
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcrOptions {
    pub language: String,
    pub psm: i32,
}

impl Default for OcrOptions {
    fn default() -> Self {
        Self {
            language: "eng".to_string(),
            psm: 3,
        }
    }
}

/// Output of OCR recognition
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct OcrOutput {
    pub text: String,
    pub confidence: Option<f32>,
    pub duration_ms: u128,
}

/// Errors occurring during OCR operations
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OcrError {
    EngineNotAvailable(String),
    LanguageDataMissing(String),
    InvalidInput(String),
    ExecutionFailed(String),
}

impl fmt::Display for OcrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OcrError::EngineNotAvailable(msg) => write!(f, "Engine not available: {}", msg),
            OcrError::LanguageDataMissing(msg) => write!(f, "Language data missing: {}", msg),
            OcrError::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
            OcrError::ExecutionFailed(msg) => write!(f, "Execution failed: {}", msg),
        }
    }
}

impl std::error::Error for OcrError {}

/// Core trait implemented by all OCR engines
pub trait OcrEngine: Send + Sync {
    /// Unique identifier for the engine (e.g. "tesseract")
    fn id(&self) -> &'static str;

    /// Display name for the engine (e.g. "Tesseract 5 CLI")
    fn name(&self) -> &str;

    /// Check if the engine binary and dependencies are available on the system
    fn available(&self) -> bool;

    /// Perform OCR on the given input with specified options
    fn recognize(&self, input: &OcrInput, opts: &OcrOptions) -> Result<OcrOutput, OcrError>;
}

/// Registry of available OCR engines
#[derive(Default, Clone)]
pub struct EngineRegistry {
    engines: Vec<Arc<dyn OcrEngine>>,
}

impl EngineRegistry {
    pub fn new() -> Self {
        Self { engines: Vec::new() }
    }

    pub fn register(&mut self, engine: Arc<dyn OcrEngine>) {
        self.engines.push(engine);
    }

    pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn OcrEngine>> {
        self.engines.iter()
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn OcrEngine>> {
        self.engines.iter().find(|e| e.id() == id).cloned()
    }

    pub fn default_engine(&self) -> Option<Arc<dyn OcrEngine>> {
        self.engines.iter().find(|e| e.available()).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockEngine {
        available: bool,
    }

    impl OcrEngine for MockEngine {
        fn id(&self) -> &'static str {
            "mock"
        }

        fn name(&self) -> &str {
            "Mock Engine"
        }

        fn available(&self) -> bool {
            self.available
        }

        fn recognize(&self, _input: &OcrInput, _opts: &OcrOptions) -> Result<OcrOutput, OcrError> {
            Ok(OcrOutput {
                text: "mock result".into(),
                confidence: Some(0.99),
                duration_ms: 10,
            })
        }
    }

    #[test]
    fn test_engine_registry() {
        let mut registry = EngineRegistry::new();
        let mock = Arc::new(MockEngine { available: true });
        registry.register(mock.clone());

        assert_eq!(registry.iter().count(), 1);
        assert!(registry.get("mock").is_some());
        assert!(registry.default_engine().is_some());
        assert!(registry.get("nonexistent").is_none());
    }
}
