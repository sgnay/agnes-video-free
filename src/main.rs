mod models;
mod pipeline;
mod split;
mod styles;
mod tts;

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

use models::{Lang, Storyboard};
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
    /// 组装成片（ffmpeg + libass；M2 实现）
    Assemble,
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
    /// 并发数（免费 key 限流 1 req/min，默认 1）
    #[arg(long, default_value_t = 1)]
    concurrency: u32,
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
        Some(Command::Video(a)) => {
            eprintln!(
                "「video」尚未实现（计划 M1：Agnes 异步任务+轮询；--concurrency {}）",
                a.concurrency
            );
            ExitCode::from(1)
        }
        Some(Command::Assemble) => {
            eprintln!("「assemble」尚未实现（计划 M2：ffmpeg + libass）");
            ExitCode::from(1)
        }
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
