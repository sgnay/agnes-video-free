# agnes-video-free

用 Rust 编写的故事短视频自动生成工具：把一段中文/英文故事文本变成带旁白、字幕的竖屏短视频，
发布到 TikTok / 小红书 / 微博。

**工具链（全免费）**：Agnes Video V2.0 纯文生视频（当前 $0/秒，国际站 `https://apihub.agnes-ai.com`）
+ Rust 原生 edge-tts 免费旁白 + ffmpeg/libass 确定性字幕渲染（无 Node.js / Chrome 依赖）。

> 借鉴 [story-handdrawn-video](https://github.com/liangdabiao/story-handdrawn-video) 的方法论：
> 先 TTS 定时长 → 纯文生视频 → 字幕永远本地渲染。完整规划见 [PLAN.md](PLAN.md)。

## 快速开始

```bash
# 环境变量（或在工作区放 .env 一行 AGNES_API_KEY=sk-...）
export AGNES_API_KEY=sk-...

# ① 分句 + 预览 prompt（不请求任何生成 API）
agnes-video-free all examples/story_realistic.txt --dry-run

# ② 分句 → storyboard.json
agnes-video-free split examples/story_realistic.txt

# ③ 生成旁白 mp3（edge-tts，免费；已存在的自动跳过）
agnes-video-free tts

# ④ 生成视频片段 / ⑤ 组装成片（M1/M2 开发中）
```

风格：`realistic-cinematic`（TikTok 9:16）/ `realistic-vlog`（小红书 3:4）/
`realistic-documentary`（微博 16:9）。配方见 [references/prompt-recipes.md](references/prompt-recipes.md)。

## NixOS 打包与运行

项目按 [simple-translation](https://github.com/sgnay/simple-translation) 的方式提供 Nix Flake，
使用锁定的 nixpkgs 与 flake-utils 管理 Rust 编译器、ffmpeg（含 ffprobe）、CA 证书与随包 OFL 字体。
NixOS 用户需要启用 Flakes（例如在 NixOS 配置中设置 `nix.settings.experimental-features = [ "nix-command" "flakes" ];`）。

```bash
# 进入包含 Rust 和 ffmpeg 的开发环境
nix develop

# 编译 Nix 包，结果位于 ./result/bin/agnes-video-free
nix build

# 直接在 Nix 运行时环境中启动（含 ffmpeg 与证书）
nix run .# -- all examples/story_realistic.txt --dry-run

# 运行单元测试
nix develop --command cargo test
```

仍可使用传统入口进入开发环境：

```bash
nix-shell
nix-shell --run "cargo build --release"
```

Flake 包会自动注入 `PATH`（ffmpeg/ffprobe）、`SSL_CERT_FILE`（cacert）并安装随包字体
（`AGNES_VIDEO_FREE_FONTS`，供字幕渲染使用）。

## 项目结构

```
src/
├── main.rs        # clap 子命令：split / tts / video / assemble / all
├── models.rs      # Storyboard / Scene / StyleProfile / num_frames 计算
├── split.rs       # 中英文分句（一句一拍）
├── pipeline.rs    # 全流程编排（分句 → prompt → storyboard）
├── styles/        # 风格注册表（realistic 族 + 共享负向词基线）
└── tts/           # Rust 原生 edge-tts（kothok-edge-tts）
assets/fonts/      # 思源黑体 SC（OFL）
references/        # prompt 配方 / 流程细节
```

## 开发状态

- M0 ✅ 脚手架、clap 子命令、中英文分句、dry-run 预览
- M1 🚧 TTS 已完成（edge-tts 选型 spike 通过）；ffprobe 封装与 Agnes 视频 API 客户端进行中
- M2 成片组装（ffmpeg + libass）；M3 交互模式 + agent skill；M4 GUI
