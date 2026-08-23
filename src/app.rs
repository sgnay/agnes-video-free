//! Main Application View and UI Components

use gpui::{
    div, img, prelude::*, rgb, Context, FocusHandle, IntoElement, KeyDownEvent, PathPromptOptions, Window,
};
use ocr_api::{OcrEngine, OcrInput};
use std::path::PathBuf;

use crate::sources::clipboard::{copy_to_clipboard, get_clipboard_content, ClipboardContent};
use crate::sources::image::is_valid_image_file;
use crate::sources::pdf::PdfRasterizer;
use crate::state::{all_languages, OcrLanguage, OcrModel, OcrSourceType, OcrStatus, UiLanguage};

pub struct OcrAppView {
    pub model: OcrModel,
    languages: Vec<(&'static str, OcrLanguage)>,
    selected_lang_index: usize,
    path_input: String,
    copied_toast: bool,
    path_focus: Option<FocusHandle>,
}

impl OcrAppView {
    /// Return localized UI text based on the current UI language.
    fn t(&self, zh: &'static str, en: &'static str) -> &'static str {
        match self.model.ui_language {
            UiLanguage::Zh => zh,
            UiLanguage::En => en,
        }
    }

    /// Toggle between Chinese and English UI.
    fn toggle_ui_language(&mut self) {
        self.model.ui_language = match self.model.ui_language {
            UiLanguage::Zh => UiLanguage::En,
            UiLanguage::En => UiLanguage::Zh,
        };
    }

    /// Localized source-type label.
    fn source_label(&self) -> &'static str {
        match self.model.source_type {
            OcrSourceType::Image => self.t("图片文件", "Image File"),
            OcrSourceType::Pdf => self.t("PDF 文件", "PDF File"),
            OcrSourceType::Clipboard => self.t("剪贴板", "Clipboard"),
        }
    }

    pub fn new() -> Self {
        let languages = all_languages();
        Self {
            model: OcrModel::new(),
            languages,
            selected_lang_index: 0,
            path_input: String::new(),
            copied_toast: false,
            path_focus: None,
        }
    }

    pub fn set_image(&mut self, path: PathBuf) {
        self.model.set_image(path.clone());
        self.path_input = path.to_string_lossy().to_string();
    }

    pub fn set_pdf(&mut self, path: PathBuf) {
        self.model.set_pdf(path.clone(), 0);
        self.path_input = path.to_string_lossy().to_string();
    }

    pub fn load_from_input(&mut self, cx: &mut Context<Self>) {
        let trimmed = self.path_input.trim().to_string();
        if trimmed.is_empty() {
            self.model.error = Some(self.t("未输入路径", "No path entered").to_string());
            self.model.status = OcrStatus::Error(self.t("无路径", "No path").to_string());
            cx.notify();
            return;
        }

        let path = PathBuf::from(&trimmed);
        if !path.exists() {
            let msg = format!("{}: {}", self.t("文件不存在", "File not found"), trimmed);
            self.model.error = Some(msg.clone());
            self.model.status = OcrStatus::Error(msg);
            cx.notify();
            return;
        }

        let is_pdf = path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()) == Some("pdf".to_string());

        if is_pdf {
            self.set_pdf(path);
        } else if is_valid_image_file(&path) {
            self.set_image(path);
        } else {
            let msg = format!("{}: {}", self.t("不支持的格式", "Unsupported format"), trimmed);
            self.model.error = Some(msg.clone());
            self.model.status = OcrStatus::Error(msg);
        }
        cx.notify();
    }

    pub fn load_from_clipboard(&mut self, cx: &mut Context<Self>) {
        match get_clipboard_content() {
            Ok(ClipboardContent::ImageBytes(_)) => {
                self.model.set_clipboard(Some(self.t("剪贴板中的图像数据", "Image data in clipboard").to_string()));
                self.path_input = if self.model.ui_language == UiLanguage::Zh {
                    "<剪贴板图像>".to_string()
                } else {
                    "<Clipboard Image>".to_string()
                };
            }
            Ok(ClipboardContent::FilePath(path)) => {
                let path_str = path.to_string_lossy().to_string();
                self.path_input = path_str.clone();
                let is_pdf = path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()) == Some("pdf".to_string());
                if is_pdf {
                    self.model.set_pdf(path, 0);
                } else {
                    self.model.set_image(path);
                }
            }
            Ok(ClipboardContent::Text(txt)) => {
                let preview = if txt.len() > 30 {
                    format!("{}...", &txt[..30])
                } else {
                    txt
                };
                self.model.set_clipboard(Some(format!(
                    "{}: {}",
                    self.t("文本", "Text"),
                    preview
                )));
                self.path_input = if self.model.ui_language == UiLanguage::Zh {
                    "<剪贴板文本>".to_string()
                } else {
                    "<Clipboard Text>".to_string()
                };
            }
            Err(e) => {
                self.model.error = Some(e.clone());
                self.model.status = OcrStatus::Error(e);
            }
        }
        cx.notify();
    }

    pub fn trigger_ocr(&mut self, cx: &mut Context<Self>) {
        let engine = match self.model.current_engine() {
            Some(e) => e,
            None => {
                self.model.error = Some(self.t("未选择有效的 OCR 引擎", "No valid OCR engine selected").to_string());
                self.model.status = OcrStatus::Error(self.t("无引擎", "No engine").into());
                cx.notify();
                return;
            }
        };

        let opts = self.model.get_options();
        self.model.error = None;
        self.copied_toast = false;

        match self.model.source_type {
            OcrSourceType::Image => {
                let path = match &self.model.image_path {
                    Some(p) => p.clone(),
                    None => {
                        self.model.error = Some(self.t("未加载图片文件", "No image file loaded").to_string());
                        self.model.status = OcrStatus::Error(self.t("无图片", "No image").into());
                        cx.notify();
                        return;
                    }
                };

                self.model.status = OcrStatus::Processing { page: 1, total: 1 };
                cx.notify();

                cx.spawn(async move |this, cx| {
                    let res = cx
                        .background_executor()
                        .spawn(async move { engine.recognize(&OcrInput::Path(path), &opts) })
                        .await;

                    this.update(cx, |this, cx| {
                        match res {
                            Ok(output) => {
                                this.model.result = output.text;
                                this.model.status = OcrStatus::Completed;
                            }
                            Err(err) => {
                                let msg = err.to_string();
                                this.model.error = Some(msg.clone());
                                this.model.status = OcrStatus::Error(msg);
                            }
                        }
                        cx.notify();
                    })
                    .ok();
                })
                .detach();
            }
            OcrSourceType::Pdf => {
                let pdf_path = match &self.model.pdf_path {
                    Some(p) => p.clone(),
                    None => {
                        self.model.error = Some(self.t("未加载 PDF 文件", "No PDF file loaded").to_string());
                        self.model.status = OcrStatus::Error(self.t("无 PDF", "No PDF").into());
                        cx.notify();
                        return;
                    }
                };

                self.model.status = OcrStatus::Processing { page: 0, total: 0 };
                cx.notify();

                let page_label = self.t("第", "Page");
                let sep = format!("--- {} {} ---\n", page_label, "{}");
                cx.spawn(async move |this, cx| {
                    let raster_res = cx
                        .background_executor()
                        .spawn(async move { PdfRasterizer::rasterize(&pdf_path) })
                        .await;

                    let (_temp_dir, pages) = match raster_res {
                        Ok(res) => res,
                        Err(e) => {
                            this.update(cx, |this, cx| {
                                this.model.error = Some(e.clone());
                                this.model.status = OcrStatus::Error(e);
                                cx.notify();
                            })
                            .ok();
                            return;
                        }
                    };

                    let total_pages = pages.len();
                    let mut combined_text = String::new();

                    for (idx, page_path) in pages.iter().enumerate() {
                        let page_num = idx + 1;
                        let opts_clone = opts.clone();
                        let engine_clone = engine.clone();
                        let page_path_clone = page_path.clone();

                        let _ = this.update(cx, |this, cx| {
                            this.model.pdf_current_page = page_num;
                            this.model.pdf_page_count = total_pages;
                            this.model.status = OcrStatus::Processing {
                                page: page_num,
                                total: total_pages,
                            };
                            cx.notify();
                        });

                        let page_res = cx
                            .background_executor()
                            .spawn(async move {
                                engine_clone
                                    .recognize(&OcrInput::Path(page_path_clone), &opts_clone)
                            })
                            .await;

                        match page_res {
                            Ok(output) => {
                                if total_pages > 1 {
                                    combined_text.push_str(&sep.replace("{}", &page_num.to_string()));
                                }
                                combined_text.push_str(&output.text);
                                combined_text.push_str("\n\n");
                            }
                            Err(e) => {
                                combined_text.push_str(&format!(
                                    "--- Page {} (Error: {}) ---\n\n",
                                    page_num, e
                                ));
                            }
                        }
                    }

                    this.update(cx, |this, cx| {
                        this.model.result = combined_text.trim().to_string();
                        this.model.status = OcrStatus::Completed;
                        cx.notify();
                    })
                    .ok();
                })
                .detach();
            }
            OcrSourceType::Clipboard => {
                self.model.status = OcrStatus::Processing { page: 1, total: 1 };
                cx.notify();

                cx.spawn(async move |this, cx| {
                    let clip_res = cx
                        .background_executor()
                        .spawn(async move { get_clipboard_content() })
                        .await;

                    match clip_res {
                        Ok(ClipboardContent::ImageBytes(bytes)) => {
                            let res = cx
                                .background_executor()
                                .spawn(async move {
                                    engine.recognize(&OcrInput::Bytes(bytes), &opts)
                                })
                                .await;

                            this.update(cx, |this, cx| {
                                match res {
                                    Ok(output) => {
                                        this.model.result = output.text;
                                        this.model.status = OcrStatus::Completed;
                                    }
                                    Err(err) => {
                                        let msg = err.to_string();
                                        this.model.error = Some(msg.clone());
                                        this.model.status = OcrStatus::Error(msg);
                                    }
                                }
                                cx.notify();
                            })
                            .ok();
                        }
                        Ok(ClipboardContent::FilePath(path)) => {
                            let is_pdf = path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()) == Some("pdf".to_string());
                            if is_pdf {
                                this.update(cx, |this, cx| {
                                    this.model.set_pdf(path, 0);
                                    this.trigger_ocr(cx);
                                })
                                .ok();
                            } else {
                                let res = cx
                                    .background_executor()
                                    .spawn(async move {
                                        engine.recognize(&OcrInput::Path(path), &opts)
                                    })
                                    .await;

                                this.update(cx, |this, cx| {
                                    match res {
                                        Ok(output) => {
                                            this.model.result = output.text;
                                            this.model.status = OcrStatus::Completed;
                                        }
                                        Err(err) => {
                                            let msg = err.to_string();
                                            this.model.error = Some(msg.clone());
                                            this.model.status = OcrStatus::Error(msg);
                                        }
                                    }
                                    cx.notify();
                                })
                                .ok();
                            }
                        }
                        Ok(ClipboardContent::Text(txt)) => {
                            this.update(cx, |this, cx| {
                                this.model.error = Some(
                                    this.t("剪贴板内容是文本", "Clipboard content is text").to_string(),
                                );
                                this.model.result = txt;
                                this.model.status = OcrStatus::Completed;
                                cx.notify();
                            })
                            .ok();
                        }
                        Err(e) => {
                            this.update(cx, |this, cx| {
                                this.model.error = Some(e.clone());
                                this.model.status = OcrStatus::Error(e);
                                cx.notify();
                            })
                            .ok();
                        }
                    }
                })
                .detach();
            }
        }
    }

    fn copy_result(&mut self, cx: &mut Context<Self>) {
        if !self.model.result.is_empty() {
            if copy_to_clipboard(&self.model.result).is_ok() {
                self.copied_toast = true;
                cx.notify();
            }
        }
    }

    fn cycle_language(&mut self) {
        if self.languages.is_empty() {
            return;
        }
        self.selected_lang_index = (self.selected_lang_index + 1) % self.languages.len();
        self.model.language = self.languages[self.selected_lang_index].1.clone();
    }

    fn cycle_engine(&mut self) {
        let engines: Vec<String> = self.model.registry.iter().map(|e| e.id().to_string()).collect();
        if engines.is_empty() {
            return;
        }
        if let Some(pos) = engines.iter().position(|id| id == &self.model.selected_engine_id) {
            let next_idx = (pos + 1) % engines.len();
            self.model.selected_engine_id = engines[next_idx].clone();
        } else {
            self.model.selected_engine_id = engines[0].clone();
        }
    }

    fn browse_for_path(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select an image or PDF".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await {
                if let Some(path) = paths.into_iter().next() {
                    this.update(cx, |this, cx| {
                        this.path_input = path.to_string_lossy().to_string();
                        let is_pdf = path
                            .extension()
                            .and_then(|e| e.to_str())
                            .map(|e| e.to_lowercase())
                            == Some("pdf".to_string());
                        if is_pdf {
                            this.model.set_pdf(path, 0);
                        } else {
                            this.model.set_image(path);
                        }
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    fn handle_path_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        // Ignore keybindings with modifiers like ctrl/cmd/alt.
        if keystroke.modifiers.control
            || keystroke.modifiers.alt
            || keystroke.modifiers.platform
            || keystroke.modifiers.function
        {
            return;
        }
        if let Some(ch) = &keystroke.key_char {
            self.path_input.push_str(ch);
        } else {
            match keystroke.key.as_str() {
                "backspace" => {
                    self.path_input.pop();
                }
                "space" => {
                    self.path_input.push(' ');
                }
                "enter" => {
                    self.load_from_input(cx);
                    return;
                }
                _ => {}
            }
        }
        cx.notify();
    }
}

impl Default for OcrAppView {
    fn default() -> Self {
        Self::new()
    }
}

pub fn perform_quick_clipboard_ocr() -> Result<String, String> {
    let content = get_clipboard_content()?;
    let engine = engine_tesseract::TesseractEngine::new();
    let opts = ocr_api::OcrOptions {
        language: "chi_sim".to_string(),
        psm: 3,
    };

    let result_text = match content {
        ClipboardContent::ImageBytes(bytes) => {
            let output = engine.recognize(&OcrInput::Bytes(bytes), &opts).map_err(|e| e.to_string())?;
            output.text
        }
        ClipboardContent::FilePath(path) => {
            let is_pdf = path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()) == Some("pdf".to_string());
            if is_pdf {
                let (_temp_dir, pages) = PdfRasterizer::rasterize(&path)?;
                let mut combined = String::new();
                for (idx, page_path) in pages.iter().enumerate() {
                    let out = engine.recognize(&OcrInput::Path(page_path.clone()), &opts).map_err(|e| e.to_string())?;
                    if pages.len() > 1 {
                        combined.push_str(&format!("--- Page {} ---\n", idx + 1));
                    }
                    combined.push_str(&out.text);
                    combined.push_str("\n\n");
                }
                combined
            } else {
                let output = engine.recognize(&OcrInput::Path(path), &opts).map_err(|e| e.to_string())?;
                output.text
            }
        }
        ClipboardContent::Text(t) => t,
    };

    let trimmed = result_text.trim().to_string();
    if !trimmed.is_empty() {
        copy_to_clipboard(&trimmed)?;
    }
    Ok(trimmed)
}

impl Render for OcrAppView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let is_processing = matches!(self.model.status, OcrStatus::Processing { .. });
        let engine_name = self
            .model
            .current_engine()
            .map(|e| e.name().to_string())
            .unwrap_or_else(|| self.t("无引擎", "No Engine").to_string());

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x1e1e1e))
            // Header bar
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px_4()
                    .py_2()
                    .bg(rgb(0x2d2d30))
                    .border_b_1()
                    .border_color(rgb(0x404040))                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(gpui::white())
                                    .child(if self.model.ui_language == UiLanguage::Zh {
                                        "简单 OCR"
                                    } else {
                                        "Simple OCR v2"
                                    }),
                            )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .items_center()
                            // UI language toggle
                            .child(
                                div()
                                    .id("ui-lang-btn")
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .bg(rgb(0x3a3a3d))
                                    .on_click(_cx.listener(|this, _event, _window, cx| {
                                        this.toggle_ui_language();
                                        cx.notify();
                                    }))
                                    .child(if self.model.ui_language == UiLanguage::Zh {
                                        "EN"
                                    } else {
                                        "中文"
                                    }),
                            )
                            // Source type tab
                            .child(
                                div()
                                    .id("source-btn")
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .bg(rgb(0x3a3a3d))
                                    .on_click(_cx.listener(|this, _event, _window, cx| {
                                        match this.model.source_type {
                                            OcrSourceType::Image => this.model.source_type = OcrSourceType::Pdf,
                                            OcrSourceType::Pdf => this.model.source_type = OcrSourceType::Clipboard,
                                            OcrSourceType::Clipboard => this.model.source_type = OcrSourceType::Image,
                                        }
                                        cx.notify();
                                    }))
                                    .child(format!("{}: {}", self.t("来源", "Source"), self.source_label())),
                            )
                            // Engine selector
                            .child(
                                div()
                                    .id("engine-select")
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .bg(rgb(0x3a3a3d))
                                    .on_click(_cx.listener(|this, _event, _window, cx| {
                                        this.cycle_engine();
                                        cx.notify();
                                    }))
                                    .child(format!("{}: {}", self.t("引擎", "Engine"), engine_name)),
                            )
                            // OCR language selector
                            .child(
                                div()
                                    .id("lang-select")
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .bg(rgb(0x3a3a3d))
                                    .on_click(_cx.listener(|this, _event, _window, cx| {
                                        this.cycle_language();
                                        cx.notify();
                                    }))
                                    .child(format!("{}: {}", self.t("识别语言", "Lang"), self.model.language)),
                            )
                            // Recognize button
                            .child(
                                div()
                                    .id("ocr-btn")
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .bg(if is_processing {
                                        rgb(0x505050)
                                    } else {
                                        rgb(0x007acc)
                                    })
                                    .on_click(_cx.listener(|this, _event, _window, cx| {
                                        this.trigger_ocr(cx);
                                    }))
                                    .child(match &self.model.status {
                                        OcrStatus::Processing { page, total } if *total > 1 => {
                                            format!("{} ({}/{})", self.t("识别中", "Processing"), page, total)
                                        }
                                        OcrStatus::Processing { .. } => {
                                            self.t("识别中...", "Processing...").to_string()
                                        }
                                        _ => self.t("识别", "Recognize").to_string(),
                                    }),
                            ),
                    ),
            )
            // Input control bar
            .child({
                let path_focus = self.path_focus.get_or_insert_with(|| _cx.focus_handle());
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .p_2()
                    .bg(rgb(0x252526))
                    .border_b_1()
                    .border_color(rgb(0x404040))
                    .child(
                        div()
                            .id("path-input")
                            .flex_1()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(0x1a1a1a))
                            .text_sm()
                            .text_color(rgb(0xcccccc))
                            .track_focus(path_focus)
                            .focus(|s| s.border_color(rgb(0x007acc)))
                            .on_click(_cx.listener(|this, _event, window, cx| {
                                if let Some(handle) = &this.path_focus {
                                    handle.focus(window, cx);
                                }
                            }))
                            .on_key_down(_cx.listener(|this, event, _window, cx| {
                                this.handle_path_key(event, cx);
                            }))
                            .child(if self.path_input.is_empty() {
                                div()
                                    .text_color(rgb(0x606060))
                                    .child(self.t("输入路径或点击浏览...", "Enter path or press Browse..."))
                            } else {
                                div().child(self.path_input.clone())
                            }),
                    )
                    .child(
                        div()
                            .id("browse-btn")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(0x8250df))
                            .on_click(_cx.listener(|this, _event, _window, cx| {
                                this.browse_for_path(cx);
                            }))
                            .child(self.t("浏览...", "Browse...")),
                    )
                    .child(
                        div()
                            .id("load-btn")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(0x007acc))
                            .on_click(_cx.listener(|this, _event, _window, cx| {
                                this.load_from_input(cx);
                            }))
                            .child(self.t("加载文件", "Load File")),
                    )
                    .child(
                        div()
                            .id("paste-btn")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(0x28a745))
                            .on_click(_cx.listener(|this, _event, _window, cx| {
                                this.load_from_clipboard(cx);
                            }))
                            .child(self.t("粘贴剪贴板", "Paste Clipboard")),
                    )
            })
            // Main content body
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .size_full()
                    .gap_2()
                    .p_2()
                    // Preview side panel
                    .child(
                        div()
                            .w_48()
                            .flex()
                            .flex_col()
                            .items_center()
                            .child(
                                div()
                                    .w_full()
                                    .h_48()
                                    .rounded_md()
                                    .bg(rgb(0x303030))
                                    .border_1()
                                    .border_color(rgb(0x505050))
                                    .items_center()
                                    .justify_center()
                                    .overflow_hidden()
                                    .child(match self.model.source_type {
                                        OcrSourceType::Image => {
                                            if let Some(ref path) = self.model.image_path {
                                                div().child(
                                                    img(path.as_path())
                                                        .w_full()
                                                        .h_full()
                                                        .object_fit(gpui::ObjectFit::Contain),
                                                )
                                            } else {
                                                div()
                                                    .text_color(rgb(0x808080))
                                                    .text_sm()
                                                    .child(self.t("未加载图片", "No image loaded"))
                                            }
                                        }
                                        OcrSourceType::Pdf => div()
                                            .flex()
                                            .flex_col()
                                            .items_center()
                                            .justify_center()
                                            .text_color(rgb(0xaaaaaa))
                                            .text_sm()
                                            .child(self.t("PDF 文档", "PDF Document"))
                                            .child(if self.model.pdf_page_count > 0 {
                                                format!(
                                                    "{} {}",
                                                    self.model.pdf_page_count,
                                                    self.t("页", "pages")
                                                )
                                            } else {
                                                self.t("准备处理", "Ready to process").to_string()
                                            }),
                                        OcrSourceType::Clipboard => div()
                                            .flex()
                                            .flex_col()
                                            .items_center()
                                            .justify_center()
                                            .p_2()
                                            .text_color(rgb(0xaaaaaa))
                                            .text_xs()
                                            .child(
                                                self.model
                                                    .clipboard_preview_text
                                                    .clone()
                                                    .unwrap_or_else(|| {
                                                        self.t("剪贴板来源", "Clipboard Source").to_string()
                                                    }),
                                            ),
                                    }),
                            )
                            .child(
                                div()
                                    .mt_1()
                                    .text_xs()
                                    .text_color(rgb(0xaaaaaa))
                                    .truncate()
                                    .child(match &self.model.status {
                                        OcrStatus::Processing { page, total } => {
                                            if *total > 0 {
                                                format!(
                                                    "{} {} / {}",
                                                    self.t("第", "Page"),
                                                    page,
                                                    total
                                                )
                                            } else {
                                                self.t("识别中...", "Processing...").to_string()
                                            }
                                        }
                                        OcrStatus::Completed => self.t("已完成", "Completed").to_string(),
                                        OcrStatus::Idle => self.t("空闲", "Idle").to_string(),
                                        OcrStatus::Error(e) => {
                                            format!("{}: {}", self.t("错误", "Error"), e)
                                        }
                                    }),
                            ),
                    )
                    // Results side panel
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .size_full()
                            .rounded_md()
                            .bg(rgb(0x303030))
                            .border_1()
                            .border_color(rgb(0x505050))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .justify_between()
                                    .items_center()
                                    .px_3()
                                    .py_2()
                                    .border_b_1()
                                    .border_color(rgb(0x505050))
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(gpui::white())
                                            .child(self.t("OCR 识别结果", "OCR Output Result")),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .gap_2()
                                            .items_center()
                                            .child(
                                                div()
                                                    .id("copy-btn")
                                                    .px_2()
                                                    .py_1()
                                                    .rounded_md()
                                                    .bg(rgb(0x007acc))
                                                    .text_xs()
                                                    .on_click(_cx.listener(|this, _event, _window, cx| {
                                                        this.copy_result(cx);
                                                    }))
                                                    .child(if self.copied_toast {
                                                        self.t("已复制！", "Copied!")
                                                    } else {
                                                        self.t("复制文本", "Copy Text")
                                                    }),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(0x888888))
                                                    .child(match &self.model.status {
                                                        OcrStatus::Idle => self.t("就绪", "Ready").to_string(),
                                                        OcrStatus::Processing { page, total } => {
                                                            if *total > 1 {
                                                                format!("{} {}/{}", self.t("页", "Page"), page, total)
                                                            } else {
                                                                self.t("识别中...", "Processing...").to_string()
                                                            }
                                                        }
                                                        OcrStatus::Completed => self.t("完成", "Done").to_string(),
                                                        OcrStatus::Error(_) => self.t("错误", "Error").to_string(),
                                                    }),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .id("ocr-results")
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .size_full()
                                    .p_3()
                                    .overflow_y_scroll()
                                    .text_base()
                                    .text_color(gpui::white())
                                    .child(if self.model.result.is_empty() {
                                        div()
                                            .text_color(rgb(0x606060))
                                            .child(if matches!(self.model.status, OcrStatus::Error(_)) {
                                                self.model.error.clone().unwrap_or_default()
                                            } else {
                                                self.t("识别出的文本将显示在这里...", "Extracted text will appear here...").to_string()
                                            })
                                    } else {
                                        div()
                                            .whitespace_normal()
                                            .child(self.model.result.clone())
                                    }),
                            ),
                    ),
            )
    }
}