# Agent 工作流

本项目只处理独立视觉场景。输入是 `visual_plan.v2.json`，输出是视觉视频和可选的独立音频、BGM、SRT/LRC 字幕成片。

## 1. 预览与生成 storyboard

```bash
nix run .# -- split \
  --visual-plan examples/visual_plan.v2.example.json \
  --style realistic-cinematic \
  --lang zh \
  --title "我的视频" \
  --out storyboard.json
```

这一步只解析场景、计算帧数并写入 prompt，不请求视频 API。

## 2. 生成视觉视频

得到用户确认且已配置 `AGNES_API_KEY` 后：

```bash
nix run .# -- video --storyboard storyboard.json
```

每个场景按 `duration_sec` 生成，任务 ID 会在创建后立刻持久化。已有有效 MP4 会跳过。

## 3. 组装独立轨道

```bash
nix run .# -- assemble \
  --storyboard storyboard.json \
  --audio voiceover.mp3 \
  --bgm music.mp3 \
  --subtitles captions.srt \
  --output out/story.mp4
```

音频和字幕可以全部省略。`--audio` 是主音轨，`--bgm` 会循环并以 22% 音量混入；字幕支持 SRT/LRC，并保持文件中的时间轴。

## 4. 状态与恢复

```bash
nix run .# -- status --storyboard storyboard.json
nix run .# -- resume --storyboard storyboard.json \
  --audio voiceover.mp3 --bgm music.mp3 --subtitles captions.srt
```

恢复决策：

| 状态 | 操作 |
|---|---|
| 有场景 `待视频` | `resume`，只生成缺失视觉视频 |
| 有场景 `任务待轮询` | `resume`，复用已保存任务 ID |
| 所有视频有效、成片缺失 | `assemble` 或 `resume` |
| 所有视频和成片有效 | 无需操作 |

## 5. 清理单个场景

```bash
nix run .# -- clean --storyboard storyboard.json --scene v01 --dry-run
nix run .# -- clean --storyboard storyboard.json --scene v01 --stage all --yes
```

只允许安全的单个场景 ID。`video` 阶段同时删除依赖它的临时 clip；最终成片始终保留。

## 6. 约束

visual plan 的每项必须包含唯一 `id`、非空英文 `visual` 和 1.7 到 18.3 秒的 `duration_sec`。相邻镜头重复人物描述。程序会自动加入正确人体结构、稳定连续运动、无抖动、无变形、无水印和无文字约束。
