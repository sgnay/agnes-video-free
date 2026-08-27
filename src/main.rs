mod agnes;
mod media;
mod models;
mod pipeline;
mod styles;

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

use agnes::{AgnesClient, AgnesOptions, CreateVideoRequest, MIN_VIDEO_BYTES};
use media::ffmpeg;
use models::{Lang, Scene, Storyboard, VisualSceneSpec, num_frames_for_duration};

#[derive(Parser)]
#[command(
    name = "agnes-video-free",
    version,
    about = "按 visual_plan 生成稳定的视觉场景，并叠加独立音轨、音乐和字幕"
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
    /// 读取 visual_plan.v2.json，生成视觉 storyboard
    Split(SplitArgs),
    /// 按 storyboard 中的视觉场景生成 Agnes 视频
    Video(VideoArgs),
    /// 拼接视觉视频并叠加独立音轨、BGM 和字幕
    Assemble(AssembleArgs),
    /// 查看视觉场景、任务和视频状态
    Status(StatusArgs),
    /// 继续轮询或生成缺失的视觉视频，然后按需重新组装
    Resume(ResumeArgs),
    /// 安全清理单个视觉场景的视频产物
    Clean(CleanArgs),
}

#[derive(Args)]
struct SplitArgs {
    /// visual_plan.v2.json，必须是 {"scenes":[...]} 格式
    #[arg(long)]
    visual_plan: PathBuf,
    /// 标题（默认使用 visual_plan 文件名）
    #[arg(long)]
    title: Option<String>,
    /// storyboard 语言元数据：zh | en
    #[arg(long, default_value = "zh")]
    lang: String,
    /// 风格 id
    #[arg(long, default_value = "realistic-cinematic")]
    style: String,
    /// 输出 storyboard.json 路径
    #[arg(long, default_value = "storyboard.json")]
    out: PathBuf,
}

#[derive(Args)]
struct VideoArgs {
    /// storyboard.json 路径
    #[arg(long, default_value = "storyboard.json")]
    storyboard: PathBuf,
    /// 视频片段输出目录（每场写入 <id>.mp4）
    #[arg(long, default_value = "assets/videos")]
    out_dir: PathBuf,
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
struct AssembleArgs {
    /// storyboard.json 路径
    #[arg(long, default_value = "storyboard.json")]
    storyboard: PathBuf,
    /// 视频目录（场景未记录路径时使用）
    #[arg(long, default_value = "assets/videos")]
    video_dir: PathBuf,
    /// 独立完整音轨或旁白（可选）
    #[arg(long)]
    audio: Option<PathBuf>,
    /// 背景音乐（可选，会循环并以较低音量混入）
    #[arg(long)]
    bgm: Option<PathBuf>,
    /// 独立字幕文件（支持 .srt / .lrc）
    #[arg(long)]
    subtitles: Option<PathBuf>,
    /// 字体目录
    #[arg(long, default_value = "assets/fonts")]
    fonts_dir: PathBuf,
    /// 最终输出 MP4
    #[arg(long, default_value = "out/story.mp4")]
    output: PathBuf,
}

#[derive(Args)]
struct StatusArgs {
    /// storyboard.json 路径
    #[arg(long, default_value = "storyboard.json")]
    storyboard: PathBuf,
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
    /// 视频片段目录
    #[arg(long, default_value = "assets/videos")]
    video_dir: PathBuf,
    /// 字体目录
    #[arg(long, default_value = "assets/fonts")]
    fonts_dir: PathBuf,
    /// 最终输出路径
    #[arg(long, default_value = "out/story.mp4")]
    output: PathBuf,
    /// 独立完整音轨或旁白（可选）
    #[arg(long)]
    audio: Option<PathBuf>,
    /// 背景音乐（可选）
    #[arg(long)]
    bgm: Option<PathBuf>,
    /// 独立字幕文件（支持 .srt / .lrc，可选）
    #[arg(long)]
    subtitles: Option<PathBuf>,
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
    /// 要清理的单个场景 ID，例如 v07
    #[arg(long)]
    scene: String,
    /// 清理阶段：video / clip / all
    #[arg(long, value_enum, default_value_t = CleanStage::All)]
    stage: CleanStage,
    /// 视频目录
    #[arg(long, default_value = "assets/videos")]
    video_dir: PathBuf,
    /// 最终输出路径（用于定位临时 clip；最终成片不会被删除）
    #[arg(long, default_value = "out/story.mp4")]
    output: PathBuf,
    /// 只显示待删除文件，不执行删除
    #[arg(long)]
    dry_run: bool,
    /// 确认执行删除
    #[arg(long)]
    yes: bool,
}

#[derive(clap::ValueEnum, Clone, Copy)]
enum CleanStage {
    Video,
    Clip,
    All,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        None | Some(Command::Interactive) => cmd_interactive().await,
        Some(Command::Styles) => list_styles(),
        Some(Command::Split(args)) => cmd_split(&args),
        Some(Command::Video(args)) => cmd_video(&args).await,
        Some(Command::Assemble(args)) => cmd_assemble(&args),
        Some(Command::Status(args)) => cmd_status(&args),
        Some(Command::Resume(args)) => cmd_resume(&args).await,
        Some(Command::Clean(args)) => cmd_clean(&args),
    }
}

fn cmd_split(args: &SplitArgs) -> ExitCode {
    let lang = match args.lang.parse::<Lang>() {
        Ok(lang) => lang,
        Err(error) => return err(&error),
    };
    let Some(style) = styles::by_id(&args.style) else {
        return err(&unknown_style(&args.style));
    };
    let visual_scenes = match read_visual_scene_plan(&args.visual_plan) {
        Ok(scenes) => scenes,
        Err(error) => return err(&error),
    };
    let title = args.title.clone().unwrap_or_else(|| {
        args.visual_plan
            .file_stem()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| "story".to_string())
    });
    let storyboard = pipeline::build_visual_storyboard(&title, lang, &style, visual_scenes);
    print_visual_scenes(&storyboard.scenes);
    match write_storyboard(&storyboard, &args.out) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => err(&error),
    }
}

async fn cmd_video(args: &VideoArgs) -> ExitCode {
    dotenvy::dotenv().ok();
    let api_key = match std::env::var("AGNES_API_KEY") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => return err("未找到 AGNES_API_KEY，请设置环境变量或在当前目录 .env 中配置"),
    };
    let mut storyboard = match read_storyboard(&args.storyboard) {
        Ok(storyboard) => storyboard,
        Err(error) => return err(&error),
    };
    if storyboard.scenes.is_empty() {
        return err("storyboard 没有任何视觉场景");
    }
    let client = match AgnesClient::with_options(
        api_key,
        &args.api_base_url,
        AgnesOptions {
            poll_interval: std::time::Duration::from_secs(args.poll_interval),
            poll_timeout: std::time::Duration::from_secs(args.poll_timeout),
            ..AgnesOptions::default()
        },
    ) {
        Ok(client) => client,
        Err(error) => return err(&error.to_string()),
    };
    if let Err(error) = fs::create_dir_all(&args.out_dir) {
        return err(&format!("创建视频输出目录失败: {error}"));
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
        let video_path = args.out_dir.join(format!("{scene_id}.mp4"));
        let duration = scene.duration_sec;
        let num_frames = num_frames_for_duration(duration);

        if has_file(&video_path, MIN_VIDEO_BYTES) {
            println!("  {scene_id} 已存在，跳过（{duration:.2}s，{num_frames} 帧）");
            storyboard.scenes[index].motion_video = Some(video_path.display().to_string());
            storyboard.scenes[index].num_frames = num_frames;
            skipped += 1;
            if let Err(error) = write_storyboard(&storyboard, &args.storyboard) {
                return err(&error);
            }
            continue;
        }
        if video_path.exists() {
            eprintln!("  {scene_id} 视频文件无效，将重新生成");
        }

        let request = CreateVideoRequest::new(
            scene.prompt.clone(),
            scene.negative_prompt.clone(),
            storyboard.width,
            storyboard.height,
            num_frames,
            storyboard.frame_rate_video,
        );
        let existing_task_id = scene.agnes_task_id.clone();
        let reusing_task = existing_task_id
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty());
        println!(
            "  {scene_id} {}（{duration:.2}s，{num_frames} 帧）…",
            if reusing_task {
                "继续轮询已记录任务"
            } else {
                "提交任务"
            }
        );
        let task_id =
            match resolve_video_task_id(&client, &request, existing_task_id.as_deref()).await {
                Ok(task_id) => task_id,
                Err(error) => {
                    eprintln!("  {scene_id} 创建/复用任务失败: {error}");
                    failed += 1;
                    continue;
                }
            };
        if !reusing_task {
            storyboard.scenes[index].agnes_task_id = Some(task_id.clone());
            if let Err(error) = write_storyboard(&storyboard, &args.storyboard) {
                return err(&error);
            }
        }
        let result = match client.wait_for_video(&task_id).await {
            Ok(result) => result,
            Err(error) => {
                if matches!(&error, agnes::AgnesError::FailedTask(_)) {
                    storyboard.scenes[index].agnes_task_id = None;
                    if let Err(write_error) = write_storyboard(&storyboard, &args.storyboard) {
                        return err(&write_error);
                    }
                }
                eprintln!("  {scene_id} 任务失败: {error}");
                failed += 1;
                continue;
            }
        };
        if let Err(error) = client.download_video(&result.url, &video_path).await {
            eprintln!("  {scene_id} 下载失败: {error}");
            failed += 1;
            continue;
        }
        storyboard.scenes[index].motion_video = Some(video_path.display().to_string());
        storyboard.scenes[index].num_frames = num_frames;
        generated += 1;
        println!("  {scene_id} 完成 → {}", video_path.display());
        if let Err(error) = write_storyboard(&storyboard, &args.storyboard) {
            return err(&error);
        }
    }
    println!("视频生成完成: 新增 {generated}，跳过 {skipped}，失败 {failed}");
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

async fn resolve_video_task_id(
    client: &AgnesClient,
    request: &CreateVideoRequest,
    existing_task_id: Option<&str>,
) -> Result<String, agnes::AgnesError> {
    if let Some(task_id) = existing_task_id.filter(|value| !value.trim().is_empty()) {
        return Ok(task_id.to_string());
    }
    Ok(client.create_video(request).await?.video_id)
}

fn cmd_assemble(args: &AssembleArgs) -> ExitCode {
    let mut storyboard = match read_storyboard(&args.storyboard) {
        Ok(storyboard) => storyboard,
        Err(error) => return err(&error),
    };
    let fonts_dir = match resolve_fonts_dir(&args.fonts_dir) {
        Ok(path) => path,
        Err(error) => return err(&error),
    };
    let tracks = ffmpeg::AudioTracks {
        audio: args.audio.as_deref(),
        bgm: args.bgm.as_deref(),
        subtitles: args.subtitles.as_deref(),
    };
    println!(
        "开始组装: {} 场 → {}（字体: {}）",
        storyboard.scenes.len(),
        args.output.display(),
        fonts_dir.display()
    );
    let result = match ffmpeg::assemble_storyboard_with_tracks(
        &mut storyboard,
        &args.video_dir,
        &fonts_dir,
        tracks,
        &args.output,
    ) {
        Ok(result) => result,
        Err(error) => return err(&error.to_string()),
    };
    if let Err(error) = write_storyboard(&storyboard, &args.storyboard) {
        return err(&error);
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

async fn cmd_resume(args: &ResumeArgs) -> ExitCode {
    dotenvy::dotenv().ok();
    let storyboard = match read_storyboard(&args.storyboard) {
        Ok(storyboard) => storyboard,
        Err(error) => return err(&error),
    };
    if storyboard.scenes.is_empty() {
        return err("storyboard 没有任何视觉场景");
    }
    let needs_video = storyboard.scenes.iter().any(|scene| {
        !has_file(
            &args.video_dir.join(format!("{}.mp4", scene.id)),
            MIN_VIDEO_BYTES,
        )
    });
    let output_exists = has_file(&args.output, MIN_VIDEO_BYTES);
    if needs_video {
        println!("恢复阶段 1/2：补齐缺失视觉视频（已完成文件自动跳过）");
        let code = cmd_video(&VideoArgs {
            storyboard: args.storyboard.clone(),
            out_dir: args.video_dir.clone(),
            api_base_url: args.api_base_url.clone(),
            poll_interval: args.poll_interval,
            poll_timeout: args.poll_timeout,
        })
        .await;
        if code != ExitCode::SUCCESS {
            return code;
        }
    } else {
        println!("恢复阶段 1/2：视觉视频已齐全，跳过（不会请求 Agnes API）");
    }
    if !needs_video && output_exists {
        println!(
            "恢复阶段 2/2：最终成片已存在，跳过组装 → {}",
            args.output.display()
        );
        return ExitCode::SUCCESS;
    }
    if needs_video && output_exists {
        println!("恢复阶段 2/2：素材状态变化，将重新组装并覆盖现有成片");
    }
    println!("恢复阶段 2/2：组装最终成片");
    cmd_assemble(&AssembleArgs {
        storyboard: args.storyboard.clone(),
        video_dir: args.video_dir.clone(),
        audio: args.audio.clone(),
        bgm: args.bgm.clone(),
        subtitles: args.subtitles.clone(),
        fonts_dir: args.fonts_dir.clone(),
        output: args.output.clone(),
    })
}

fn cmd_status(args: &StatusArgs) -> ExitCode {
    let storyboard = match read_storyboard(&args.storyboard) {
        Ok(storyboard) => storyboard,
        Err(error) => return err(&error),
    };
    if storyboard.scenes.is_empty() {
        return err("storyboard 没有任何视觉场景");
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
    println!("场景     时长      任务                  视频      clip      状态");
    println!("────────────────────────────────────────────────────────");
    let mut video_done = 0;
    let mut clip_done = 0;
    for scene in &storyboard.scenes {
        let video = scene_path(scene.motion_video.as_deref(), &args.video_dir, &scene.id);
        let clip = clip_dir.join(format!("{}.mp4", scene.id));
        let video_ok = has_file(&video, MIN_VIDEO_BYTES);
        let clip_ok = has_file(&clip, 1);
        video_done += usize::from(video_ok);
        clip_done += usize::from(clip_ok);
        let state = if clip_ok {
            "ready"
        } else if video_ok {
            "待组装"
        } else if scene.agnes_task_id.is_some() {
            "任务待轮询"
        } else {
            "待视频"
        };
        println!(
            "{:<8} {:<9.1} {:<20} {:<9} {:<9} {}",
            scene.id,
            scene.duration_sec,
            scene.agnes_task_id.as_deref().unwrap_or("—"),
            marker(video_ok),
            marker(clip_ok),
            state
        );
    }
    println!(
        "视频 {}/{}，clip {}/{}",
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

fn cmd_clean(args: &CleanArgs) -> ExitCode {
    let mut storyboard = match read_storyboard(&args.storyboard) {
        Ok(storyboard) => storyboard,
        Err(error) => return err(&error),
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
    let video = args.video_dir.join(format!("{}.mp4", args.scene));
    let clip = clip_dir.join(format!("{}.mp4", args.scene));
    let targets: Vec<(&str, PathBuf)> = match args.stage {
        CleanStage::Video => vec![("video", video), ("clip", clip)],
        CleanStage::Clip => vec![("clip", clip)],
        CleanStage::All => vec![("video", video), ("clip", clip)],
    };
    println!(
        "清理场景 {}（阶段: {}）",
        args.scene,
        clean_stage_label(args.stage)
    );
    for (kind, path) in &targets {
        println!(
            "  {:<5} {} {}",
            kind,
            if path.exists() { "删除" } else { "不存在" },
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
    let mut video_cleared = false;
    let mut failures = Vec::new();
    for (kind, path) in &targets {
        match remove_file_if_present(path) {
            Ok(true) => {
                println!("  ✓ 已清理 {}: {}", kind, path.display());
                video_cleared |= *kind == "video";
            }
            Ok(false) => {
                println!("  - {} 不存在，跳过", path.display());
                video_cleared |= *kind == "video";
            }
            Err(error) => failures.push(error),
        }
    }
    if video_cleared {
        storyboard.scenes[scene_index].motion_video = None;
        storyboard.scenes[scene_index].agnes_task_id = None;
    }
    if let Err(error) = write_storyboard(&storyboard, &args.storyboard) {
        return err(&error);
    }
    if failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        err(&failures.join("；"))
    }
}

fn remove_file_if_present(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() || metadata.file_type().is_symlink() => {
            fs::remove_file(path)
                .map(|()| true)
                .map_err(|error| format!("删除 {} 失败: {error}", path.display()))
        }
        Ok(_) => Err(format!("{} 不是普通文件，拒绝删除", path.display())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("检查 {} 失败: {error}", path.display())),
    }
}

fn clean_stage_label(stage: CleanStage) -> &'static str {
    match stage {
        CleanStage::Video => "video",
        CleanStage::Clip => "clip",
        CleanStage::All => "all",
    }
}

fn safe_scene_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

fn has_file(path: &Path, min_bytes: usize) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.len() >= min_bytes as u64)
        .unwrap_or(false)
}

fn scene_path(stored: Option<&str>, fallback_dir: &Path, id: &str) -> PathBuf {
    stored
        .map(PathBuf::from)
        .unwrap_or_else(|| fallback_dir.join(format!("{id}.mp4")))
}

fn resolve_fonts_dir(path: &Path) -> Result<PathBuf, String> {
    if path.is_dir() {
        return Ok(path.to_path_buf());
    }
    if let Ok(value) = std::env::var("AGNES_VIDEO_FREE_FONTS") {
        let fallback = PathBuf::from(value);
        if fallback.is_dir() {
            return Ok(fallback);
        }
    }
    Err(format!("字体目录不存在: {}", path.display()))
}

async fn cmd_interactive() -> ExitCode {
    println!("╔══════════════════════════════════════════════════════╗");
    println!("║       agnes-video-free 视觉视频交互式向导            ║");
    println!("╚══════════════════════════════════════════════════════╝");
    println!("输入 q 可在任意配置步骤取消。视觉场景与音频、字幕始终独立。\n");

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
        Err(error) => return wizard_error(&error),
    };
    let style = profiles[style_index].clone();
    let lang_options = vec!["中文（zh）".to_string(), "English（en）".to_string()];
    let lang_index = match prompt_choice("选择 storyboard 语言", &lang_options, 0) {
        Ok(index) => index,
        Err(error) => return wizard_error(&error),
    };
    let lang = if lang_index == 0 { Lang::Zh } else { Lang::En };
    let plan_path = match prompt_line(
        "visual_plan.v2.json 路径",
        Some("examples/visual_plan.v2.example.json"),
    ) {
        Ok(value) => PathBuf::from(value),
        Err(error) => return wizard_error(&error),
    };
    let visual_scenes = match read_visual_scene_plan(&plan_path) {
        Ok(scenes) => scenes,
        Err(error) => return wizard_error(&error),
    };
    let project_dir = match prompt_line("项目输出目录", Some(".")) {
        Ok(value) => PathBuf::from(value),
        Err(error) => return wizard_error(&error),
    };
    let default_title = plan_path
        .file_stem()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "story".to_string());
    let title = match prompt_line("成片标题", Some(&default_title)) {
        Ok(value) if !value.trim().is_empty() => sanitize_title(&value),
        Ok(_) => return wizard_error("成片标题不能为空"),
        Err(error) => return wizard_error(&error),
    };
    if title.is_empty() {
        return wizard_error("成片标题不能只包含路径分隔符或控制字符");
    }
    let audio = match prompt_optional_file("主音轨/旁白文件（可留空）") {
        Ok(path) => path,
        Err(error) => return wizard_error(&error),
    };
    let bgm = match prompt_optional_file("背景音乐文件（可留空）") {
        Ok(path) => path,
        Err(error) => return wizard_error(&error),
    };
    let subtitles = match prompt_optional_file("字幕文件 .srt/.lrc（可留空）") {
        Ok(path) => path,
        Err(error) => return wizard_error(&error),
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
    let video_dir = project_dir.join("assets/videos");
    let fonts_dir = project_dir.join("assets/fonts");
    let output = project_dir.join("out").join(format!("{title}.mp4"));
    let preview = pipeline::build_visual_storyboard(&title, lang, &style, visual_scenes);
    println!("\n═══ 配置确认 ═══");
    println!("模式: 独立视觉场景 + 外部轨道");
    println!(
        "视觉场景: {} 场 | 总时长: {:.1}s",
        preview.scenes.len(),
        preview
            .scenes
            .iter()
            .map(|scene| scene.duration_sec)
            .sum::<f64>()
    );
    println!(
        "风格: {}（{}x{}）",
        style.id, style.canvas.0, style.canvas.1
    );
    println!("主音轨: {}", optional_path_display(audio.as_deref()));
    println!("背景音乐: {}", optional_path_display(bgm.as_deref()));
    println!("字幕: {}", optional_path_display(subtitles.as_deref()));
    println!("输出: {}", output.display());
    print_visual_scenes(&preview.scenes);
    let confirmed = match prompt_yes_no("确认开始生成视觉视频并组装？", true) {
        Ok(value) => value,
        Err(error) => return wizard_error(&error),
    };
    if !confirmed {
        println!("已取消，未执行生成。");
        return ExitCode::SUCCESS;
    }
    if let Err(error) = fs::create_dir_all(&project_dir) {
        return err(&format!("创建项目目录失败: {error}"));
    }
    println!("\n═══ 1/3 写入视觉 storyboard ═══");
    let code = cmd_split(&SplitArgs {
        visual_plan: plan_path,
        title: Some(title.clone()),
        lang: lang.label().to_string(),
        style: style.id.to_string(),
        out: storyboard.clone(),
    });
    if code != ExitCode::SUCCESS {
        return code;
    }
    println!("\n═══ 2/3 生成视觉视频 ═══");
    let code = cmd_video(&VideoArgs {
        storyboard: storyboard.clone(),
        out_dir: video_dir.clone(),
        api_base_url: agnes::DEFAULT_BASE_URL.to_string(),
        poll_interval: 8,
        poll_timeout: 900,
    })
    .await;
    if code != ExitCode::SUCCESS {
        return code;
    }
    println!("\n═══ 3/3 混音、字幕与成片组装 ═══");
    let code = cmd_assemble(&AssembleArgs {
        storyboard,
        video_dir,
        audio,
        bgm,
        subtitles,
        fonts_dir,
        output: output.clone(),
    });
    if code == ExitCode::SUCCESS {
        println!("\n向导完成，成片位于: {}", output.display());
    }
    code
}

fn optional_path_display(path: Option<&Path>) -> String {
    path.map(|value| value.display().to_string())
        .unwrap_or_else(|| "—".to_string())
}

fn prompt_optional_file(label: &str) -> Result<Option<PathBuf>, String> {
    let value = prompt_line(label, None)?;
    if value.is_empty() {
        return Ok(None);
    }
    let path = PathBuf::from(value);
    if path.is_file() {
        Ok(Some(path))
    } else {
        Err(format!("文件不存在或不是普通文件: {}", path.display()))
    }
}

fn prompt_line(label: &str, default: Option<&str>) -> Result<String, String> {
    match default {
        Some(value) => print!("{label} [{value}]: "),
        None => print!("{label}: "),
    }
    io::stdout()
        .flush()
        .map_err(|error| format!("刷新终端输出失败: {error}"))?;
    let mut value = String::new();
    if io::stdin()
        .read_line(&mut value)
        .map_err(|error| format!("读取终端输入失败: {error}"))?
        == 0
    {
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
        println!("  {}. {option}", index + 1);
    }
    loop {
        let value = prompt_line(
            &format!("请输入编号（默认 {}）", default + 1),
            Some(&(default + 1).to_string()),
        )?;
        let number = match value.parse::<usize>() {
            Ok(number) => number,
            Err(_) => {
                println!("请输入有效编号。");
                continue;
            }
        };
        if (1..=options.len()).contains(&number) {
            return Ok(number - 1);
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

fn sanitize_title(value: &str) -> String {
    value
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
    if matches!(message, "用户取消向导" | "输入已结束，向导取消") {
        println!("\n{message}。");
        ExitCode::SUCCESS
    } else {
        err(message)
    }
}

fn list_styles() -> ExitCode {
    let styles = styles::all();
    println!(
        "agnes-video-free v{} — 可用风格（{}）",
        env!("CARGO_PKG_VERSION"),
        styles.len()
    );
    for style in &styles {
        println!("\n[{}] {} — {}", style.id, style.name, style.description);
        println!(
            "  平台 {} | 画幅 {}x{} | {}",
            style.default_platform.label(),
            style.canvas.0,
            style.canvas.1,
            style.aspect_line()
        );
        println!("  STYLE_HEADER:  {}", style.style_header());
        println!("  MOTION_FOOTER: {}", style.motion_footer);
        println!("  NEGATIVE:      {}", style.negative);
    }
    ExitCode::SUCCESS
}

fn print_visual_scenes(scenes: &[Scene]) {
    for scene in scenes {
        println!(
            "  {} [{:.1}s / {} 帧] {}",
            scene.id, scene.duration_sec, scene.num_frames, scene.visual
        );
    }
}

fn write_storyboard(storyboard: &Storyboard, out: &Path) -> Result<(), String> {
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("创建 {} 失败: {error}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(storyboard).map_err(|error| error.to_string())?;
    fs::write(out, json + "\n").map_err(|error| format!("写入 {} 失败: {error}", out.display()))?;
    println!("✓ storyboard 已写入 {}", out.display());
    Ok(())
}

fn read_storyboard(path: &Path) -> Result<Storyboard, String> {
    let raw = fs::read_to_string(path)
        .map_err(|error| format!("读取 {} 失败: {error}", path.display()))?;
    serde_json::from_str(&raw).map_err(|error| format!("解析 {} 失败: {error}", path.display()))
}

fn read_visual_scene_plan(path: &Path) -> Result<Vec<VisualSceneSpec>, String> {
    let raw = fs::read_to_string(path)
        .map_err(|error| format!("读取 visual_plan {} 失败: {error}", path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|error| format!("解析 visual_plan {} 失败: {error}", path.display()))?;
    let scenes_value = value.get("scenes").ok_or_else(|| {
        format!(
            "visual_plan {} 必须使用 {{\"scenes\":[...]}} 格式；旧版 map 格式已移除",
            path.display()
        )
    })?;
    let scenes: Vec<VisualSceneSpec> =
        serde_json::from_value(scenes_value.clone()).map_err(|error| {
            format!(
                "解析 visual_plan {} 失败（每项需要 id、visual、duration_sec）: {error}",
                path.display()
            )
        })?;
    if scenes.is_empty() {
        return Err(format!("visual_plan {} 的 scenes 不能为空", path.display()));
    }
    let mut ids = std::collections::HashSet::new();
    for scene in &scenes {
        if !safe_scene_id(&scene.id) {
            return Err(format!(
                "visual_plan {} 包含不安全场景 ID: {}",
                path.display(),
                scene.id
            ));
        }
        if !ids.insert(&scene.id) {
            return Err(format!(
                "visual_plan {} 包含重复场景 ID: {}",
                path.display(),
                scene.id
            ));
        }
        if scene.visual.trim().is_empty() {
            return Err(format!(
                "visual_plan {} 中场景 {} 的画面描述不能为空",
                path.display(),
                scene.id
            ));
        }
        if !scene.duration_sec.is_finite() || !(1.7..=18.3).contains(&scene.duration_sec) {
            return Err(format!(
                "visual_plan {} 中场景 {} 的 duration_sec 必须在 1.7 到 18.3 秒之间",
                path.display(),
                scene.id
            ));
        }
    }
    Ok(scenes)
}

fn unknown_style(id: &str) -> String {
    format!("未知风格「{id}」，可用: {}", styles::ids().join(" / "))
}

fn marker(done: bool) -> &'static str {
    if done { "✓" } else { "—" }
}

fn err(message: &str) -> ExitCode {
    eprintln!("错误: {message}");
    ExitCode::from(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_plan_parser_rejects_legacy_map_and_invalid_duration() {
        let path =
            std::env::temp_dir().join(format!("agnes-video-free-plan-{}.json", std::process::id()));
        fs::write(&path, r#"{"s01":"legacy"}"#).unwrap();
        assert!(read_visual_scene_plan(&path).is_err());
        fs::write(&path, r#"{"scenes":[{"id":"v01","visual":"a street, morning light, slow tracking shot","duration_sec":20.0}]}"#).unwrap();
        assert!(read_visual_scene_plan(&path).is_err());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn visual_plan_parser_accepts_scenes_array_and_rejects_duplicate_ids() {
        let path = std::env::temp_dir().join(format!(
            "agnes-video-free-scenes-{}.json",
            std::process::id()
        ));
        fs::write(&path, r#"{"scenes":[{"id":"v01","visual":"a street, morning light, slow tracking shot","duration_sec":8.0}]}"#).unwrap();
        let plan = read_visual_scene_plan(&path).unwrap();
        assert_eq!(plan[0].id, "v01");
        fs::write(&path, r#"{"scenes":[{"id":"v01","visual":"one, morning light, slow shot","duration_sec":8.0},{"id":"v01","visual":"two, morning light, slow shot","duration_sec":8.0}]}"#).unwrap();
        assert!(read_visual_scene_plan(&path).is_err());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn sanitize_title_prevents_path_traversal() {
        assert_eq!(sanitize_title(" 春日/故事\\n "), "春日_故事_n");
        assert_eq!(sanitize_title(" 旅行 "), "旅行");
    }

    #[test]
    fn clean_rejects_path_like_scene_ids() {
        assert!(!safe_scene_id("../v01"));
        assert!(!safe_scene_id("v/01"));
        assert!(safe_scene_id("v01"));
    }
}
