mod agnes;
mod media;
mod models;
mod pipeline;
mod split;
mod styles;
mod tts;

use std::fs;
use std::path::PathBuf;
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
}

#[derive(Args)]
struct SplitArgs {
    /// 故事文本路径（UTF-8）
    story: PathBuf,
    /// 语言：zh | en
    #[arg(long, default_value = "zh")]
    lang: String,
    /// 风格 id（realistic-cinematic / realistic-vlog / realistic-documentary）
    #[arg(long, default_value = "realistic-cinematic")]
    style: String,
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
    /// dry-run：只预览分句与 prompt，不写文件
    #[arg(long)]
    dry_run: bool,
    /// 输出 storyboard.json 路径
    #[arg(long, default_value = "storyboard.json")]
    out: PathBuf,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        None => list_styles(),
        Some(Command::Split(a)) => cmd_split(&a),
        Some(Command::Tts(a)) => cmd_tts(&a).await,
        Some(Command::Video(a)) => cmd_video(&a).await,
        Some(Command::Assemble(a)) => cmd_assemble(&a),
        Some(Command::All(a)) => cmd_all(&a),
    }
}

/// 默认入口：输出可用风格与完整三段式配置总览（交互式向导后续接入）。
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

    let scenes = pipeline::plan_scenes(&story, lang, &style);
    print_scenes(&scenes);

    let title = args
        .story
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "未命名".to_string());
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
        if out.exists() {
            println!("  {} 已存在，跳过", out.display());
            skipped += 1;
            continue;
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
        let audio_path = args.audio_dir.join(format!("{}.mp3", scene.id));
        let video_path = args.out_dir.join(format!("{}.mp4", scene.id));

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

        println!(
            "  {} 提交任务（旁白 {:.2}s → {} 帧）…",
            scene.id, duration, num_frames
        );
        let task = match client.create_video(&request).await {
            Ok(task) => task,
            Err(e) => {
                eprintln!("  {} 创建任务失败: {e}", scene.id);
                failed += 1;
                continue;
            }
        };
        println!("  {} 任务 {}，开始轮询…", scene.id, task.video_id);
        let result = match client.wait_for_video(&task.video_id).await {
            Ok(result) => result,
            Err(e) => {
                eprintln!("  {} 任务失败: {e}", scene.id);
                failed += 1;
                continue;
            }
        };
        if let Err(e) = client.download_video(&result.url, &video_path).await {
            eprintln!("  {} 下载失败: {e}", scene.id);
            failed += 1;
            continue;
        }

        println!("  {} 完成 → {}", scene.id, video_path.display());
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

    let scenes = pipeline::plan_scenes(&story, lang, &style);

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
        println!("{}", s.prompt.as_deref().unwrap_or_default());
        if lang == Lang::En {
            for issue in realistic::validate_scene_body(&s.caption) {
                println!("  ⚠ {issue}");
            }
        }
    }
    if lang == Lang::Zh {
        println!();
        println!(
            "注: SCENE_BODY 为中文原句直塞（Agnes 可理解中文）；写实风格建议后续提供英文 visual_plan 以获得更稳定的画面。"
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
