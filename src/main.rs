mod agnes;
mod media;
mod models;
mod pipeline;
mod split;
mod styles;
mod tts;

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

use agnes::{AgnesClient, AgnesOptions, CreateVideoRequest, MIN_VIDEO_BYTES};
use media::{ffmpeg, ffprobe};
use models::{Lang, Storyboard, num_frames_for_duration};
use styles::realistic;
use tts::{
    EdgeTtsProvider, TtsParams, default_voice_with_gender, lang_tag, rate_from_speed,
    synthesize_with_retry,
};

#[derive(Parser)]
#[command(
    name = "agnes-video-free",
    version,
    about = "把一段故事文本变成带旁白/字幕的短视频（Agnes Video V2.0 + edge-tts + ffmpeg）"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// 启动交互式向导（无子命令时也会进入）
    Interactive,
    /// 输出可用风格列表
    Styles,
    /// 分句：把故事切成一句一拍，写入 storyboard.json
    Split(SplitArgs),
    /// 生成旁白 mp3（Rust 原生 edge-tts，免费无需 key）
    Tts(TtsArgs),
    /// 生成 Agnes 视频片段（异步任务+轮询；M1 实现）
    Video(VideoArgs),
    /// 组装成片（ffmpeg + libass）
    Assemble(AssembleArgs),
    /// 全流程；--dry-run 只预览分句与 prompt，不写文件
    All(AllArgs),
    /// 查看每个场景的 TTS、视频、clip 和成片状态
    Status(StatusArgs),
    /// 从缺失阶段继续执行，已完成阶段自动跳过
    Resume(ResumeArgs),
    /// 安全清理单个场景的生成产物
    Clean(CleanArgs),
}

#[derive(Args)]
struct SplitArgs {
    /// 故事文本路径（UTF-8）
    story: PathBuf,
    /// 标题（默认使用故事文件名）
    #[arg(long)]
    title: Option<String>,
    /// 语言：zh | en
    #[arg(long, default_value = "zh")]
    lang: String,
    /// 风格 id（realistic-cinematic / realistic-vlog / realistic-documentary）
    #[arg(long, default_value = "realistic-cinematic")]
    style: String,
    /// 可选 visual_plan.json（key 为 s01/01，value 为英文画面描述）
    #[arg(long)]
    visual_plan: Option<PathBuf>,
    /// 输出 storyboard.json 路径
    #[arg(long, default_value = "storyboard.json")]
    out: PathBuf,
}

#[derive(Args)]
struct TtsArgs {
    /// storyboard.json 路径（由 split / all 生成）
    #[arg(long, default_value = "storyboard.json")]
    storyboard: PathBuf,
    /// 音色（指定后覆盖 --gender 的默认值）
    #[arg(long)]
    voice: Option<String>,
    /// 性别（决定默认音色）
    #[arg(long, value_enum, default_value_t = Gender::Female)]
    gender: Gender,
    /// 语速（1.0 = 正常）
    #[arg(long, default_value_t = 1.0)]
    speed: f64,
    /// 输出目录（每场生成 <id>.mp3）
    #[arg(long, default_value = "audio/narration")]
    out_dir: PathBuf,
}

#[derive(clap::ValueEnum, Clone, Copy)]
enum Gender {
    Female,
    Male,
}

#[derive(clap::ValueEnum, Clone, Copy)]
enum CleanStage {
    Audio,
    Video,
    Clip,
    All,
}

#[derive(Args)]
struct VideoArgs {
    /// storyboard.json 路径
    #[arg(long, default_value = "storyboard.json")]
    storyboard: PathBuf,
    /// 旁白目录（每场读取 <id>.mp3）
    #[arg(long, default_value = "audio/narration")]
    audio_dir: PathBuf,
    /// 视频片段输出目录（每场写入 <id>.mp4）
    #[arg(long, default_value = "assets/videos")]
    out_dir: PathBuf,
    /// Agnes API 根地址（默认国际站 apihub.agnes-ai.com）
    #[arg(long, default_value = agnes::DEFAULT_BASE_URL)]
    api_base_url: String,
    /// 轮询间隔（秒）
    #[arg(long, default_value_t = 8)]
    poll_interval: u64,
    /// 单段最大轮询时间（秒）
    #[arg(long, default_value_t = 900)]
    poll_timeout: u64,
    /// 并发数（免费 key 限流 1 req/min，当前实现串行）
    #[arg(long, default_value_t = 1)]
    concurrency: u32,
}

#[derive(Args)]
struct AssembleArgs {
    /// storyboard.json 路径
    #[arg(long, default_value = "storyboard.json")]
    storyboard: PathBuf,
    /// 旁白目录（场景未记录路径时使用）
    #[arg(long, default_value = "audio/narration")]
    audio_dir: PathBuf,
    /// 视频目录（场景未记录路径时使用）
    #[arg(long, default_value = "assets/videos")]
    video_dir: PathBuf,
    /// 字体目录（Nix 包运行时会优先回退到 AGNES_VIDEO_FREE_FONTS）
    #[arg(long, default_value = "assets/fonts")]
    fonts_dir: PathBuf,
    /// 最终输出 MP4
    #[arg(long, default_value = "out/story.mp4")]
    output: PathBuf,
}

#[derive(Args)]
struct AllArgs {
    /// 故事文本路径（UTF-8）
    story: PathBuf,
    /// 标题（写入 storyboard）
    #[arg(long, default_value = "未命名")]
    title: String,
    /// 语言：zh | en
    #[arg(long, default_value = "zh")]
    lang: String,
    /// 风格 id
    #[arg(long, default_value = "realistic-cinematic")]
    style: String,
    /// 可选 visual_plan.json（key 为 s01/01，value 为英文画面描述）
    #[arg(long)]
    visual_plan: Option<PathBuf>,
    /// dry-run：只预览分句与 prompt，不写文件
    #[arg(long)]
    dry_run: bool,
    /// 输出 storyboard.json 路径
    #[arg(long, default_value = "storyboard.json")]
    out: PathBuf,
}

#[derive(Args)]
struct StatusArgs {
    /// storyboard.json 路径
    #[arg(long, default_value = "storyboard.json")]
    storyboard: PathBuf,
    /// 旁白目录
    #[arg(long, default_value = "audio/narration")]
    audio_dir: PathBuf,
    /// 视频片段目录
    #[arg(long, default_value = "assets/videos")]
    video_dir: PathBuf,
    /// 最终成片路径
    #[arg(long, default_value = "out/story.mp4")]
    output: PathBuf,
}

#[derive(Args)]
struct ResumeArgs {
    /// storyboard.json 路径
    #[arg(long, default_value = "storyboard.json")]
    storyboard: PathBuf,
    /// 旁白目录
    #[arg(long, default_value = "audio/narration")]
    audio_dir: PathBuf,
    /// 视频片段目录
    #[arg(long, default_value = "assets/videos")]
    video_dir: PathBuf,
    /// 字体目录（不存在时回退到 AGNES_VIDEO_FREE_FONTS）
    #[arg(long, default_value = "assets/fonts")]
    fonts_dir: PathBuf,
    /// 最终成片路径
    #[arg(long, default_value = "out/story.mp4")]
    output: PathBuf,
    /// 音色（指定后覆盖 --gender 的默认值）
    #[arg(long)]
    voice: Option<String>,
    /// 性别（决定默认音色）
    #[arg(long, value_enum, default_value_t = Gender::Female)]
    gender: Gender,
    /// 语速（1.0 = 正常）
    #[arg(long, default_value_t = 1.0)]
    speed: f64,
    /// Agnes API 根地址
    #[arg(long, default_value = agnes::DEFAULT_BASE_URL)]
    api_base_url: String,
    /// 轮询间隔（秒）
    #[arg(long, default_value_t = 8)]
    poll_interval: u64,
    /// 单段最大轮询时间（秒）
    #[arg(long, default_value_t = 900)]
    poll_timeout: u64,
}

#[derive(Args)]
struct CleanArgs {
    /// storyboard.json 路径，用于校验场景 ID
    #[arg(long, default_value = "storyboard.json")]
    storyboard: PathBuf,
    /// 要清理的单个场景 ID，例如 s07
    #[arg(long)]
    scene: String,
    /// 清理阶段：audio / video / clip / all
    #[arg(long, value_enum, default_value_t = CleanStage::All)]
    stage: CleanStage,
    /// 旁白目录
    #[arg(long, default_value = "audio/narration")]
    audio_dir: PathBuf,
    /// 视频片段目录
    #[arg(long, default_value = "assets/videos")]
    video_dir: PathBuf,
    /// 最终输出路径（用于定位临时 clip；最终成片不会被删除）
    #[arg(long, default_value = "out/story.mp4")]
    output: PathBuf,
    /// 只显示待删除文件，不执行删除
    #[arg(long)]
    dry_run: bool,
    /// 确认执行删除；不提供时命令只做预览并退出
    #[arg(long)]
    yes: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        None | Some(Command::Interactive) => cmd_interactive().await,
        Some(Command::Styles) => list_styles(),
        Some(Command::Split(a)) => cmd_split(&a),
        Some(Command::Tts(a)) => cmd_tts(&a).await,
        Some(Command::Video(a)) => cmd_video(&a).await,
        Some(Command::Assemble(a)) => cmd_assemble(&a),
        Some(Command::All(a)) => cmd_all(&a),
        Some(Command::Status(a)) => cmd_status(&a),
        Some(Command::Resume(a)) => cmd_resume(&a).await,
        Some(Command::Clean(a)) => cmd_clean(&a),
    }
}

fn cmd_status(args: &StatusArgs) -> ExitCode {
    let storyboard = match read_storyboard(&args.storyboard) {
        Ok(sb) => sb,
        Err(e) => return err(&e),
    };
    if storyboard.scenes.is_empty() {
        return err("storyboard 没有任何场景");
    }

    let clip_dir = args
        .output
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(".agnes-video-free");
    println!("项目: {}", storyboard.title);
    println!(
        "风格: {} | 场景: {}",
        storyboard.style,
        storyboard.scenes.len()
    );
    println!();
    println!("场景     TTS       视频      任务                  clip      状态");
    println!("────────────────────────────────────────────────────────────");

    let mut tts_done = 0;
    let mut video_done = 0;
    let mut clip_done = 0;
    for scene in &storyboard.scenes {
        let audio = args.audio_dir.join(format!("{}.mp3", scene.id));
        let video = args.video_dir.join(format!("{}.mp4", scene.id));
        let clip = clip_dir.join(format!("{}.mp4", scene.id));
        let audio_ok = has_file(&audio, 1);
        let video_ok = has_file(&video, MIN_VIDEO_BYTES);
        let clip_ok = has_file(&clip, 1);
        tts_done += usize::from(audio_ok);
        video_done += usize::from(video_ok);
        clip_done += usize::from(clip_ok);
        let task = scene.agnes_task_id.as_deref().unwrap_or("—");
        let state = if clip_ok {
            "ready"
        } else if video_ok {
            "待组装"
        } else if scene.agnes_task_id.is_some() {
            "任务待轮询"
        } else if audio_ok {
            "待视频"
        } else {
            "待旁白"
        };
        println!(
            "{:<8} {:<9} {:<9} {:<9} {:<20} {}",
            scene.id,
            marker(audio_ok),
            marker(video_ok),
            task,
            marker(clip_ok),
            state
        );
    }
    println!();
    println!(
        "进度: TTS {}/{}，视频 {}/{}，clip {}/{}",
        tts_done,
        storyboard.scenes.len(),
        video_done,
        storyboard.scenes.len(),
        clip_done,
        storyboard.scenes.len()
    );
    println!(
        "最终成片: {} {}",
        marker(has_file(&args.output, MIN_VIDEO_BYTES)),
        args.output.display()
    );
    ExitCode::SUCCESS
}

async fn cmd_resume(args: &ResumeArgs) -> ExitCode {
    dotenvy::dotenv().ok();
    let storyboard = match read_storyboard(&args.storyboard) {
        Ok(sb) => sb,
        Err(e) => return err(&e),
    };
    if storyboard.scenes.is_empty() {
        return err("storyboard 没有任何场景");
    }

    let needs_tts = storyboard
        .scenes
        .iter()
        .any(|scene| !has_file(&args.audio_dir.join(format!("{}.mp3", scene.id)), 1));
    if needs_tts {
        println!("恢复阶段 1/3：补齐缺失旁白（已有有效文件自动跳过）");
        let tts_code = cmd_tts(&TtsArgs {
            storyboard: args.storyboard.clone(),
            voice: args.voice.clone(),
            gender: args.gender,
            speed: args.speed,
            out_dir: args.audio_dir.clone(),
        })
        .await;
        if tts_code != ExitCode::SUCCESS {
            return tts_code;
        }
    } else {
        println!("恢复阶段 1/3：旁白已齐全，跳过");
    }

    let needs_video = storyboard.scenes.iter().any(|scene| {
        !has_file(
            &args.video_dir.join(format!("{}.mp4", scene.id)),
            MIN_VIDEO_BYTES,
        )
    });
    let had_missing_media = needs_tts || needs_video;
    if needs_video {
        println!("恢复阶段 2/3：补齐缺失视频（已有有效文件自动跳过）");
        let video_code = cmd_video(&VideoArgs {
            storyboard: args.storyboard.clone(),
            audio_dir: args.audio_dir.clone(),
            out_dir: args.video_dir.clone(),
            api_base_url: args.api_base_url.clone(),
            poll_interval: args.poll_interval,
            poll_timeout: args.poll_timeout,
            concurrency: 1,
        })
        .await;
        if video_code != ExitCode::SUCCESS {
            return video_code;
        }
    } else {
        println!("恢复阶段 2/3：视频已齐全，跳过（不会请求 Agnes API）");
    }

    if !had_missing_media && has_file(&args.output, MIN_VIDEO_BYTES) {
        println!(
            "恢复阶段 3/3：最终成片已存在，跳过组装 → {}",
            args.output.display()
        );
        return ExitCode::SUCCESS;
    }
    if had_missing_media && has_file(&args.output, MIN_VIDEO_BYTES) {
        println!("恢复阶段 3/3：检测到素材曾缺失，将重新组装并覆盖现有成片");
    }

    println!("恢复阶段 3/3：组装最终成片");
    cmd_assemble(&AssembleArgs {
        storyboard: args.storyboard.clone(),
        audio_dir: args.audio_dir.clone(),
        video_dir: args.video_dir.clone(),
        fonts_dir: args.fonts_dir.clone(),
        output: args.output.clone(),
    })
}

fn cmd_clean(args: &CleanArgs) -> ExitCode {
    let mut storyboard = match read_storyboard(&args.storyboard) {
        Ok(sb) => sb,
        Err(e) => return err(&e),
    };
    if !safe_scene_id(&args.scene) {
        return err("场景 ID 不安全，只允许字母、数字、短横线和下划线");
    }
    let Some(scene_index) = storyboard
        .scenes
        .iter()
        .position(|scene| scene.id == args.scene)
    else {
        return err(&format!("storyboard 中不存在场景 {}", args.scene));
    };

    let clip_dir = args
        .output
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(".agnes-video-free");
    let audio = args.audio_dir.join(format!("{}.mp3", args.scene));
    let video = args.video_dir.join(format!("{}.mp4", args.scene));
    let clip = clip_dir.join(format!("{}.mp4", args.scene));
    let targets: Vec<(&str, PathBuf)> = match args.stage {
        // 音频或视频变化都会使已经封装的 clip 失效，因此一并清理 clip。
        CleanStage::Audio => vec![("audio", audio), ("clip", clip)],
        CleanStage::Video => vec![("video", video), ("clip", clip)],
        CleanStage::Clip => vec![("clip", clip)],
        CleanStage::All => vec![("audio", audio), ("video", video), ("clip", clip)],
    };

    println!(
        "清理场景 {}（阶段: {}）",
        args.scene,
        clean_stage_label(args.stage)
    );
    for (kind, path) in &targets {
        let present = std::fs::symlink_metadata(path).is_ok();
        println!(
            "  {:<5} {} {}",
            kind,
            if present { "删除" } else { "不存在" },
            path.display()
        );
    }
    println!("  最终成片保留: {}", args.output.display());

    if args.dry_run {
        println!("[dry-run] 未删除任何文件，也未修改 storyboard。");
        return ExitCode::SUCCESS;
    }
    if !args.yes {
        return err("为安全起见，实际删除必须显式添加 --yes；仅预览请使用 --dry-run");
    }

    let mut cleared_audio = false;
    let mut cleared_video = false;
    let mut failures = Vec::new();
    for (kind, path) in &targets {
        match remove_file_if_present(path) {
            Ok(true) => {
                println!("  ✓ 已清理 {}: {}", kind, path.display());
                match *kind {
                    "audio" => cleared_audio = true,
                    "video" => cleared_video = true,
                    _ => {}
                }
            }
            Ok(false) => {
                println!("  - {} 不存在，跳过", path.display());
                match *kind {
                    "audio" => cleared_audio = true,
                    "video" => cleared_video = true,
                    _ => {}
                }
            }
            Err(e) => failures.push(e),
        }
    }

    let scene = &mut storyboard.scenes[scene_index];
    if cleared_audio {
        scene.narration_audio = None;
        scene.agnes_task_id = None;
        scene.duration_sec = None;
        scene.num_frames = None;
    }
    if cleared_video {
        scene.motion_video = None;
        scene.agnes_task_id = None;
        scene.num_frames = None;
    }
    if let Err(e) = write_storyboard(&storyboard, &args.storyboard) {
        return err(&e);
    }
    println!("已同步 storyboard 场景状态；最终成片未删除，请在素材恢复后重新 assemble。");

    if failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        err(&failures.join("；"))
    }
}

fn remove_file_if_present(path: &Path) -> Result<bool, String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() || metadata.file_type().is_symlink() => {
            std::fs::remove_file(path)
                .map(|()| true)
                .map_err(|e| format!("删除 {} 失败: {e}", path.display()))
        }
        Ok(_) => Err(format!("{} 不是普通文件，拒绝删除", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("检查 {} 失败: {error}", path.display())),
    }
}

fn safe_scene_id(scene_id: &str) -> bool {
    !scene_id.is_empty()
        && scene_id.len() <= 64
        && scene_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

fn clean_stage_label(stage: CleanStage) -> &'static str {
    match stage {
        CleanStage::Audio => "audio",
        CleanStage::Video => "video",
        CleanStage::Clip => "clip",
        CleanStage::All => "all",
    }
}

fn has_file(path: &Path, min_bytes: usize) -> bool {
    std::fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.len() >= min_bytes as u64)
        .unwrap_or(false)
}

fn marker(done: bool) -> &'static str {
    if done { "✓" } else { "—" }
}

/// 交互式向导：收集配置后按 split → tts → video → assemble 执行。
async fn cmd_interactive() -> ExitCode {
    println!("╔══════════════════════════════════════════════════════╗");
    println!("║          agnes-video-free 交互式向导                 ║");
    println!("╚══════════════════════════════════════════════════════╝");
    println!("输入 q 可在任意配置步骤取消。每个阶段都会保留已生成的文件，便于稍后续跑。\n");

    let profiles = styles::all();
    let style_options: Vec<String> = profiles
        .iter()
        .map(|style| {
            format!(
                "{} — {}（{}，{}x{}）",
                style.name,
                style.id,
                style.default_platform.label(),
                style.canvas.0,
                style.canvas.1
            )
        })
        .collect();
    let style_index = match prompt_choice("选择风格", &style_options, 0) {
        Ok(index) => index,
        Err(e) => return wizard_error(&e),
    };
    let style = profiles[style_index].clone();

    let lang_options = vec!["中文（zh）".to_string(), "English（en）".to_string()];
    let lang_index = match prompt_choice("选择旁白语言", &lang_options, 0) {
        Ok(index) => index,
        Err(e) => return wizard_error(&e),
    };
    let lang = if lang_index == 0 { Lang::Zh } else { Lang::En };

    let source_options = vec!["读取故事文件".to_string(), "直接粘贴故事".to_string()];
    let source_index = match prompt_choice("选择故事来源", &source_options, 0) {
        Ok(index) => index,
        Err(e) => return wizard_error(&e),
    };
    let (story, story_input, title_default) = if source_index == 0 {
        let path = match prompt_line(
            "故事文件路径（UTF-8）",
            Some("examples/story_realistic.txt"),
        ) {
            Ok(value) => PathBuf::from(value),
            Err(e) => return wizard_error(&e),
        };
        let story = match read_story(&path) {
            Ok(story) => story,
            Err(e) => return wizard_error(&e),
        };
        let title = path
            .file_stem()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| "story".to_string());
        (story, Some(path), title)
    } else {
        let story = match read_pasted_story() {
            Ok(story) => story,
            Err(e) => return wizard_error(&e),
        };
        (story, None, "story".to_string())
    };
    let visual_plan_path = match prompt_line("visual_plan.json 路径（可留空）", None) {
        Ok(value) if value.is_empty() => None,
        Ok(value) => Some(PathBuf::from(value)),
        Err(e) => return wizard_error(&e),
    };
    let visual_plan = match read_visual_plan(visual_plan_path.as_deref()) {
        Ok(plan) => plan,
        Err(e) => return wizard_error(&e),
    };
    let scenes = match visual_plan.as_ref() {
        Some(plan) => pipeline::plan_scenes_with_visual_plan(&story, lang, &style, Some(plan)),
        None => pipeline::plan_scenes(&story, lang, &style),
    };
    if scenes.is_empty() {
        return wizard_error("故事没有可用内容，请检查输入内容");
    }

    let project_dir = match prompt_line("项目输出目录", Some(".")) {
        Ok(value) => PathBuf::from(value),
        Err(e) => return wizard_error(&e),
    };
    let pasted_story = story_input.is_none();
    let story_input = story_input.unwrap_or_else(|| project_dir.join("story.txt"));
    let title = match prompt_line("成片标题", Some(&title_default)) {
        Ok(value) if !value.trim().is_empty() => sanitize_title(&value),
        Ok(_) => return wizard_error("成片标题不能为空"),
        Err(e) => return wizard_error(&e),
    };
    if title.is_empty() {
        return wizard_error("成片标题不能只包含路径分隔符或控制字符");
    }

    let gender_options = vec!["女声".to_string(), "男声".to_string()];
    let gender_index = match prompt_choice("选择 TTS 音色性别", &gender_options, 0) {
        Ok(index) => index,
        Err(e) => return wizard_error(&e),
    };
    let speed = match prompt_line("TTS 语速（1.0 为正常）", Some("1.0")) {
        Ok(value) => match value.parse::<f64>() {
            Ok(speed) if speed.is_finite() && speed > 0.0 => speed,
            _ => return wizard_error("语速必须是大于 0 的数字"),
        },
        Err(e) => return wizard_error(&e),
    };

    dotenvy::dotenv().ok();
    if std::env::var("AGNES_API_KEY")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .is_none()
    {
        return wizard_error("未找到 AGNES_API_KEY，请设置环境变量或在当前目录 .env 中配置");
    }

    let storyboard = project_dir.join("storyboard.json");
    let audio_dir = project_dir.join("audio/narration");
    let video_dir = project_dir.join("assets/videos");
    let fonts_dir = project_dir.join("assets/fonts");
    let output = project_dir.join("out").join(format!("{title}.mp4"));

    println!("\n═══ 配置确认 ═══");
    println!("故事: {}", story_input.display());
    println!("场景: {} 场", scenes.len());
    println!(
        "风格: {}（{}，{}x{}）",
        style.id,
        style.default_platform.label(),
        style.canvas.0,
        style.canvas.1
    );
    println!(
        "语言: {} | 音色: {} | 语速: {speed:.2}",
        lang.label(),
        if gender_index == 0 {
            "女声"
        } else {
            "男声"
        }
    );
    println!("输出: {}", output.display());
    if let Some(path) = &visual_plan_path {
        println!("画面计划: {}", path.display());
    }
    println!("\n分句预览:");
    print_scenes(&scenes);
    println!("\n首场 prompt 预览:");
    println!("{}", scenes[0].prompt.as_deref().unwrap_or_default());

    let confirmed = match prompt_yes_no("确认开始执行完整流程？", true) {
        Ok(value) => value,
        Err(e) => return wizard_error(&e),
    };
    if !confirmed {
        println!("已取消，未执行任何生成阶段。");
        return ExitCode::SUCCESS;
    }

    if let Err(e) = fs::create_dir_all(&project_dir) {
        return err(&format!("创建项目目录 {} 失败: {e}", project_dir.display()));
    }
    if pasted_story && let Err(e) = fs::write(&story_input, &story) {
        return err(&format!(
            "写入粘贴的故事 {} 失败: {e}",
            story_input.display()
        ));
    }

    println!("\n═══ 1/4 分句并写入 storyboard ═══");
    let split_code = cmd_split(&SplitArgs {
        story: story_input,
        title: Some(title.clone()),
        lang: lang.label().to_string(),
        style: style.id.to_string(),
        visual_plan: visual_plan_path.clone(),
        out: storyboard.clone(),
    });
    if split_code != ExitCode::SUCCESS {
        return split_code;
    }

    println!("\n═══ 2/4 生成旁白 ═══");
    let tts_code = cmd_tts(&TtsArgs {
        storyboard: storyboard.clone(),
        voice: None,
        gender: if gender_index == 0 {
            Gender::Female
        } else {
            Gender::Male
        },
        speed,
        out_dir: audio_dir.clone(),
    })
    .await;
    if tts_code != ExitCode::SUCCESS {
        return tts_code;
    }

    println!("\n═══ 3/4 生成 Agnes 视频片段 ═══");
    let video_code = cmd_video(&VideoArgs {
        storyboard: storyboard.clone(),
        audio_dir,
        out_dir: video_dir,
        api_base_url: agnes::DEFAULT_BASE_URL.to_string(),
        poll_interval: 8,
        poll_timeout: 900,
        concurrency: 1,
    })
    .await;
    if video_code != ExitCode::SUCCESS {
        return video_code;
    }

    println!("\n═══ 4/4 组装最终成片 ═══");
    let assemble_code = cmd_assemble(&AssembleArgs {
        storyboard,
        audio_dir: project_dir.join("audio/narration"),
        video_dir: project_dir.join("assets/videos"),
        fonts_dir,
        output: output.clone(),
    });
    if assemble_code != ExitCode::SUCCESS {
        return assemble_code;
    }
    println!("\n向导完成，成片位于: {}", output.display());
    ExitCode::SUCCESS
}

fn read_pasted_story() -> Result<String, String> {
    println!("请粘贴故事内容；完成后输入单独一行 END：");
    let mut story = String::new();
    loop {
        let mut line = String::new();
        let read = io::stdin()
            .read_line(&mut line)
            .map_err(|e| format!("读取故事内容失败: {e}"))?;
        if read == 0 {
            return Err("输入已结束，故事粘贴未完成".to_string());
        }
        if line.trim() == "END" {
            break;
        }
        story.push_str(&line);
    }
    if story.trim().is_empty() {
        Err("粘贴的故事不能为空".to_string())
    } else {
        Ok(story)
    }
}

fn prompt_line(label: &str, default: Option<&str>) -> Result<String, String> {
    match default {
        Some(default) => print!("{label} [{default}]: "),
        None => print!("{label}: "),
    }
    io::stdout()
        .flush()
        .map_err(|e| format!("刷新终端输出失败: {e}"))?;
    let mut value = String::new();
    let read = io::stdin()
        .read_line(&mut value)
        .map_err(|e| format!("读取终端输入失败: {e}"))?;
    if read == 0 {
        return Err("输入已结束，向导取消".to_string());
    }
    let value = value.trim().to_string();
    if value.eq_ignore_ascii_case("q") {
        return Err("用户取消向导".to_string());
    }
    if value.is_empty() {
        Ok(default.unwrap_or_default().to_string())
    } else {
        Ok(value)
    }
}

fn prompt_choice(label: &str, options: &[String], default: usize) -> Result<usize, String> {
    if options.is_empty() || default >= options.len() {
        return Err(format!("{label} 没有可用选项"));
    }
    println!("{label}:");
    for (index, option) in options.iter().enumerate() {
        println!("  {}. {}", index + 1, option);
    }
    let default_value = (default + 1).to_string();
    loop {
        let value = prompt_line(
            &format!("请输入编号（默认 {}）", default + 1),
            Some(&default_value),
        )?;
        let value = match value.parse::<usize>() {
            Ok(value) => value,
            Err(_) => {
                println!("请输入有效编号。");
                continue;
            }
        };
        if (1..=options.len()).contains(&value) {
            return Ok(value - 1);
        }
        println!("请输入 1 到 {} 之间的编号。", options.len());
    }
}

fn prompt_yes_no(label: &str, default: bool) -> Result<bool, String> {
    let suffix = if default { "Y/n" } else { "y/N" };
    loop {
        let value = prompt_line(&format!("{label} [{suffix}]"), None)?;
        if value.is_empty() {
            return Ok(default);
        }
        match value.to_ascii_lowercase().as_str() {
            "y" | "yes" | "是" => return Ok(true),
            "n" | "no" | "否" => return Ok(false),
            _ => println!("请输入 y 或 n。"),
        }
    }
}

fn sanitize_title(title: &str) -> String {
    title
        .trim()
        .chars()
        .map(|ch| {
            if ch == '/' || ch == '\\' || ch.is_control() {
                '_'
            } else {
                ch
            }
        })
        .collect()
}

fn wizard_error(message: &str) -> ExitCode {
    if message == "用户取消向导" || message == "输入已结束，向导取消" {
        println!("\n{message}。");
        ExitCode::SUCCESS
    } else {
        err(message)
    }
}

/// 输出可用风格与完整三段式配置总览。
fn list_styles() -> ExitCode {
    let styles = styles::all();
    println!(
        "agnes-video-free v{} — 可用风格（{}）",
        env!("CARGO_PKG_VERSION"),
        styles.len()
    );
    for s in &styles {
        println!();
        println!("[{}] {} — {}", s.id, s.name, s.description);
        println!(
            "  平台 {} | 画幅 {}x{} | {}",
            s.default_platform.label(),
            s.canvas.0,
            s.canvas.1,
            s.aspect_line()
        );
        println!("  STYLE_HEADER:  {}", s.style_header());
        println!("  MOTION_FOOTER: {}", s.motion_footer);
        println!("  NEGATIVE:      {}", s.negative);
        println!(
            "  字幕: {}（{}）字号 {} 描边 {} 文字 {} 描边色 {}",
            s.subtitle.font,
            s.subtitle.font_file,
            s.subtitle.size,
            s.subtitle.outline,
            s.subtitle.color,
            s.subtitle.outline_color
        );
    }
    ExitCode::SUCCESS
}

fn cmd_split(args: &SplitArgs) -> ExitCode {
    let lang = match parse_lang(&args.lang) {
        Ok(l) => l,
        Err(e) => return err(&e),
    };
    let style = match styles::by_id(&args.style) {
        Some(s) => s,
        None => return err(&unknown_style(&args.style)),
    };
    let story = match read_story(&args.story) {
        Ok(s) => s,
        Err(e) => return err(&e),
    };

    let visual_plan = match read_visual_plan(args.visual_plan.as_deref()) {
        Ok(plan) => plan,
        Err(e) => return err(&e),
    };
    let scenes = pipeline::plan_scenes_with_visual_plan(&story, lang, &style, visual_plan.as_ref());
    print_visual_plan_warnings(visual_plan.as_ref(), &scenes);
    print_scenes(&scenes);

    let title = args.title.clone().unwrap_or_else(|| {
        args.story
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "未命名".to_string())
    });
    let sb = pipeline::build_storyboard(&title, lang, &style, scenes);
    match write_storyboard(&sb, &args.out) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => err(&e),
    }
}

async fn cmd_tts(args: &TtsArgs) -> ExitCode {
    // 一次性初始化 rustls crypto provider（kothok 要求，幂等）
    kothok_edge_tts::init_tls();

    let sb = match read_storyboard(&args.storyboard) {
        Ok(sb) => sb,
        Err(e) => return err(&e),
    };
    let lang = match sb.lang.parse::<Lang>() {
        Ok(l) => l,
        Err(e) => return err(&e),
    };
    let voice = args.voice.clone().unwrap_or_else(|| {
        default_voice_with_gender(lang, matches!(args.gender, Gender::Male)).to_string()
    });
    let rate = rate_from_speed(args.speed);
    let lang = lang_tag(lang);

    if let Err(e) = fs::create_dir_all(&args.out_dir) {
        return err(&format!(
            "创建输出目录 {} 失败: {e}",
            args.out_dir.display()
        ));
    }

    println!(
        "TTS: 音色 {voice}，语速 {rate}，共 {} 场\n",
        sb.scenes.len()
    );
    let provider = EdgeTtsProvider;
    let (mut done, mut skipped, mut failed) = (0, 0, 0);

    for scene in &sb.scenes {
        let out = args.out_dir.join(format!("{}.mp3", scene.id));
        if has_file(&out, 1) {
            println!("  {} 已存在，跳过", out.display());
            skipped += 1;
            continue;
        }
        if out.exists() {
            println!("  {} 文件为空或无效，将重新生成", out.display());
        }
        print!("  合成 {} … ", scene.id);
        match synthesize_with_retry(
            &provider,
            TtsParams {
                text: &scene.narration,
                voice: &voice,
                rate: &rate,
                lang,
            },
            &out,
            3,
            1,
        )
        .await
        {
            Ok(()) => {
                println!("✓ {}", out.display());
                done += 1;
            }
            Err(e) => {
                println!("✗ {e}");
                failed += 1;
            }
        }
    }

    println!();
    println!("TTS 完成: 新增 {done}，跳过 {skipped}，失败 {failed}");
    if failed > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn cmd_assemble(args: &AssembleArgs) -> ExitCode {
    let mut storyboard = match read_storyboard(&args.storyboard) {
        Ok(sb) => sb,
        Err(e) => return err(&e),
    };
    let fonts_dir = if args.fonts_dir.is_dir() {
        args.fonts_dir.clone()
    } else if let Ok(value) = std::env::var("AGNES_VIDEO_FREE_FONTS") {
        let fallback = PathBuf::from(value);
        if fallback.is_dir() {
            fallback
        } else {
            return err(&format!("字体目录不存在: {}", args.fonts_dir.display()));
        }
    } else {
        return err(&format!("字体目录不存在: {}", args.fonts_dir.display()));
    };

    println!(
        "开始组装: {} 场 → {}（字体: {}）",
        storyboard.scenes.len(),
        args.output.display(),
        fonts_dir.display()
    );
    let result = match ffmpeg::assemble_storyboard(
        &mut storyboard,
        &args.audio_dir,
        &args.video_dir,
        &fonts_dir,
        &args.output,
    ) {
        Ok(result) => result,
        Err(e) => return err(&e.to_string()),
    };
    if let Err(e) = write_storyboard(&storyboard, &args.storyboard) {
        return err(&e);
    }
    println!(
        "✓ 成片完成: {}（{:.2}s，{}x{}）",
        result.output.display(),
        result.duration_sec,
        result.media.width.unwrap_or_default(),
        result.media.height.unwrap_or_default()
    );
    ExitCode::SUCCESS
}

async fn resolve_video_task_id(
    client: &AgnesClient,
    request: &CreateVideoRequest,
    existing_task_id: Option<&str>,
) -> Result<String, agnes::AgnesError> {
    if let Some(task_id) = existing_task_id.filter(|task_id| !task_id.trim().is_empty()) {
        return Ok(task_id.to_string());
    }
    Ok(client.create_video(request).await?.video_id)
}

async fn cmd_video(args: &VideoArgs) -> ExitCode {
    dotenvy::dotenv().ok();

    if args.concurrency != 1 {
        eprintln!(
            "提示：当前 video 实现按免费额度串行处理，忽略 --concurrency {}，使用 1。",
            args.concurrency
        );
    }
    let api_key = match std::env::var("AGNES_API_KEY") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => return err("未找到 AGNES_API_KEY，请设置环境变量或在当前目录 .env 中配置"),
    };
    let mut storyboard = match read_storyboard(&args.storyboard) {
        Ok(sb) => sb,
        Err(e) => return err(&e),
    };
    if storyboard.scenes.is_empty() {
        return err("storyboard 没有任何场景");
    }

    let options = AgnesOptions {
        poll_interval: std::time::Duration::from_secs(args.poll_interval),
        poll_timeout: std::time::Duration::from_secs(args.poll_timeout),
        ..AgnesOptions::default()
    };
    let client = match AgnesClient::with_options(&api_key, &args.api_base_url, options) {
        Ok(client) => client,
        Err(e) => return err(&e.to_string()),
    };
    if let Err(e) = fs::create_dir_all(&args.out_dir) {
        return err(&format!("创建视频输出目录失败: {e}"));
    }

    println!(
        "Agnes Video: {}，模型 {}，轮询 {}s/段，超时 {}s/段",
        args.api_base_url,
        agnes::MODEL,
        args.poll_interval,
        args.poll_timeout
    );
    let (mut generated, mut skipped, mut failed) = (0, 0, 0);

    for index in 0..storyboard.scenes.len() {
        let scene = &storyboard.scenes[index];
        let scene_id = scene.id.clone();
        let audio_path = args.audio_dir.join(format!("{}.mp3", scene_id));
        let video_path = args.out_dir.join(format!("{}.mp4", scene_id));

        let audio_info = match ffprobe::probe(&audio_path) {
            Ok(info) => info,
            Err(e) => {
                eprintln!("  {} 音频检查失败: {e}", scene.id);
                failed += 1;
                continue;
            }
        };
        let duration = audio_info.duration_sec;
        let num_frames = num_frames_for_duration(duration);

        // 有效文件直接跳过，并把 ffprobe 结果补回 storyboard。
        if video_path.exists() {
            match fs::metadata(&video_path) {
                Ok(metadata) if metadata.len() as usize >= MIN_VIDEO_BYTES => {
                    println!(
                        "  {} 已存在，跳过（音频 {:.2}s，{} 帧）",
                        scene.id, duration, num_frames
                    );
                    let scene = &mut storyboard.scenes[index];
                    scene.narration_audio = Some(audio_path.display().to_string());
                    scene.motion_video = Some(video_path.display().to_string());
                    scene.duration_sec = Some(duration);
                    scene.num_frames = Some(num_frames);
                    skipped += 1;
                    if let Err(e) = write_storyboard(&storyboard, &args.storyboard) {
                        return err(&e);
                    }
                    continue;
                }
                Ok(metadata) => {
                    eprintln!(
                        "  {} 已存在但文件过小（{} bytes），将重新生成",
                        scene.id,
                        metadata.len()
                    );
                }
                Err(e) => {
                    eprintln!("  {} 检查既有视频失败，将重新生成: {e}", scene.id);
                }
            }
        }

        let prompt = scene
            .prompt
            .clone()
            .unwrap_or_else(|| scene.caption.clone());
        let negative = scene.negative_prompt.clone().unwrap_or_default();
        let request = CreateVideoRequest::new(
            prompt,
            negative,
            storyboard.width,
            storyboard.height,
            num_frames,
            storyboard.frame_rate_video,
        );

        let existing_task_id = scene.agnes_task_id.clone();
        let has_existing_task = existing_task_id
            .as_deref()
            .is_some_and(|task_id| !task_id.trim().is_empty());
        if has_existing_task {
            println!(
                "  {} 复用已记录任务 {}，继续轮询…",
                scene_id,
                existing_task_id.as_deref().unwrap_or_default()
            );
        } else {
            println!(
                "  {} 提交任务（旁白 {:.2}s → {} 帧）…",
                scene_id, duration, num_frames
            );
        }
        let task_id =
            match resolve_video_task_id(&client, &request, existing_task_id.as_deref()).await {
                Ok(task_id) => task_id,
                Err(e) => {
                    eprintln!("  {} 创建/复用任务失败: {e}", scene_id);
                    failed += 1;
                    continue;
                }
            };
        if !has_existing_task {
            storyboard.scenes[index].agnes_task_id = Some(task_id.clone());
            // 任务创建成功后立刻落盘，避免进程在轮询期间中断导致任务 ID 丢失。
            if let Err(e) = write_storyboard(&storyboard, &args.storyboard) {
                return err(&e);
            }
            println!("  {} 任务 {} 已保存，开始轮询…", scene_id, task_id);
        }
        let result = match client.wait_for_video(&task_id).await {
            Ok(result) => result,
            Err(e) => {
                if matches!(&e, agnes::AgnesError::FailedTask(_)) {
                    storyboard.scenes[index].agnes_task_id = None;
                    if let Err(write_error) = write_storyboard(&storyboard, &args.storyboard) {
                        return err(&write_error);
                    }
                }
                eprintln!("  {} 任务失败: {e}", scene_id);
                failed += 1;
                continue;
            }
        };
        if let Err(e) = client.download_video(&result.url, &video_path).await {
            eprintln!("  {} 下载失败: {e}", scene_id);
            failed += 1;
            continue;
        }

        println!("  {} 完成 → {}", scene_id, video_path.display());
        let scene = &mut storyboard.scenes[index];
        scene.narration_audio = Some(audio_path.display().to_string());
        scene.motion_video = Some(video_path.display().to_string());
        scene.duration_sec = Some(duration);
        scene.num_frames = Some(num_frames);
        generated += 1;
        if let Err(e) = write_storyboard(&storyboard, &args.storyboard) {
            return err(&e);
        }
    }

    println!();
    println!("视频生成完成: 新增 {generated}，跳过 {skipped}，失败 {failed}");
    if failed > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn cmd_all(args: &AllArgs) -> ExitCode {
    let lang = match parse_lang(&args.lang) {
        Ok(l) => l,
        Err(e) => return err(&e),
    };
    let style = match styles::by_id(&args.style) {
        Some(s) => s,
        None => return err(&unknown_style(&args.style)),
    };
    let story = match read_story(&args.story) {
        Ok(s) => s,
        Err(e) => return err(&e),
    };

    let visual_plan = match read_visual_plan(args.visual_plan.as_deref()) {
        Ok(plan) => plan,
        Err(e) => return err(&e),
    };
    let scenes = pipeline::plan_scenes_with_visual_plan(&story, lang, &style, visual_plan.as_ref());
    print_visual_plan_warnings(visual_plan.as_ref(), &scenes);

    println!("═══ 分句预览（{} 场）═══", scenes.len());
    print_scenes(&scenes);

    println!();
    println!(
        "═══ prompt 预览（style: {}，{}x{}，{}）═══",
        style.id,
        style.canvas.0,
        style.canvas.1,
        style.aspect_line()
    );
    for s in &scenes {
        println!();
        println!("{} — {}", s.id, s.caption);
        if let Some(visual) = &s.visual {
            println!("visual: {visual}");
        }
        println!("{}", s.prompt.as_deref().unwrap_or_default());
        if lang == Lang::En && s.visual.is_none() {
            for issue in realistic::validate_scene_body(&s.caption) {
                println!("  ⚠ {issue}");
            }
        }
    }
    if lang == Lang::Zh && visual_plan.is_none() {
        println!();
        println!(
            "注: SCENE_BODY 为中文原句直塞（Agnes 可理解中文）；写实风格建议提供英文 visual_plan 以获得更稳定的画面。"
        );
    }

    if args.dry_run {
        println!();
        println!("[dry-run] 未写入任何文件。");
        return ExitCode::SUCCESS;
    }

    let sb = pipeline::build_storyboard(&args.title, lang, &style, scenes);
    if let Err(e) = write_storyboard(&sb, &args.out) {
        return err(&e);
    }
    println!();
    println!("后续阶段将在 M1/M2 接入: tts → video → assemble（当前已完成分句与 storyboard）");
    ExitCode::SUCCESS
}

fn print_scenes(scenes: &[models::Scene]) {
    for s in scenes {
        println!("  {} [{}字] {}", s.id, s.caption.chars().count(), s.caption);
    }
}

fn write_storyboard(sb: &Storyboard, out: &PathBuf) -> Result<(), String> {
    let json = serde_json::to_string_pretty(sb).map_err(|e| e.to_string())?;
    fs::write(out, json + "\n").map_err(|e| format!("写入 {} 失败: {e}", out.display()))?;
    println!("✓ storyboard 已写入 {}", out.display());
    Ok(())
}

fn read_story(path: &PathBuf) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("读取 {} 失败: {e}", path.display()))
}

fn read_visual_plan(path: Option<&Path>) -> Result<Option<BTreeMap<String, String>>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    let raw = fs::read_to_string(path)
        .map_err(|e| format!("读取 visual_plan {} 失败: {e}", path.display()))?;
    let plan: BTreeMap<String, String> = serde_json::from_str(&raw).map_err(|e| {
        format!(
            "解析 visual_plan {} 失败（需要 JSON 对象：场景 ID → 描述）: {e}",
            path.display()
        )
    })?;
    for (scene_id, visual) in &plan {
        if scene_id.trim().is_empty() {
            return Err(format!("visual_plan {} 包含空场景 ID", path.display()));
        }
        if visual.trim().is_empty() {
            return Err(format!(
                "visual_plan {} 中场景 {} 的画面描述不能为空",
                path.display(),
                scene_id
            ));
        }
    }
    Ok(Some(plan))
}

fn print_visual_plan_warnings(
    visual_plan: Option<&BTreeMap<String, String>>,
    scenes: &[models::Scene],
) {
    let Some(visual_plan) = visual_plan else {
        return;
    };
    for scene_id in visual_plan.keys() {
        let short_id = scene_id.strip_prefix('s').unwrap_or(scene_id);
        let matched = scenes
            .iter()
            .any(|scene| scene.id == *scene_id || scene.id == format!("s{short_id}"));
        if !matched {
            println!("⚠ visual_plan 中的场景 {} 没有对应分句，将被忽略", scene_id);
        }
    }
}

fn read_storyboard(path: &PathBuf) -> Result<Storyboard, String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("读取 {} 失败: {e}", path.display()))?;
    serde_json::from_str(&raw).map_err(|e| format!("解析 {} 失败: {e}", path.display()))
}

fn parse_lang(s: &str) -> Result<Lang, String> {
    s.parse()
}

fn unknown_style(id: &str) -> String {
    format!("未知风格「{id}」，可用: {}", styles::ids().join(" / "))
}

fn err(msg: &str) -> ExitCode {
    eprintln!("错误: {msg}");
    ExitCode::from(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_title_prevents_path_traversal_and_control_chars() {
        assert_eq!(sanitize_title(" 春日/故事\\n "), "春日_故事_n");
        assert_eq!(sanitize_title(" 旅行 "), "旅行");
    }

    #[test]
    fn visual_plan_parser_validates_descriptions() {
        let path = std::env::temp_dir().join(format!(
            "agnes-video-free-visual-plan-{}.json",
            std::process::id()
        ));
        fs::write(&path, r#"{"s01":"a woman walking in the rain"}"#).unwrap();
        let plan = read_visual_plan(Some(&path)).unwrap().unwrap();
        assert_eq!(
            plan.get("s01").map(String::as_str),
            Some("a woman walking in the rain")
        );

        fs::write(&path, r#"{"s01":"   "}"#).unwrap();
        assert!(read_visual_plan(Some(&path)).is_err());
        fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn persisted_task_id_reuses_task_without_create_request() {
        use wiremock::matchers::{method, path, query_param};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/videos"))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/agnesapi"))
            .and(query_param("video_id", "task-existing"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "video_id": "task-existing",
                "status": "completed",
                "url": "https://cdn.example/video.mp4"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = AgnesClient::with_options(
            "test-key",
            &server.uri(),
            AgnesOptions {
                poll_interval: std::time::Duration::from_millis(1),
                poll_timeout: std::time::Duration::from_secs(2),
                ..AgnesOptions::default()
            },
        )
        .unwrap();
        let request = CreateVideoRequest::new("prompt", "negative", 720, 1280, 121, 24);
        let scene = models::Scene {
            id: "s01".to_string(),
            caption: "测试。".to_string(),
            narration: "测试。".to_string(),
            visual: None,
            prompt: Some("prompt".to_string()),
            negative_prompt: Some("negative".to_string()),
            narration_audio: None,
            motion_video: None,
            agnes_task_id: Some("task-existing".to_string()),
            duration_sec: None,
            num_frames: None,
        };

        let task_id = resolve_video_task_id(&client, &request, scene.agnes_task_id.as_deref())
            .await
            .unwrap();
        assert_eq!(task_id, "task-existing");
        let result = client.wait_for_video(&task_id).await.unwrap();
        assert_eq!(result.url, "https://cdn.example/video.mp4");
    }

    #[test]
    fn clean_removes_one_scene_and_updates_storyboard() {
        let root =
            std::env::temp_dir().join(format!("agnes-video-free-clean-{}", std::process::id()));
        let audio_dir = root.join("audio");
        let video_dir = root.join("video");
        let output = root.join("out/story.mp4");
        let clip = output.parent().unwrap().join(".agnes-video-free/s01.mp4");
        fs::create_dir_all(&audio_dir).unwrap();
        fs::create_dir_all(&video_dir).unwrap();
        fs::create_dir_all(clip.parent().unwrap()).unwrap();
        fs::create_dir_all(output.parent().unwrap()).unwrap();

        let storyboard_path = root.join("storyboard.json");
        let storyboard = Storyboard {
            title: "clean-test".to_string(),
            lang: "zh".to_string(),
            style: "realistic-cinematic".to_string(),
            width: 720,
            height: 1280,
            fps: 30,
            frame_rate_video: 24,
            scenes: vec![models::Scene {
                id: "s01".to_string(),
                caption: "测试场景。".to_string(),
                narration: "测试场景。".to_string(),
                visual: None,
                prompt: Some("prompt".to_string()),
                negative_prompt: Some("negative".to_string()),
                narration_audio: Some("audio/s01.mp3".to_string()),
                motion_video: Some("video/s01.mp4".to_string()),
                agnes_task_id: Some("task_s01".to_string()),
                duration_sec: Some(2.0),
                num_frames: Some(49),
            }],
        };
        fs::write(&storyboard_path, serde_json::to_vec(&storyboard).unwrap()).unwrap();
        fs::write(audio_dir.join("s01.mp3"), [1_u8]).unwrap();
        fs::write(video_dir.join("s01.mp4"), [1_u8]).unwrap();
        fs::write(&clip, [1_u8]).unwrap();

        let code = cmd_clean(&CleanArgs {
            storyboard: storyboard_path.clone(),
            scene: "s01".to_string(),
            stage: CleanStage::All,
            audio_dir,
            video_dir,
            output,
            dry_run: false,
            yes: true,
        });
        assert_eq!(code, ExitCode::SUCCESS);
        let updated: Storyboard =
            serde_json::from_slice(&fs::read(&storyboard_path).unwrap()).unwrap();
        assert!(updated.scenes[0].narration_audio.is_none());
        assert!(updated.scenes[0].motion_video.is_none());
        assert!(updated.scenes[0].agnes_task_id.is_none());
        assert!(updated.scenes[0].duration_sec.is_none());
        assert!(updated.scenes[0].num_frames.is_none());
        assert!(!clip.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn clean_rejects_path_like_scene_ids() {
        assert!(!safe_scene_id("../s01"));
        assert!(!safe_scene_id("s/01"));
        assert!(safe_scene_id("s01"));
    }

    #[tokio::test]
    async fn resume_skips_completed_media_without_api_request() {
        let root =
            std::env::temp_dir().join(format!("agnes-video-free-resume-{}", std::process::id()));
        let audio_dir = root.join("audio/narration");
        let video_dir = root.join("assets/videos");
        let output = root.join("out/story.mp4");
        fs::create_dir_all(&audio_dir).unwrap();
        fs::create_dir_all(&video_dir).unwrap();
        fs::create_dir_all(output.parent().unwrap()).unwrap();

        let storyboard = Storyboard {
            title: "resume-test".to_string(),
            lang: "zh".to_string(),
            style: "realistic-cinematic".to_string(),
            width: 720,
            height: 1280,
            fps: 30,
            frame_rate_video: 24,
            scenes: vec![models::Scene {
                id: "s01".to_string(),
                caption: "测试场景。".to_string(),
                narration: "测试场景。".to_string(),
                visual: None,
                prompt: Some("prompt".to_string()),
                negative_prompt: Some("negative".to_string()),
                narration_audio: None,
                motion_video: None,
                agnes_task_id: None,
                duration_sec: None,
                num_frames: None,
            }],
        };
        fs::write(
            root.join("storyboard.json"),
            serde_json::to_vec(&storyboard).unwrap(),
        )
        .unwrap();
        fs::write(audio_dir.join("s01.mp3"), [1_u8]).unwrap();
        fs::write(video_dir.join("s01.mp4"), vec![1_u8; MIN_VIDEO_BYTES]).unwrap();
        fs::write(&output, vec![1_u8; MIN_VIDEO_BYTES]).unwrap();

        let code = cmd_resume(&ResumeArgs {
            storyboard: root.join("storyboard.json"),
            audio_dir,
            video_dir,
            fonts_dir: root.join("assets/fonts"),
            output,
            voice: None,
            gender: Gender::Female,
            speed: 1.0,
            api_base_url: agnes::DEFAULT_BASE_URL.to_string(),
            poll_interval: 1,
            poll_timeout: 1,
        })
        .await;
        assert_eq!(code, ExitCode::SUCCESS);
        fs::remove_dir_all(root).unwrap();
    }
}
