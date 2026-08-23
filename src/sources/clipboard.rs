use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone)]
pub enum ClipboardContent {
    ImageBytes(Vec<u8>),
    FilePath(PathBuf),
    Text(String),
}

pub fn get_clipboard_content() -> Result<ClipboardContent, String> {
    // 1. Try reading PNG image data using wl-paste (Wayland) or xclip (X11)
    if let Ok(bytes) = try_get_clipboard_image() {
        if !bytes.is_empty() {
            return Ok(ClipboardContent::ImageBytes(bytes));
        }
    }

    // 2. Try reading text clipboard content
    if let Ok(text) = try_get_clipboard_text() {
        let trimmed = text.trim();
        let path = PathBuf::from(trimmed);
        if path.exists() && path.is_file() {
            return Ok(ClipboardContent::FilePath(path));
        }
        if !trimmed.is_empty() {
            return Ok(ClipboardContent::Text(trimmed.to_string()));
        }
    }

    Err("Clipboard is empty or unsupported format".to_string())
}

fn try_get_clipboard_image() -> Result<Vec<u8>, String> {
    // Try wl-paste
    if let Ok(output) = Command::new("wl-paste")
        .arg("-t")
        .arg("image/png")
        .output()
    {
        if output.status.success() && !output.stdout.is_empty() {
            return Ok(output.stdout);
        }
    }

    // Try xclip
    if let Ok(output) = Command::new("xclip")
        .arg("-selection")
        .arg("clipboard")
        .arg("-t")
        .arg("image/png")
        .arg("-o")
        .output()
    {
        if output.status.success() && !output.stdout.is_empty() {
            return Ok(output.stdout);
        }
    }

    Err("No image found on clipboard".to_string())
}

fn try_get_clipboard_text() -> Result<String, String> {
    // Try wl-paste
    if let Ok(output) = Command::new("wl-paste").arg("--no-newline").output() {
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).to_string());
        }
    }

    // Try xclip
    if let Ok(output) = Command::new("xclip")
        .arg("-selection")
        .arg("clipboard")
        .arg("-o")
        .output()
    {
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).to_string());
        }
    }

    Err("No text found on clipboard".to_string())
}

pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    use std::io::Write;

    // Try wl-copy
    if let Ok(mut child) = Command::new("wl-copy").stdin(std::process::Stdio::piped()).spawn() {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        if child.wait().map(|s| s.success()).unwrap_or(false) {
            return Ok(());
        }
    }

    // Try xclip
    if let Ok(mut child) = Command::new("xclip")
        .arg("-selection")
        .arg("clipboard")
        .stdin(std::process::Stdio::piped())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        if child.wait().map(|s| s.success()).unwrap_or(false) {
            return Ok(());
        }
    }

    Err("Failed to copy text to clipboard using wl-copy/xclip".to_string())
}
