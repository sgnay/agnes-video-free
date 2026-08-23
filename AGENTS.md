# Repository Guidelines

## Project Overview
Simple OCR desktop application using Rust and GPUI framework, targeting NixOS. Implements image recognition via Tesseract CLI with GPU acceleration through Vulkan/WGPU.

## Architecture & Data Flow
```
[Main Window] -> [OCR Engine] -> [Tesseract CLI] -> [OCR Results]
           |                         ^
           v                         |
     [State Management] <--- [Async Task]
```
- UI built with GPUI (Zed's internal framework)
- OCR processing uses `spawn_blocking` for async tasks
- State managed via `Entity<Model>` pattern
- Tesseract CLI integration with language selection

## Key Directories
- `src/`: Core implementation (main.rs, app.rs, ocr_engine.rs, state.rs)
- `flake.nix`: NixOS build configuration
- `Cargo.toml`: Rust project metadata

## Development Commands
```bash
# Build & run
nix run .#

# Build package
nix build .

# Enter dev shell
nix shell

# Run tests
cargo test
```

## Code Conventions & Patterns
- Async: GPUI's `Task<T, E>` + `spawn_blocking` for CPU-bound work
- State: `Entity<Model>` with `cx.notify()` for re-renders
- Error Handling: `OcrResult` enum with explicit error messages
- Language: Rust 2021 edition with `serde` for serialization
- Dependencies: Tesseract 5 CLI, Vulkan drivers

## Important Files
- `src/main.rs`: Application entry point
- `src/app.rs`: UI layout and interaction logic
- `src/ocr_engine.rs`: Tesseract CLI wrapper
- `src/state.rs`: Application state management
- `flake.nix`: NixOS build and runtime configuration
- `PLAN.md`: Project architecture and roadmap

## Runtime/Tooling Preferences
- Required: NixOS, Rust toolchain, Tesseract 5, Vulkan drivers
- Build: Nix flakes
- Packaging: Native NixOS package
- No web dependencies

## Testing & QA
- Unit tests for OCR engine (mock Tesseract commands)
- Integration tests via `nix run` validation
- Manual testing required for GPUI UI components
- Tesseract language data included via Nixpkgs
