# Agent 工作流参考

本文档配合根目录 `SKILL.md` 使用，给出 Agent 可直接采用的非交互调用方式。

## 环境变量

```bash
export AGNES_API_KEY=sk-...
```

如果只做预览、分句、TTS 或组装已有素材，不需要 API Key；`video` 和需要补视频的 `resume` 才需要 Key。

## 推荐流程

```bash
# 0. 先看 prompt，不请求任何生成 API
nix run .# -- all story.txt \
  --title "我的故事" \
  --lang zh \
  --style realistic-cinematic \
  --dry-run

# 1. 创建 storyboard
nix run .# -- split story.txt \
  --title "我的故事" \
  --lang zh \
  --style realistic-cinematic \
  --out storyboard.json

# 2. 旁白
nix run .# -- tts \
  --storyboard storyboard.json \
  --gender female \
  --speed 1.0 \
  --out-dir audio/narration

# 3. 查看当前状态
nix run .# -- status \
  --storyboard storyboard.json \
  --audio-dir audio/narration \
  --video-dir assets/videos \
  --output out/story.mp4

# 4. 用户确认后生成视频
nix run .# -- video \
  --storyboard storyboard.json \
  --audio-dir audio/narration \
  --out-dir assets/videos

# 5. 组装成片
nix run .# -- assemble \
  --storyboard storyboard.json \
  --audio-dir audio/narration \
  --video-dir assets/videos \
  --fonts-dir assets/fonts \
  --output out/story.mp4
```

如果使用已经安装的二进制，把 `nix run .# --` 替换为 `agnes-video-free`。

## 恢复决策表

| `status` 结果 | 下一步 |
|---|---|
| 有场景 `待旁白` | `resume` 或 `tts` |
| 有场景 `待视频` | 检查 `AGNES_API_KEY` 后运行 `resume` 或 `video` |
| 有场景 `待组装` | 运行 `resume` 或 `assemble`，不需要 API Key |
| 所有场景 `ready`，最终成片缺失 | 运行 `assemble` |
| 最终成片有效 | 无需重复组装；除非用户明确要求重新渲染 |

完整恢复命令：

```bash
nix run .# -- resume \
  --storyboard storyboard.json \
  --audio-dir audio/narration \
  --video-dir assets/videos \
  --fonts-dir assets/fonts \
  --output out/story.mp4
```

`resume` 会根据文件是否存在以及文件大小判定阶段，不依赖独立数据库。视频文件必须至少 20 KB；旁白和临时 clip 必须是非空文件。

## 失败处理

### TTS 失败

保留已成功的 mp3，直接再次运行：

```bash
nix run .# -- tts --storyboard storyboard.json
```

### Agnes 单段失败

不要删除其他场景。先查看状态，再运行：

```bash
nix run .# -- status
nix run .# -- resume
```

客户端会对 429 和 5xx 自动重试。免费 Key 默认串行处理，不能通过增大并发来规避限流。

### 组装失败

确认系统存在 ffmpeg/ffprobe，并确认字体目录有效：

```bash
nix develop --command ffmpeg -version
nix run .# -- assemble --fonts-dir assets/fonts
```

组装失败不会删除已下载的场景视频。

## 阶段完成报告示例

```text
阶段完成：视频生成
新增 3，跳过 1，失败 0
产物：assets/videos/s01.mp4 ... assets/videos/s04.mp4
下一步：运行 assemble 生成带旁白和字幕的最终 MP4。
```
