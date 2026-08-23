use std::path::{Path, PathBuf};

pub fn is_valid_image_file(path: &Path) -> bool {
    if !path.exists() || !path.is_file() {
        return false;
    }
    let valid_extensions = ["png", "jpg", "jpeg", "tiff", "bmp", "gif", "webp"];
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| valid_extensions.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

pub fn get_image_path(path: &Path) -> Result<PathBuf, String> {
    if !path.exists() {
        return Err(format!("File does not exist: {}", path.display()));
    }
    if !is_valid_image_file(path) {
        return Err(format!("Unsupported image format: {}", path.display()));
    }
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_image_file() {
        assert!(!is_valid_image_file(Path::new("nonexistent.png")));
        let res_eng = Path::new("resources/eng.png");
        if res_eng.exists() {
            assert!(is_valid_image_file(res_eng));
            assert!(get_image_path(res_eng).is_ok());
        }
    }
}
