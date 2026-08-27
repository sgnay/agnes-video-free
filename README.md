# agnes-video-free

用 Rust 编写的视觉短视频生成工具：读取 `visual_plan.v2.json`，按视觉场景调用 Agnes Video V2.0，再在本地叠加独立音轨、背景音乐和字幕，发布到 TikTok、小红书或微博。

视频提示词、音频和字幕完全分离。一个 `scenes` 项对应一个视觉镜头，不按字幕或句子切镜头，因此字幕可以跨越多个视觉场景。

## 快速开始

准备 Agnes API key：

```bash
export AGNES_API_KEY=sk-...
```

生成视觉 storyboard：

```bash
agnes-video-free split \
  --visual-plan examples/visual_plan.v2.example.json \
  --style realistic-cinematic \
  --lang zh \
  --title "早上上班" \
  --out storyboard.json
```

生成视觉片段：

```bash
agnes-video-free video --storyboard storyboard.json
```

组装成片。音轨、BGM 和字幕均可省略：

```bash
agnes-video-free assemble \
  --storyboard storyboard.json \
  --audio voiceover-or-main-track.mp3 \
  --bgm morning-music.mp3 \
  --subtitles captions.srt \
  --fonts-dir assets/fonts \
  --output out/morning.mp4
```

字幕支持 `.srt` 和 `.lrc`。字幕时间轴由字幕文件本身决定，不会根据视觉场景边界自动改写。

## 图生视频（ti2vid）

每个场景可带可选 `image` 字段（本地路径或 http(s) URL）。带 `image` 的场景会自动以 ti2vid 图生视频模式生成——模型以该图为**首帧条件**，生成与参考图主体、风格一致的连续画面。适合固定人物或物品外观。

```bash
agnes-video-free split \
  --visual-plan examples/visual_plan.v2.example.json \
  --image refs/woman.png \
  --style realistic-cinematic \
  --lang zh \
  --title "早上上班" \
  --out storyboard.json

agnes-video-free video --storyboard storyboard.json
```

`split --image` 会把参考图填充到所有既没有 `image` 也没有 `keyframes` 字段的场景；场景自带的 `image`/`keyframes` 优先。`video` 命令为带参考图的场景发送 `mode: "ti2vid"` 请求：本地文件在请求时 base64 编码，http(s) URL 原样传递。

`split --keyframes "a.png,b.png"` 同理，填充没有 `image`/`keyframes` 的场景为关键帧动画模式（逗号分隔，至少 2 张）：

## 关键帧动画（keyframes）

场景可带 `keyframes` 数组（至少 2 张，本地路径或 http(s) URL），让模型在两个或多个关键帧之间生成平滑过渡动画：

```json
{
  "scenes": [
    {
      "id": "v03",
      "visual": "a smooth cinematic transition between the two keyframes, maintaining character identity and natural motion",
      "duration_sec": 8.0,
      "keyframes": ["refs/frame-a.png", "refs/frame-b.png"]
    }
  ]
}
```

`video` 命令会把 `keyframes` 放进请求的 `extra_body.image` 并设 `mode: "keyframes"`：本地文件以完整 data URI（带 mime 头）传递，http(s) URL 原样传递。一个场景的 `image`（ti2vid）与 `keyframes` 二选一，不能同时设置；`split --image` 也不会覆盖带 `keyframes` 的场景。

## visual_plan.v2 格式

```json
{
  "scenes": [
    {
      "id": "v01",
      "visual": "a quiet bedroom at dawn, one consistent woman opens the window, soft morning light, slow push-in",
      "duration_sec": 8.0,
      "image": "refs/woman.png"
    },
    {
      "id": "v02",
      "visual": "the same woman rides a bicycle through a waking city street, morning light, smooth tracking shot",
      "duration_sec": 8.0
    }
  ]
}
```

每个场景的 `duration_sec` 必须在 1.7 到 18.3 秒之间。程序会根据该时长计算 Agnes 所需的 `8n+1` 帧数。建议在相邻场景中重复人物的完整外观描述，帮助模型保持人物一致。`image` 和 `keyframes` 均为可选字段，省略时该场景走文生视频；`image`（单图 ti2vid）与 `keyframes`（至少 2 张关键帧动画）二选一。

程序会自动向每个场景加入写实视频约束：单一人物、正确人体结构、两只手和五指、自然头部转动、稳定连续运动、无抖动和变形、无水印和文字。对应的 negative prompt 也会排除多手、多臂、缺指、360 度转头、镜头抖动、闪烁、变形、水印和 logo。

## 交互向导

直接运行即可进入交互向导：

```bash
agnes-video-free
# 或
agnes-video-free interactive
```

向导依次选择风格、语言、visual plan、项目目录、主音轨、BGM、字幕和全局视觉生成模式（纯文生 / ti2vid / keyframes），然后可逐场景编辑参考图或关键帧，确认后执行：

```text
visual_plan -> storyboard -> Agnes 视觉视频 -> 独立轨道与字幕 -> 成片
```

向导不会生成 TTS，不会把字幕或音频文本放进视频提示词。

### Dry Run 预览

```bash
agnes-video-free interactive --dry-run
```

只走完配置流程和逐场景编辑，预览最终 storyboard 内容和生成模式统计，不写入文件、不调用 API。适合在正式生成前确认配置是否正确。

## 其他命令

```bash
# 查看风格
agnes-video-free styles

# 查看场景、任务和视频状态（单次）
agnes-video-free status --storyboard storyboard.json

# watch 模式：自动轮询刷新，直到所有场景完成（Ctrl+C 退出）
agnes-video-free status --watch --interval 5

# 中断后继续生成缺失视频，并重新组装（需要时传入轨道参数）
agnes-video-free resume --storyboard storyboard.json \
  --audio voiceover.mp3 --bgm music.mp3 --subtitles captions.srt

# 先预览，再确认删除单个场景
agnes-video-free clean --storyboard storyboard.json --scene v01 --dry-run
agnes-video-free clean --storyboard storyboard.json --scene v01 --stage all --yes
```

`resume` 会复用 storyboard 中已经保存的 `agnes_task_id`，有效视频不会重复请求。`clean` 只删除指定场景的视频或临时 clip，不删除最终成片。

## NixOS

```bash
nix develop
nix build
nix run .# -- styles
nix develop --command cargo test
```

Nix 包会提供 `ffmpeg`、`ffprobe`、CA 证书和随包字体。非 Nix 环境默认从 `assets/fonts` 读取字幕字体。

## 项目结构

```text
src/
├── main.rs        # CLI、视觉向导、状态和恢复
├── models.rs      # Storyboard、视觉 Scene、风格和画幅
├── pipeline.rs    # visual_plan -> storyboard
├── agnes.rs       # Agnes 创建、轮询、下载和重试
├── media/         # ffprobe、视觉拼接、混音、ASS 字幕
└── styles/        # realistic 风格族和视频安全约束
assets/fonts/      # 思源黑体（OFL）
examples/          # visual_plan.v2 示例
references/        # prompt 配方和 Agent 工作流
```

## 开发检查

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```
