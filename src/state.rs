//! OCR App State Model
//! Manages input sources, engine registry, language selection, OCR results, and progress status.

use engine_tesseract::TesseractEngine;
use ocr_api::{EngineRegistry, OcrEngine, OcrOptions};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

/// Input source types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OcrSourceType {
    Image,
    Pdf,
    Clipboard,
}

impl std::fmt::Display for OcrSourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OcrSourceType::Image => write!(f, "Image File"),
            OcrSourceType::Pdf => write!(f, "PDF File"),
            OcrSourceType::Clipboard => write!(f, "Clipboard"),
        }
    }
}

/// UI display language
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiLanguage {
    /// Simplified Chinese (default)
    Zh,
    /// English
    En,
}

impl Default for UiLanguage {
    fn default() -> Self {
        UiLanguage::Zh
    }
}

/// Available OCR languages
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OcrLanguage {
    Eng,
    ChiSim,
    ChiTra,
    Jpn,
    Kor,
    Sp,
    Fra,
    Deu,
    Ita,
}

impl std::fmt::Display for OcrLanguage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OcrLanguage::Eng => write!(f, "eng"),
            OcrLanguage::ChiSim => write!(f, "chi_sim"),
            OcrLanguage::ChiTra => write!(f, "chi_tra"),
            OcrLanguage::Jpn => write!(f, "jpn"),
            OcrLanguage::Kor => write!(f, "kor"),
            OcrLanguage::Sp => write!(f, "spa"),
            OcrLanguage::Fra => write!(f, "fra"),
            OcrLanguage::Deu => write!(f, "deu"),
            OcrLanguage::Ita => write!(f, "ita"),
        }
    }
}

impl Default for OcrLanguage {
    fn default() -> Self {
        OcrLanguage::ChiSim
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OcrStatus {
    Idle,
    Processing { page: usize, total: usize },
    Completed,
    Error(String),
}

/// Application state for OCR
#[derive(Clone)]
pub struct OcrModel {
    pub source_type: OcrSourceType,
    pub image_path: Option<PathBuf>,
    pub pdf_path: Option<PathBuf>,
    pub pdf_page_count: usize,
    pub pdf_current_page: usize,
    pub clipboard_preview_text: Option<String>,

    pub language: OcrLanguage,
    pub ui_language: UiLanguage,
    pub selected_engine_id: String,
    pub registry: EngineRegistry,

    pub result: String,
    pub status: OcrStatus,
    pub error: Option<String>,
}

impl Default for OcrModel {
    fn default() -> Self {
        let mut registry = EngineRegistry::new();
        registry.register(Arc::new(TesseractEngine::new()));

        let default_engine_id = registry
            .default_engine()
            .map(|e| e.id().to_string())
            .unwrap_or_else(|| "tesseract".to_string());

        Self {
            source_type: OcrSourceType::Image,
            image_path: None,
            pdf_path: None,
            pdf_page_count: 0,
            pdf_current_page: 0,
            clipboard_preview_text: None,

            language: OcrLanguage::default(),
            ui_language: UiLanguage::default(),
            selected_engine_id: default_engine_id,
            registry,

            result: String::new(),
            status: OcrStatus::Idle,
            error: None,
        }
    }
}

impl OcrModel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_image(&mut self, path: PathBuf) {
        self.source_type = OcrSourceType::Image;
        self.image_path = Some(path);
        self.pdf_path = None;
        self.result.clear();
        self.error = None;
        self.status = OcrStatus::Idle;
    }

    pub fn set_pdf(&mut self, path: PathBuf, page_count: usize) {
        self.source_type = OcrSourceType::Pdf;
        self.pdf_path = Some(path);
        self.pdf_page_count = page_count;
        self.pdf_current_page = 1;
        self.image_path = None;
        self.result.clear();
        self.error = None;
        self.status = OcrStatus::Idle;
    }

    pub fn set_clipboard(&mut self, text_preview: Option<String>) {
        self.source_type = OcrSourceType::Clipboard;
        self.clipboard_preview_text = text_preview;
        self.image_path = None;
        self.pdf_path = None;
        self.result.clear();
        self.error = None;
        self.status = OcrStatus::Idle;
    }

    pub fn get_options(&self) -> OcrOptions {
        OcrOptions {
            language: self.language.to_string(),
            psm: 3,
        }
    }

    pub fn current_engine(&self) -> Option<Arc<dyn OcrEngine>> {
        self.registry.get(&self.selected_engine_id)
    }
}

pub fn all_languages() -> Vec<(&'static str, OcrLanguage)> {
    vec![
        ("Chinese (Simplified)", OcrLanguage::ChiSim),
        ("Chinese (Traditional)", OcrLanguage::ChiTra),
        ("English", OcrLanguage::Eng),
        ("Japanese", OcrLanguage::Jpn),
        ("Korean", OcrLanguage::Kor),
        ("Spanish", OcrLanguage::Sp),
        ("French", OcrLanguage::Fra),
        ("German", OcrLanguage::Deu),
        ("Italian", OcrLanguage::Ita),
    ]
}