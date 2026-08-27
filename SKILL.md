---
name: agnes-video-free
description: 使用 Agnes Video V2.0 和 ffmpeg，按独立视觉场景生成短视频，并叠加独立音轨、背景音乐与 SRT/LRC 字幕。
---

# agnes-video-free Agent Skill

本工具只接受 `visual_plan.v2.json` 的 `scenes` 数组。视觉场景、音频和字幕是三个独立输入：不要按字幕切视频，不要把字幕或音频文本写进 Agnes prompt。

## 安全规则

- 在用户明确确认前，只运行 `split`、`styles`、`status` 或 `--dry-run` 类检查，不请求 Agnes 视频 API。
- 需要生成视频时确认 `AGNES_API_KEY` 已配置，不要在聊天、日志或提交中输出 key。
- 默认 API 地址是 `https://apihub.agnes-ai.com`。
- 视频任务创建后会立刻写入 storyboard 的 `agnes_task_id`。中断后先运行 `status`，再运行 `resume`。
- 有效视频不要重复请求；只有明确要求时才使用 `clean --yes` 删除单个场景素材。
- 任何外部音频、BGM 和字幕只在 `assemble` 阶段处理。

## 标准流程

```bash
BIN="nix run .# --"

$BIN split \
  --visual-plan examples/visual_plan.v2.example.json \
  --style realistic-cinematic \
  --lang zh \
  --title "我的视频" \
  --out storyboard.json

# 用户确认后执行，需要 AGNES_API_KEY
$BIN video --storyboard storyboard.json

$BIN assemble \
  --storyboard storyboard.json \
  --audio voiceover.mp3 \
  --bgm music.mp3 \
  --subtitles captions.srt \
  --output out/story.mp4
```

`--audio`、`--bgm`、`--subtitles` 都是可选参数。字幕支持 `.srt` 和 `.lrc`，时间轴由文件决定。

## visual_plan.v2

```json
{
  "scenes": [
    {
      "id": "v01",
      "visual": "a quiet courtyard at dawn, one consistent woman opens a wooden window, soft morning light, slow push-in",
      "duration_sec": 8.0
    }
  ]
}
```

每个场景必须有唯一且安全的 `id`、非空英文 `visual` 和 1.7 到 18.3 秒的 `duration_sec`。相邻场景应重复人物外观和服装描述，以提高跨镜头一致性。程序会自动追加正确人体结构、自然头部转动、稳定连续运动和无水印约束，以及对应 negative prompt。

## 状态与恢复

```bash
$BIN status --storyboard storyboard.json
$BIN resume --storyboard storyboard.json \
  --audio voiceover.mp3 --bgm music.mp3 --subtitles captions.srt
```

`video` 按视觉场景时长计算 `8n+1` 帧数，成功下载的 MP4 自动跳过。`resume` 只补齐缺失视觉视频；素材齐全且成片有效时跳过组装。

清理前先预览：

```bash
$BIN clean --storyboard storyboard.json --scene v01 --dry-run
$BIN clean --storyboard storyboard.json --scene v01 --stage all --yes
```

## 交互向导

```bash
$BIN interactive
# 或直接运行
agnes-video-free
```

向导流程是：选择风格和语言 -> 读取 visual plan -> 选择项目目录 -> 选择主音轨、BGM、字幕 -> 预览场景和总时长 -> 确认 -> 生成视频 -> 组装成片。向导不生成 TTS，也没有旧的旁白驱动画面模式。

## 提示词要求

- 每场只描述一个清晰视觉镜头和一组连续动作。
- 相邻场景重复人物的外观、年龄、发型和服装描述。
- 使用明确的光线、环境和镜头运动词。
- 保持一个头、两只手、正确手指和自然关节运动。
- 明确要求稳定画面，禁止抖动、闪烁、变形、360 度转头、水印、logo 和文字。
- 字幕永远由本地 ffmpeg/libass 渲染，不交给视频模型绘制。
