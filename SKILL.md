---
name: agnes-video-free
description: 使用 Agnes Video V2.0、Rust edge-tts 和 ffmpeg 将中文或英文故事制作成带旁白与字幕的 TikTok、小红书或微博短视频。适用于短视频、文生视频、故事视频、竖屏视频、Agnes 视频等任务。
---

# agnes-video-free Agent Skill

这个 Skill 让 Agent 通过本仓库的 Rust CLI 生成故事短视频。优先使用子命令模式，不要直接编辑中间产物；所有阶段都围绕同一个 `storyboard.json` 工作。

## 安全与执行原则

- 在用户明确确认之前，只执行 `all --dry-run`、`split`、`status` 等不请求 Agnes 视频 API 的步骤。
- 需要视频生成时，确认当前环境已有 `AGNES_API_KEY`；不要在聊天、日志或提交中输出 API Key。
- 默认 API 地址是 `https://apihub.agnes-ai.com`，不要切换到长期维护中的中国站。
- TTS 使用 Rust 原生 edge-tts，不需要 Agnes API Key；视频生成和下载才需要 Key。
- 生成任务可能耗时很长。默认串行处理，不能通过重复提交来“催进度”。
- 中断后先运行 `status`，再运行 `resume`；不要删除有效的 mp3/mp4。
- 只有用户明确要求时，才删除单个损坏素材并重新生成。

## 前置条件

运行二进制、Cargo 或 Nix Flake 均可。推荐在仓库根目录使用：

```bash
BIN="nix run .# --"
# 也可以使用已安装的 agnes-video-free 或 `cargo run --`
```

视频组装需要 `ffmpeg`/`ffprobe` 和随包思源黑体。Nix 运行环境会自动提供它们；非 Nix 环境默认使用 `assets/fonts`。

视频阶段需要：

```bash
export AGNES_API_KEY=sk-...
# 或在当前工作目录的 .env 中写入 AGNES_API_KEY=sk-...
```

不要把 `.env` 加入 Git。

## 标准工作流

### 1. 预览，不产生网络请求

先确认故事、语言和风格：

```bash
$BIN all story.txt \
  --title "我的故事" \
  --lang zh \
  --style realistic-cinematic \
  --dry-run
```

可用风格：

- `realistic-cinematic`：TikTok，720×1280，电影写实
- `realistic-vlog`：小红书，1080×1440，生活 vlog
- `realistic-documentary`：微博，1280×720，纪录片

检查分句是否合理。当前没有 `visual_plan.json` 接入时，场景 prompt 会把分句原文作为 `SCENE_BODY`；如需更稳定的写实画面，应先把故事句子改写成具体的“主体 + 环境 + 动作 + 光线 + 镜头”描述。

### 2. 创建或更新 storyboard

```bash
$BIN split story.txt \
  --title "我的故事" \
  --lang zh \
  --style realistic-cinematic \
  --out storyboard.json
```

`split` 会覆盖指定的 storyboard 输出文件。若已有项目正在生成，不要对同一个 storyboard 重新 `split`，否则会丢失原有场景的素材字段。

### 3. 生成旁白

```bash
$BIN tts \
  --storyboard storyboard.json \
  --gender female \
  --speed 1.0 \
  --out-dir audio/narration
```

有效的非空 mp3 会自动跳过；缺失或空文件会重新生成。中文默认女声为 `zh-CN-XiaoyiNeural`，英文默认女声为 `en-US-JennyNeural`。如需指定音色，使用 `--voice`。

### 4. 生成 Agnes 视频片段

在用户确认后执行：

```bash
$BIN video \
  --storyboard storyboard.json \
  --audio-dir audio/narration \
  --out-dir assets/videos
```

每个场景的旁白时长由 `ffprobe` 测量，并据此计算满足 `8n+1` 的 `num_frames`。有效的、超过 20 KB 的 mp4 会自动跳过。默认轮询间隔为 8 秒，单段最长等待 900 秒。

可在测试或特殊服务端场景覆盖：

```bash
$BIN video \
  --storyboard storyboard.json \
  --api-base-url https://apihub.agnes-ai.com \
  --poll-interval 8 \
  --poll-timeout 900
```

### 5. 查看状态

```bash
$BIN status \
  --storyboard storyboard.json \
  --audio-dir audio/narration \
  --video-dir assets/videos \
  --output out/story.mp4
```

状态含义：

- `待旁白`：缺少或为空的 `<id>.mp3`
- `待视频`：旁白存在，但 `<id>.mp4` 缺失或小于 20 KB
- `待组装`：旁白和视频均存在，但临时 clip 尚未完成
- `ready`：该场景的 clip 已存在
- 最终成片另行显示；只有有效 mp4 才算完成

### 6. 中断后恢复

```bash
$BIN resume \
  --storyboard storyboard.json \
  --audio-dir audio/narration \
  --video-dir assets/videos \
  --fonts-dir assets/fonts \
  --output out/story.mp4
```

`resume` 的行为：

1. 缺少旁白时执行 TTS，已有有效旁白跳过；
2. 缺少视频时执行 Agnes 生成，已有有效视频跳过；如果视频已经齐全，不会请求 Agnes API；
3. 最终成片已存在且有效时跳过组装，否则执行 `assemble`；
4. 任一阶段失败都会保留之前已经完成的产物，下一次可再次运行 `resume`。

如果只需要重新组装，不需要 API Key：

```bash
$BIN assemble \
  --storyboard storyboard.json \
  --audio-dir audio/narration \
  --video-dir assets/videos \
  --fonts-dir assets/fonts \
  --output out/story.mp4
```

## 交互模式

面向人类用户时可让用户自己运行：

```bash
$BIN interactive
# 或直接运行：agnes-video-free
```

交互向导支持选择风格、语言、故事文件/多行粘贴、音色、语速和输出目录；确认后才执行完整的 `split → tts → video → assemble` 流程。Agent 不应模拟交互输入，除非用户明确要求自动化测试。

## Agent 输出规范

每完成一个阶段，向用户报告：

1. 实际执行的命令和产物路径；
2. 成功、跳过、失败的场景数量；
3. 下一步建议。

推荐报告格式：

```text
阶段完成：TTS
新增 4，跳过 2，失败 0
产物：audio/narration/*.mp3
下一步：运行 status 确认素材状态，然后执行 video（需要 AGNES_API_KEY）。
```

失败时不要把无关阶段标记为成功；说明可以安全重试的命令，并指出已经保留的产物。

## 不要做的事情

- 不要在视频任务完成前重复提交同一个场景。
- 不要把字幕交给视频模型绘制；字幕由 ffmpeg + libass 在本地烧录。
- 不要因为纯文生视频跨场景人物脸部变化就批量重跑。
- 不要将 Agnes API Key 传给 CDN 视频下载地址。
- 不要覆盖正在使用的 `storyboard.json`，除非用户明确要求重新分句。
