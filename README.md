# Simple OCR Desktop Application (v2)

A fast Linux desktop OCR application built with **Rust**, **GPUI**, **Tesseract 5**, and **ksni** (StatusNotifierItem), packaged for NixOS.

## Features

1. **System Tray Integration**: Background tray service (show/hide main window, quick clipboard OCR, open file, quit).
2. **Multiple Input Sources**:
   - **Image Files**: `.png`, `.jpg`, `.jpeg`, `.tiff`, `.bmp`, `.gif`, `.webp`.
   - **PDF Files**: Multi-page PDF rasterization (via `pdftoppm` from `poppler-utils`) with page progress reporting (`Page i/N`).
   - **Clipboard Input**: Direct OCR from clipboard PNG images or clipboard file paths, with fallback to `wl-paste` / `xclip`.
3. **Pluggable Engine Architecture (`ocr-api`)**:
   - Compile-time `OcrEngine` trait & `EngineRegistry`.
   - Built-in `TesseractEngine` (`engine-tesseract`).
4. **Result Clipboard Copying**: One-click copy output text back to clipboard (`wl-copy` / `xclip`).

## NixOS Environment & Build

### Development Shell
```bash
nix develop
```

### Build Binary
```bash
nix build
```

### Run Application
```bash
nix run .#
```

### Run Tests
```bash
nix develop --command cargo test --workspace
```

## System Tray Notes
- **KDE Plasma / XFCE / Sway / Hyprland**: Supported natively out of the box via StatusNotifierItem (ksni).
- **GNOME**: GNOME Shell requires the [AppIndicator and KStatusNotifierItem Support](https://extensions.gnome.org/extension/615/appindicator-support/) extension to display system tray icons.

## Workspace Architecture

- `crates/ocr-api`: Core engine traits (`OcrEngine`, `OcrInput`, `OcrOptions`, `OcrOutput`, `OcrError`) and `EngineRegistry`.
- `crates/engine-tesseract`: Tesseract 5 CLI wrapper implementing `OcrEngine`.
- `src/`: Main GPUI desktop application, system tray integration (`tray.rs`), state management (`state.rs`), and input source handlers (`sources/`).
