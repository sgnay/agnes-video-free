//! `ffprobe` 封装：获取音频时长以及视频流尺寸/时长。

use std::path::Path;
use std::process::Command;

/// ffprobe 的媒体信息。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaInfo {
    pub duration_sec: f64,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug)]
pub enum ProbeError {
    Io(std::io::Error),
    Failed { path: String, message: String },
    InvalidJson { path: String, message: String },
    InvalidDuration { path: String, value: String },
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "启动 ffprobe 失败: {e}"),
            Self::Failed { path, message } => write!(f, "ffprobe 检查 {} 失败: {message}", path),
            Self::InvalidJson { path, message } => {
                write!(f, "ffprobe 输出 {} 无法解析: {message}", path)
            }
            Self::InvalidDuration { path, value } => {
                write!(f, "ffprobe 输出 {} 的时长无效: {value}", path)
            }
        }
    }
}

impl std::error::Error for ProbeError {}

#[derive(Debug, serde::Deserialize)]
struct RawProbe {
    #[serde(default)]
    streams: Vec<RawStream>,
    #[serde(default)]
    format: RawFormat,
}

#[derive(Debug, serde::Deserialize)]
struct RawStream {
    codec_type: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    duration: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct RawFormat {
    duration: Option<String>,
}

/// 探测媒体文件。
pub fn probe(path: &Path) -> Result<MediaInfo, ProbeError> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_entries",
            "format=duration:stream=codec_type,width,height,duration",
        ])
        .arg(path)
        .output()
        .map_err(ProbeError::Io)?;

    let display_path = path.display().to_string();
    if !output.status.success() {
        return Err(ProbeError::Failed {
            path: display_path,
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    let raw: RawProbe =
        serde_json::from_slice(&output.stdout).map_err(|e| ProbeError::InvalidJson {
            path: display_path.clone(),
            message: e.to_string(),
        })?;
    let duration_value = raw
        .format
        .duration
        .or_else(|| raw.streams.iter().find_map(|s| s.duration.clone()))
        .unwrap_or_default();
    let duration_sec = duration_value
        .parse::<f64>()
        .map_err(|_| ProbeError::InvalidDuration {
            path: display_path.clone(),
            value: duration_value,
        })?;
    if !duration_sec.is_finite() || duration_sec <= 0.0 {
        return Err(ProbeError::InvalidDuration {
            path: display_path,
            value: duration_sec.to_string(),
        });
    }

    let video = raw
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("video"));
    Ok(MediaInfo {
        duration_sec,
        width: video.and_then(|s| s.width),
        height: video.and_then(|s| s.height),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_info_has_expected_shape() {
        let info = MediaInfo {
            duration_sec: 3.5,
            width: Some(720),
            height: Some(1280),
        };
        assert_eq!(info.duration_sec, 3.5);
        assert_eq!((info.width, info.height), (Some(720), Some(1280)));
    }
}
