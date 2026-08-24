# agnes-video-free

用 Rust 编写的故事短视频自动生成工具：把一段中文/英文故事文本变成带旁白、字幕的竖屏短视频，
发布到 TikTok / 小红书 / 微博。

**工具链（全免费）**：Agnes Video V2.0 纯文生视频（当前 $0/秒，国际站 `https://apihub.agnes-ai.com`）
+ Rust 原生 edge-tts 免费旁白 + ffmpeg/libass 确定性字幕渲染（无 Node.js / Chrome 依赖）。

> 借鉴 [story-handdrawn-video](https://github.com/liangdabiao/story-handdrawn-video) 的方法论：
> 先 TTS 定时长 → 纯文生视频 → 字幕永远本地渲染。完整规划见 [PLAN.md](PLAN.md)。

## 快速开始

推荐直接使用交互式向导。无子命令或显式运行 `interactive` 都会进入向导：

```bash
# 环境变量（或在工作区放 .env 一行 AGNES_API_KEY=sk-...）
export AGNES_API_KEY=sk-...

# 交互式全流程：选择风格/语言/音色/输出目录，确认后依次执行四个阶段
agnes-video-free
# 等价写法
agnes-video-free interactive
```

向导会让你选择风格、语言、故事来源（文件或多行粘贴）、音色和语速；随后展示分句和首场 prompt 预览，确认后依次执行 `split → tts → video → assemble`。
默认产物为 `storyboard.json`、`audio/narration/`、`assets/videos/` 和 `out/<标题>.mp4`；输入 `q` 可取消配置。
已有 mp3/mp4 会自动跳过，已完成的文件会保留，便于后续手动重跑子命令。

需要脚本或 Agent 调用时，仍可使用以下子命令：

```bash
# ① 分句 + 预览 prompt（不请求任何生成 API）
agnes-video-free all examples/story_realistic.txt --dry-run

# ② 分句 → storyboard.json
agnes-video-free split examples/story_realistic.txt

# ③ 生成旁白 mp3（edge-tts，免费；已存在的自动跳过）
agnes-video-free tts

# ④ 生成视频片段（Agnes 异步任务 + 轮询；已有片段自动跳过）
agnes-video-free video

# ⑤ 组装成片（ffmpeg + libass）
agnes-video-free assemble --fonts-dir assets/fonts --output out/story.mp4

# 查看每场状态
agnes-video-free status

# 中断后继续：只补齐缺失阶段，不重复请求已完成素材
agnes-video-free resume --output out/story.mp4

# 输出：out/story.mp4（H.264 + AAC，字幕已烧录）
```

风格：`realistic-cinematic`（TikTok 9:16）/ `realistic-vlog`（小红书 3:4）/
`realistic-documentary`（微博 16:9）。配方见 [references/prompt-recipes.md](references/prompt-recipes.md)。
Agent 调用契约见 [SKILL.md](SKILL.md) 和 [references/agent-workflow.md](references/agent-workflow.md)。

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

CI（GitHub Actions，`.github/workflows/ci.yml`）会对每个 push / PR 运行
`cargo fmt --check`、`cargo clippy -D warnings`、`cargo test`，以及 `nix flake check` + `nix build .#`
（使用 DeterminateSystems/nix-installer-action，Nix 构建产物由 magic-nix-cache 缓存加速）。

Flake 包会自动注入 `PATH`（ffmpeg/ffprobe）、`SSL_CERT_FILE`（cacert）并安装随包字体
（`AGNES_VIDEO_FREE_FONTS`，供字幕渲染使用）。

## 项目结构

```
src/
├── main.rs        # clap 子命令、interactive 向导与 status/resume
├── models.rs      # Storyboard / Scene / StyleProfile / num_frames 计算
├── split.rs       # 中英文分句（一句一拍）
├── pipeline.rs    # 全流程编排（分句 → prompt → storyboard）
├── styles/        # 风格注册表（realistic 族 + 共享负向词基线）
└── tts/           # Rust 原生 edge-tts（kothok-edge-tts）
assets/fonts/      # 思源黑体 SC（OFL）
references/        # prompt 配方 / Agent 调用与流程细节
```

## 开发状态

- M0 ✅ 脚手架、clap 子命令、中英文分句、dry-run 预览
- M1 🚧 TTS ✅；ffprobe 封装 + Agnes 视频 API 客户端 ✅；真实视频生成需配置 `AGNES_API_KEY` 后运行 `video`
- M2 ✅ 成片组装（ffmpeg + libass：场景封装、concat、ASS 字幕、画幅校验）
- M3 🚧 交互式向导 ✅；`status` / `resume` ✅；Agent Skill ✅；`clean`、visual_plan 支持待实现；M4 GUI
