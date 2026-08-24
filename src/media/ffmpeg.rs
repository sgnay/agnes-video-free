//! `ffmpeg` 成片组装：片段封装、concat、ASS 字幕生成与烧录。

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::media::ffprobe::{self, MediaInfo, ProbeError};
use crate::models::{Storyboard, SubtitleStyle};

/// 成片组装结果。
#[derive(Debug, Clone)]
pub struct AssembleResult {
    pub output: PathBuf,
    pub duration_sec: f64,
    pub media: MediaInfo,
}

#[derive(Debug)]
pub enum FfmpegError {
    Io(std::io::Error),
    Failed { operation: String, message: String },
    Probe(ProbeError),
    MissingInput(PathBuf),
    InvalidStoryboard(String),
}

impl std::fmt::Display for FfmpegError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "启动 ffmpeg 失败: {e}"),
            Self::Failed { operation, message } => {
                write!(f, "ffmpeg {operation} 失败: {message}")
            }
            Self::Probe(e) => write!(f, "媒体探测失败: {e}"),
            Self::MissingInput(path) => write!(f, "缺少输入媒体: {}", path.display()),
            Self::InvalidStoryboard(message) => write!(f, "storyboard 无法组装: {message}"),
        }
    }
}

impl std::error::Error for FfmpegError {}

impl From<ProbeError> for FfmpegError {
    fn from(value: ProbeError) -> Self {
        Self::Probe(value)
    }
}

/// 完整组装：每场封装 → concat → ASS 字幕烧录。
pub fn assemble_storyboard(
    storyboard: &mut Storyboard,
    audio_dir: &Path,
    video_dir: &Path,
    fonts_dir: &Path,
    output: &Path,
) -> Result<AssembleResult, FfmpegError> {
    if storyboard.scenes.is_empty() {
        return Err(FfmpegError::InvalidStoryboard("没有任何场景".to_string()));
    }
    let work_dir = output
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(".agnes-video-free");
    std::fs::create_dir_all(&work_dir).map_err(FfmpegError::Io)?;

    let mut durations = Vec::with_capacity(storyboard.scenes.len());
    let mut clips = Vec::with_capacity(storyboard.scenes.len());
    for index in 0..storyboard.scenes.len() {
        let scene = &storyboard.scenes[index];
        let audio = scene_path(
            scene.narration_audio.as_deref(),
            audio_dir,
            &scene.id,
            "mp3",
        );
        let video = scene_path(scene.motion_video.as_deref(), video_dir, &scene.id, "mp4");
        let duration = ffprobe::probe(&audio)?.duration_sec;
        let clip = work_dir.join(format!("{}.mp4", scene.id));
        if !valid_nonempty_file(&clip) {
            mux_scene(&video, &audio, &clip)?;
        }
        durations.push(duration);
        clips.push(clip);
        storyboard.scenes[index].narration_audio = Some(audio.display().to_string());
        storyboard.scenes[index].motion_video = Some(video.display().to_string());
        storyboard.scenes[index].duration_sec = Some(duration);
    }

    let ass = work_dir.join("subtitles.ass");
    write_ass(storyboard, &durations, &ass)?;
    let concat_file = work_dir.join("concat.txt");
    let merged = work_dir.join("merged.mp4");
    concat_clips(&clips, &concat_file, &merged)?;
    let result = burn_subtitles(&merged, &ass, fonts_dir, storyboard, output)?;
    for (scene, duration) in storyboard.scenes.iter_mut().zip(durations) {
        scene.num_frames = Some(crate::models::num_frames_for_duration(duration));
    }
    Ok(result)
}

fn scene_path(stored: Option<&str>, fallback_dir: &Path, id: &str, extension: &str) -> PathBuf {
    stored
        .map(PathBuf::from)
        .unwrap_or_else(|| fallback_dir.join(format!("{id}.{extension}")))
}

fn valid_nonempty_file(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.len() > 0)
        .unwrap_or(false)
}

/// 按累计时长生成 ASS 字幕文件。
pub fn write_ass(
    storyboard: &Storyboard,
    durations: &[f64],
    out: &Path,
) -> Result<(), FfmpegError> {
    if storyboard.scenes.len() != durations.len() {
        return Err(FfmpegError::InvalidStoryboard(format!(
            "场景数 {} 与时长数 {} 不一致",
            storyboard.scenes.len(),
            durations.len()
        )));
    }
    let style = subtitle_style(storyboard);
    let mut ass = ass_header(storyboard.width, storyboard.height, &style);
    let mut start = 0.0;
    for (scene, duration) in storyboard.scenes.iter().zip(durations) {
        if !duration.is_finite() || *duration <= 0.0 {
            return Err(FfmpegError::InvalidStoryboard(format!(
                "{} 的时长无效: {duration}",
                scene.id
            )));
        }
        let end = start + duration;
        let text = wrap_caption(&scene.caption, &storyboard.lang);
        ass.push_str(&format!(
            "Dialogue: 0,{}, {},Default,,0,0,0,,{}\n",
            ass_time(start),
            ass_time(end),
            text
        ));
        start = end;
    }
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(FfmpegError::Io)?;
    }
    std::fs::write(out, ass).map_err(FfmpegError::Io)
}

/// 每场把 Agnes 视频与对应旁白封装为统一编码的 clip。
pub fn mux_scene(video: &Path, audio: &Path, out: &Path) -> Result<(), FfmpegError> {
    require_file(video)?;
    require_file(audio)?;
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(FfmpegError::Io)?;
    }
    run_ffmpeg(
        "封装场景音视频",
        [
            OsStr::new("-y"),
            OsStr::new("-hide_banner"),
            OsStr::new("-loglevel"),
            OsStr::new("error"),
            OsStr::new("-i"),
            video.as_os_str(),
            OsStr::new("-i"),
            audio.as_os_str(),
            OsStr::new("-map"),
            OsStr::new("0:v:0"),
            OsStr::new("-map"),
            OsStr::new("1:a:0"),
            OsStr::new("-c:v"),
            OsStr::new("libx264"),
            OsStr::new("-preset"),
            OsStr::new("medium"),
            OsStr::new("-crf"),
            OsStr::new("18"),
            OsStr::new("-pix_fmt"),
            OsStr::new("yuv420p"),
            OsStr::new("-c:a"),
            OsStr::new("aac"),
            OsStr::new("-b:a"),
            OsStr::new("192k"),
            OsStr::new("-shortest"),
            out.as_os_str(),
        ],
    )
}

/// 用 concat demuxer 拼接所有场景 clip。
pub fn concat_clips(clips: &[PathBuf], concat_file: &Path, out: &Path) -> Result<(), FfmpegError> {
    if clips.is_empty() {
        return Err(FfmpegError::InvalidStoryboard(
            "没有可拼接的场景片段".to_string(),
        ));
    }
    let mut manifest = String::new();
    for clip in clips {
        require_file(clip)?;
        let absolute = clip.canonicalize().map_err(FfmpegError::Io)?;
        manifest.push_str("file '");
        manifest.push_str(&escape_concat_path(&absolute));
        manifest.push_str("'\n");
    }
    if let Some(parent) = concat_file.parent() {
        std::fs::create_dir_all(parent).map_err(FfmpegError::Io)?;
    }
    std::fs::write(concat_file, manifest).map_err(FfmpegError::Io)?;
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(FfmpegError::Io)?;
    }
    run_ffmpeg(
        "拼接场景片段",
        [
            OsStr::new("-y"),
            OsStr::new("-hide_banner"),
            OsStr::new("-loglevel"),
            OsStr::new("error"),
            OsStr::new("-f"),
            OsStr::new("concat"),
            OsStr::new("-safe"),
            OsStr::new("0"),
            OsStr::new("-i"),
            concat_file.as_os_str(),
            OsStr::new("-c"),
            OsStr::new("copy"),
            out.as_os_str(),
        ],
    )
}

/// 将 ASS 字幕烧录到最终视频，并统一输出画幅与 H.264/AAC 编码。
pub fn burn_subtitles(
    merged: &Path,
    ass: &Path,
    fonts_dir: &Path,
    storyboard: &Storyboard,
    out: &Path,
) -> Result<AssembleResult, FfmpegError> {
    require_file(merged)?;
    require_file(ass)?;
    if !fonts_dir.is_dir() {
        return Err(FfmpegError::MissingInput(fonts_dir.to_path_buf()));
    }
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(FfmpegError::Io)?;
    }
    let filter = format_ass_filter(ass, fonts_dir, storyboard.width, storyboard.height);
    run_ffmpeg(
        "烧录 ASS 字幕",
        [
            OsStr::new("-y"),
            OsStr::new("-hide_banner"),
            OsStr::new("-loglevel"),
            OsStr::new("error"),
            OsStr::new("-i"),
            merged.as_os_str(),
            OsStr::new("-vf"),
            OsStr::new(&filter),
            OsStr::new("-map"),
            OsStr::new("0:v:0"),
            OsStr::new("-map"),
            OsStr::new("0:a:0?"),
            OsStr::new("-c:v"),
            OsStr::new("libx264"),
            OsStr::new("-preset"),
            OsStr::new("medium"),
            OsStr::new("-crf"),
            OsStr::new("18"),
            OsStr::new("-pix_fmt"),
            OsStr::new("yuv420p"),
            OsStr::new("-c:a"),
            OsStr::new("aac"),
            OsStr::new("-b:a"),
            OsStr::new("192k"),
            OsStr::new("-movflags"),
            OsStr::new("+faststart"),
            out.as_os_str(),
        ],
    )?;

    let media = ffprobe::probe(out)?;
    if media.width != Some(storyboard.width) || media.height != Some(storyboard.height) {
        return Err(FfmpegError::InvalidStoryboard(format!(
            "最终画幅为 {:?}x{:?}，预期 {}x{}",
            media.width, media.height, storyboard.width, storyboard.height
        )));
    }
    Ok(AssembleResult {
        output: out.to_path_buf(),
        duration_sec: media.duration_sec,
        media,
    })
}

fn run_ffmpeg<'a, I>(operation: &str, args: I) -> Result<(), FfmpegError>
where
    I: IntoIterator<Item = &'a OsStr>,
{
    let output = Command::new("ffmpeg")
        .args(args)
        .output()
        .map_err(FfmpegError::Io)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(FfmpegError::Failed {
            operation: operation.to_string(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
    }
}

fn require_file(path: &Path) -> Result<(), FfmpegError> {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() && metadata.len() > 0 => Ok(()),
        _ => Err(FfmpegError::MissingInput(path.to_path_buf())),
    }
}

fn subtitle_style(storyboard: &Storyboard) -> SubtitleStyle {
    // realistic 风格均使用简洁白字+黑描边；如未来加入 crayon/textbook，
    // 可从 StyleProfile 传入完整字段。当前从画幅提供合适基准字号。
    let size = if storyboard.width >= 1000 { 48 } else { 40 };
    SubtitleStyle {
        font: "Source Han Sans SC",
        font_file: "SourceHanSansSC-Regular.otf",
        size,
        outline: 3,
        color: "&H00FFFFFF",
        outline_color: "&H00000000",
    }
}

fn ass_header(width: u32, height: u32, style: &SubtitleStyle) -> String {
    format!(
        "[Script Info]\nScriptType: v4.00+\nPlayResX: {width}\nPlayResY: {height}\nScaledBorderAndShadow: yes\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,{},{},{},{},{},&H80000000,0,0,0,0,100,100,0,0,1,{},0,2,48,48,100,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n",
        style.font, style.size, style.color, style.color, style.outline_color, style.outline,
    )
}

fn ass_time(seconds: f64) -> String {
    let total_centiseconds = (seconds.max(0.0) * 100.0).round() as u64;
    let hours = total_centiseconds / 3_600_000;
    let minutes = (total_centiseconds / 6_000) % 60;
    let secs = (total_centiseconds / 100) % 60;
    let centiseconds = total_centiseconds % 100;
    format!("{hours}:{minutes:02}:{secs:02}.{centiseconds:02}")
}

fn wrap_caption(caption: &str, lang: &str) -> String {
    let max_chars = if lang.eq_ignore_ascii_case("en") {
        42
    } else {
        16
    };
    let mut lines = Vec::new();
    let mut current = String::new();
    if lang.eq_ignore_ascii_case("en") {
        for word in caption.split_whitespace() {
            let candidate_len =
                current.chars().count() + word.chars().count() + usize::from(!current.is_empty());
            if !current.is_empty() && candidate_len > max_chars {
                lines.push(current);
                current = String::new();
            }
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
    } else {
        for ch in caption.chars() {
            if current.chars().count() >= max_chars {
                lines.push(current);
                current = String::new();
            }
            current.push(ch);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    // 先用真实换行连接，再转成 ASS 的 \\N 控制符，避免把控制符本身转义。
    escape_ass_text(&lines.join("\n"))
}

fn escape_ass_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('{', "\\{")
        .replace('}', "\\}")
        .replace('\n', "\\N")
}

fn escape_concat_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('\'', "'\\''")
}

fn format_ass_filter(ass: &Path, fonts_dir: &Path, width: u32, height: u32) -> String {
    format!(
        "scale={width}:{height}:force_original_aspect_ratio=decrease,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2,ass='{}':fontsdir='{}'",
        escape_filter_path(ass),
        escape_filter_path(fonts_dir)
    )
}

fn escape_filter_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace(':', "\\:")
        .replace('\'', "\\'")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Scene;

    fn storyboard(lang: &str) -> Storyboard {
        Storyboard {
            title: "test".to_string(),
            lang: lang.to_string(),
            style: "realistic-cinematic".to_string(),
            width: 720,
            height: 1280,
            fps: 30,
            frame_rate_video: 24,
            scenes: vec![Scene {
                id: "s01".to_string(),
                caption: "雨后的街道上，一盏灯亮了。".to_string(),
                narration: "雨后的街道上，一盏灯亮了。".to_string(),
                prompt: None,
                negative_prompt: None,
                narration_audio: None,
                motion_video: None,
                duration_sec: None,
                num_frames: None,
            }],
        }
    }

    #[test]
    fn ass_time_uses_ass_centiseconds() {
        assert_eq!(ass_time(0.0), "0:00:00.00");
        assert_eq!(ass_time(3.744), "0:00:03.74");
        assert_eq!(ass_time(61.25), "0:01:01.25");
    }

    #[test]
    fn ass_contains_header_timeline_and_escaped_text() {
        let mut sb = storyboard("zh");
        sb.scenes[0].caption = "第一行\\{不渲染标签\\}".to_string();
        let path =
            std::env::temp_dir().join(format!("agnes-video-free-{}.ass", std::process::id()));
        write_ass(&sb, &[3.744], &path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("PlayResX: 720"));
        assert!(text.contains("0:00:00.00, 0:00:03.74"));
        assert!(text.contains("第一行\\\\\\{不渲染标签\\\\\\}"));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn english_caption_wraps_at_words() {
        let mut sb = storyboard("en");
        sb.scenes[0].caption =
            "One two three four five six seven eight nine ten eleven twelve thirteen".to_string();
        let path =
            std::env::temp_dir().join(format!("agnes-video-free-en-{}.ass", std::process::id()));
        write_ass(&sb, &[2.0], &path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\\N"));
        std::fs::remove_file(path).unwrap();
    }
}
