//! ffmpeg 成片组装：视觉片段拼接、独立音轨混音与字幕烧录。

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::media::ffprobe::{self, MediaInfo, ProbeError};
use crate::models::{Storyboard, SubtitleStyle};

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
            Self::Io(error) => write!(f, "启动媒体工具失败: {error}"),
            Self::Failed { operation, message } => write!(f, "ffmpeg {operation} 失败: {message}"),
            Self::Probe(error) => write!(f, "媒体探测失败: {error}"),
            Self::MissingInput(path) => write!(f, "缺少输入媒体: {}", path.display()),
            Self::InvalidStoryboard(message) => write!(f, "storyboard 无法组装: {message}"),
        }
    }
}

impl std::error::Error for FfmpegError {}

impl From<ProbeError> for FfmpegError {
    fn from(error: ProbeError) -> Self {
        Self::Probe(error)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AudioTracks<'a> {
    pub audio: Option<&'a Path>,
    pub bgm: Option<&'a Path>,
    pub subtitles: Option<&'a Path>,
}

/// 拼接 storyboard 中的视觉视频，再叠加独立音轨、BGM 和字幕。
pub fn assemble_storyboard_with_tracks(
    storyboard: &mut Storyboard,
    video_dir: &Path,
    fonts_dir: &Path,
    tracks: AudioTracks<'_>,
    output: &Path,
) -> Result<AssembleResult, FfmpegError> {
    if storyboard.scenes.is_empty() {
        return Err(FfmpegError::InvalidStoryboard(
            "没有任何视觉场景".to_string(),
        ));
    }
    let work_dir = output
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(".agnes-video-free");
    std::fs::create_dir_all(&work_dir).map_err(FfmpegError::Io)?;

    let videos: Vec<PathBuf> = storyboard
        .scenes
        .iter()
        .map(|scene| {
            let path = scene_path(scene.motion_video.as_deref(), video_dir, &scene.id);
            require_file(&path).map(|()| path)
        })
        .collect::<Result<_, _>>()?;
    let concat_file = work_dir.join("visual-concat.txt");
    let merged = work_dir.join("visual-merged.mp4");
    concat_clips(&videos, &concat_file, &merged)?;
    let duration = ffprobe::probe(&merged)?.duration_sec;

    if let Some(path) = tracks.audio {
        require_file(path)?;
    }
    if let Some(path) = tracks.bgm {
        require_file(path)?;
    }
    let ass = tracks
        .subtitles
        .map(|path| {
            let ass = work_dir.join("external-subtitles.ass");
            write_external_ass(path, storyboard, duration, &ass).map(|()| ass)
        })
        .transpose()?;
    let result = render_with_tracks(
        &merged,
        ass.as_deref(),
        fonts_dir,
        storyboard,
        tracks,
        duration,
        output,
    )?;
    for (scene, video) in storyboard.scenes.iter_mut().zip(videos) {
        scene.motion_video = Some(video.display().to_string());
    }
    Ok(result)
}

fn render_with_tracks(
    merged: &Path,
    ass: Option<&Path>,
    fonts_dir: &Path,
    storyboard: &Storyboard,
    tracks: AudioTracks<'_>,
    duration: f64,
    output: &Path,
) -> Result<AssembleResult, FfmpegError> {
    require_file(merged)?;
    if ass.is_some() && !fonts_dir.is_dir() {
        return Err(FfmpegError::MissingInput(fonts_dir.to_path_buf()));
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(FfmpegError::Io)?;
    }

    let mut video_filter = format_scale_filter(storyboard.width, storyboard.height);
    if let Some(ass) = ass {
        video_filter.push_str(&format!(
            ",ass='{}':fontsdir='{}'",
            escape_filter_path(ass),
            escape_filter_path(fonts_dir)
        ));
    }
    let mut args: Vec<OsString> = vec![
        OsString::from("-y"),
        OsString::from("-hide_banner"),
        OsString::from("-loglevel"),
        OsString::from("error"),
        OsString::from("-i"),
        merged.as_os_str().to_owned(),
    ];
    if let Some(audio) = tracks.audio {
        args.extend([OsString::from("-i"), audio.as_os_str().to_owned()]);
    }
    if let Some(bgm) = tracks.bgm {
        args.extend([
            OsString::from("-stream_loop"),
            OsString::from("-1"),
            OsString::from("-i"),
            bgm.as_os_str().to_owned(),
        ]);
    }

    let has_audio = tracks.audio.is_some();
    let has_bgm = tracks.bgm.is_some();
    if has_audio || has_bgm {
        let audio_index = if has_audio { 1 } else { 2 };
        let bgm_index = if has_audio { 2 } else { 1 };
        let target = format!("{duration:.3}");
        let audio_filter = match (has_audio, has_bgm) {
            (true, true) => format!(
                "[1:a]apad,atrim=duration={target}[voice];[2:a]volume=0.22,atrim=duration={target}[music];[voice][music]amix=inputs=2:duration=longest:dropout_transition=2[mix]"
            ),
            (true, false) => format!("[{audio_index}:a]apad,atrim=duration={target}[mix]"),
            (false, true) => format!("[{bgm_index}:a]volume=0.22,atrim=duration={target}[mix]"),
            (false, false) => unreachable!(),
        };
        args.extend([
            OsString::from("-filter_complex"),
            OsString::from(format!("[0:v]{video_filter}[video];{audio_filter}")),
            OsString::from("-map"),
            OsString::from("[video]"),
            OsString::from("-map"),
            OsString::from("[mix]"),
        ]);
    } else {
        args.extend([
            OsString::from("-vf"),
            OsString::from(video_filter),
            OsString::from("-map"),
            OsString::from("0:v:0"),
            OsString::from("-map"),
            OsString::from("0:a:0?"),
        ]);
    }
    args.extend([
        OsString::from("-c:v"),
        OsString::from("libx264"),
        OsString::from("-preset"),
        OsString::from("medium"),
        OsString::from("-crf"),
        OsString::from("18"),
        OsString::from("-pix_fmt"),
        OsString::from("yuv420p"),
        OsString::from("-c:a"),
        OsString::from("aac"),
        OsString::from("-b:a"),
        OsString::from("192k"),
        OsString::from("-t"),
        OsString::from(format!("{duration:.3}")),
        OsString::from("-movflags"),
        OsString::from("+faststart"),
        output.as_os_str().to_owned(),
    ]);
    run_ffmpeg("叠加独立音频和字幕", args.iter().map(|arg| arg.as_os_str()))?;

    let media = ffprobe::probe(output)?;
    if media.width != Some(storyboard.width) || media.height != Some(storyboard.height) {
        return Err(FfmpegError::InvalidStoryboard(format!(
            "最终画幅为 {:?}x{:?}，预期 {}x{}",
            media.width, media.height, storyboard.width, storyboard.height
        )));
    }
    Ok(AssembleResult {
        output: output.to_path_buf(),
        duration_sec: media.duration_sec,
        media,
    })
}

pub fn concat_clips(
    clips: &[PathBuf],
    concat_file: &Path,
    output: &Path,
) -> Result<(), FfmpegError> {
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
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(FfmpegError::Io)?;
    }
    run_ffmpeg(
        "拼接视觉片段",
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
            output.as_os_str(),
        ],
    )
}

fn write_external_ass(
    subtitles: &Path,
    storyboard: &Storyboard,
    duration: f64,
    output: &Path,
) -> Result<(), FfmpegError> {
    let cues = parse_subtitles(subtitles, duration)?;
    let style = subtitle_style(storyboard);
    let mut ass = ass_header(storyboard.width, storyboard.height, &style);
    let mut written = 0;
    for cue in cues {
        if cue.end <= cue.start || cue.start >= duration {
            continue;
        }
        let end = cue.end.min(duration);
        let text = cue
            .text
            .lines()
            .map(|line| wrap_caption(line.trim(), &storyboard.lang))
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\\N");
        if !text.is_empty() {
            ass.push_str(&format!(
                "Dialogue: 0,{}, {},Default,,0,0,0,,{}\n",
                ass_time(cue.start.max(0.0)),
                ass_time(end),
                text
            ));
            written += 1;
        }
    }
    if written == 0 {
        return Err(FfmpegError::InvalidStoryboard(format!(
            "字幕文件没有有效条目: {}",
            subtitles.display()
        )));
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(FfmpegError::Io)?;
    }
    std::fs::write(output, ass).map_err(FfmpegError::Io)
}

struct SubtitleCue {
    start: f64,
    end: f64,
    text: String,
}

fn parse_subtitles(path: &Path, duration: f64) -> Result<Vec<SubtitleCue>, FfmpegError> {
    let raw = std::fs::read_to_string(path).map_err(FfmpegError::Io)?;
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "srt" => parse_srt(&raw),
        "lrc" => parse_lrc(&raw, duration),
        _ => Err(FfmpegError::InvalidStoryboard(format!(
            "不支持的字幕格式: {}（仅支持 .srt / .lrc）",
            path.display()
        ))),
    }
}

fn parse_srt(raw: &str) -> Result<Vec<SubtitleCue>, FfmpegError> {
    let mut cues = Vec::new();
    let normalized = raw.replace("\r\n", "\n").replace("\r", "\n");
    for block in normalized.split("\n\n") {
        let lines: Vec<&str> = block.lines().map(str::trim_end).collect();
        let Some(time_index) = lines.iter().position(|line| line.contains("-->")) else {
            continue;
        };
        let times: Vec<&str> = lines[time_index].split("-->").collect();
        if times.len() != 2 {
            continue;
        }
        let start = parse_subtitle_time(times[0]).ok_or_else(|| {
            FfmpegError::InvalidStoryboard(format!("SRT 时间无效: {}", lines[time_index]))
        })?;
        let end = parse_subtitle_time(times[1]).ok_or_else(|| {
            FfmpegError::InvalidStoryboard(format!("SRT 时间无效: {}", lines[time_index]))
        })?;
        let text = lines[time_index + 1..].join("\n");
        cues.push(SubtitleCue { start, end, text });
    }
    Ok(cues)
}

fn parse_lrc(raw: &str, duration: f64) -> Result<Vec<SubtitleCue>, FfmpegError> {
    let mut timed = Vec::new();
    let normalized = raw.replace("\r\n", "\n").replace("\r", "\n");
    for line in normalized.lines() {
        let Some(close) = line.find(']') else {
            continue;
        };
        let Some(start) = parse_lrc_time(&line[1..close]) else {
            continue;
        };
        let text = line[close + 1..].trim().to_string();
        if !text.is_empty() {
            timed.push((start, text));
        }
    }
    timed.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok(timed
        .iter()
        .enumerate()
        .map(|(index, (start, text))| SubtitleCue {
            start: *start,
            end: timed.get(index + 1).map(|next| next.0).unwrap_or(duration),
            text: text.clone(),
        })
        .collect())
}

fn parse_subtitle_time(value: &str) -> Option<f64> {
    let value = value.trim().replace(',', ".");
    let mut parts = value.split(':');
    let hours = parts.next()?.parse::<f64>().ok()?;
    let minutes = parts.next()?.parse::<f64>().ok()?;
    let seconds = parts.next()?.parse::<f64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(hours * 3600.0 + minutes * 60.0 + seconds)
}

fn parse_lrc_time(value: &str) -> Option<f64> {
    let mut parts = value.trim().split(':');
    let minutes = parts.next()?.parse::<f64>().ok()?;
    let seconds = parts.next()?.parse::<f64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(minutes * 60.0 + seconds)
}

fn scene_path(stored: Option<&str>, fallback_dir: &Path, id: &str) -> PathBuf {
    stored
        .map(PathBuf::from)
        .unwrap_or_else(|| fallback_dir.join(format!("{id}.mp4")))
}

fn require_file(path: &Path) -> Result<(), FfmpegError> {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() && metadata.len() > 0 => Ok(()),
        _ => Err(FfmpegError::MissingInput(path.to_path_buf())),
    }
}

fn run_ffmpeg<'a, I>(operation: &str, args: I) -> Result<(), FfmpegError>
where
    I: IntoIterator<Item = &'a OsStr>,
{
    let result = Command::new("ffmpeg")
        .args(args)
        .output()
        .map_err(FfmpegError::Io)?;
    if result.status.success() {
        Ok(())
    } else {
        Err(FfmpegError::Failed {
            operation: operation.to_string(),
            message: String::from_utf8_lossy(&result.stderr).trim().to_string(),
        })
    }
}

fn subtitle_style(storyboard: &Storyboard) -> SubtitleStyle {
    let size = if storyboard.width >= 1000 { 48 } else { 40 };
    SubtitleStyle {
        font: "Source Han Sans SC",
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
    let centiseconds = (seconds.max(0.0) * 100.0).round() as u64;
    let hours = centiseconds / 3_600_000;
    let minutes = (centiseconds / 6_000) % 60;
    let seconds = (centiseconds / 100) % 60;
    format!(
        "{hours}:{minutes:02}:{seconds:02}.{:02}",
        centiseconds % 100
    )
}

fn is_forbidden_line_start(ch: char) -> bool {
    matches!(
        ch,
        '，' | '。'
            | '！'
            | '？'
            | '；'
            | '：'
            | '、'
            | '）'
            | '》'
            | '」'
            | '』'
            | '】'
            | '”'
            | '’'
            | '…'
            | '—'
            | '％'
    )
}

fn is_forbidden_line_end(ch: char) -> bool {
    matches!(ch, '（' | '《' | '「' | '『' | '【' | '“' | '‘')
}

fn zh_units(text: &str) -> Vec<String> {
    static JIEBA: std::sync::OnceLock<jieba_rs::Jieba> = std::sync::OnceLock::new();
    let jieba = JIEBA.get_or_init(|| {
        let mut jieba = jieba_rs::Jieba::new();
        for word in ["冒着", "雨滴", "灯光", "羊角包"] {
            jieba.add_word(word, None, None);
        }
        jieba
    });
    jieba
        .cut(text, true)
        .into_iter()
        .map(|token| token.word.to_string())
        .collect()
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
                lines.push(std::mem::take(&mut current));
            }
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
    } else {
        for unit in zh_units(caption) {
            let unit_len = unit.chars().count();
            let fits = current.is_empty() || current.chars().count() + unit_len <= max_chars;
            let hangs = unit_len == 1
                && unit.chars().next().is_some_and(is_forbidden_line_start)
                && !current.is_empty();
            if fits || hangs {
                // 闭合标点挂到上一行，避免新行以标点开头。
                current.push_str(&unit);
                continue;
            }
            let mut carried = String::new();
            while current
                .chars()
                .next_back()
                .is_some_and(is_forbidden_line_end)
            {
                carried.insert(0, current.pop().unwrap());
            }
            lines.push(std::mem::take(&mut current));
            current = carried;
            current.push_str(&unit);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
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

fn format_scale_filter(width: u32, height: u32) -> String {
    format!(
        "scale={width}:{height}:force_original_aspect_ratio=decrease,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2"
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

    fn storyboard(lang: &str) -> Storyboard {
        Storyboard {
            title: "test".to_string(),
            lang: lang.to_string(),
            style: "realistic-cinematic".to_string(),
            width: 720,
            height: 1280,
            fps: 30,
            frame_rate_video: 24,
            scenes: vec![],
        }
    }

    #[test]
    fn ass_time_uses_centiseconds() {
        assert_eq!(ass_time(0.0), "0:00:00.00");
        assert_eq!(ass_time(3.744), "0:00:03.74");
        assert_eq!(ass_time(61.25), "0:01:01.25");
    }

    #[test]
    fn external_subtitle_parsers_keep_independent_timing() {
        let srt = parse_srt("1\n00:00:01,000 --> 00:00:03,500\n第一条字幕\n\n2\n00:00:04,000 --> 00:00:06,000\n第二条字幕\n").unwrap();
        assert_eq!(srt.len(), 2);
        assert_eq!((srt[0].start, srt[0].end), (1.0, 3.5));
        assert_eq!(srt[0].text, "第一条字幕");

        let lrc = parse_lrc("[00:01.00]第一条字幕\n[00:04.50]第二条字幕\n", 8.0).unwrap();
        assert_eq!(lrc.len(), 2);
        assert_eq!((lrc[0].start, lrc[0].end), (1.0, 4.5));
        assert_eq!(lrc[1].end, 8.0);
    }

    #[test]
    fn chinese_caption_obeys_kinsoku_and_word_boundaries() {
        let wrapped = wrap_caption(
            "黄昏时分，城市的天际线被染成金色，远处的摩天大楼亮起星星点点的灯光，",
            "zh",
        );
        for line in wrapped.split("\\N") {
            assert!(!line.chars().next().is_some_and(is_forbidden_line_start));
            assert!(!line.chars().next_back().is_some_and(is_forbidden_line_end));
        }
        assert!(!wrapped.contains("灯\\N光"));
    }

    #[test]
    fn english_caption_wraps_at_words() {
        let text = wrap_caption(
            "One two three four five six seven eight nine ten eleven twelve thirteen",
            "en",
        );
        assert!(text.contains("\\N"));
        assert!(!text.contains("thirte\\N"));
    }

    #[test]
    fn external_ass_has_style_header() {
        let sb = storyboard("zh");
        let header = ass_header(sb.width, sb.height, &subtitle_style(&sb));
        assert!(header.contains("PlayResX: 720"));
        assert!(header.contains("Source Han Sans SC"));
    }

    #[test]
    fn subtitle_times_parse_srt_and_lrc_formats() {
        assert_eq!(parse_subtitle_time("00:01:02,500"), Some(62.5));
        assert_eq!(parse_subtitle_time("00:01:02.500"), Some(62.5));
        assert_eq!(parse_lrc_time("01:02.50"), Some(62.5));
        assert_eq!(parse_lrc_time("bad"), None);
    }
}
