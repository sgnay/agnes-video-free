# PLAN.md

## 目标

将 `visual_plan.v2.json` 中的视觉场景生成 Agnes 视频，再按需叠加独立主音轨、背景音乐和字幕，输出可发布的短视频。

## 固定契约

```text
visual_plan scenes -> visual storyboard -> Agnes video scenes -> concat
                                                        + audio
                                                        + BGM
                                                        + SRT/LRC
                                                        -> final MP4
```

视觉场景边界只由 visual plan 决定，不由字幕、音频或文本句子决定。视频 prompt 不读取音频、字幕或故事文本。

每个 visual scene 包含：

```json
{
  "id": "v01",
  "visual": "a clear English description of one visual shot",
  "duration_sec": 8.0
}
```

`duration_sec` 范围为 1.7 到 18.3 秒，帧数满足 `8n+1`，范围为 41 到 441。相邻场景应重复人物外观描述。程序自动加入：

- 单一人物、一个头、两只手、五指和正确人体结构；
- 自然且小幅的头部转动，保持脸部和身体比例；
- 稳定连续的相机和主体运动；
- 禁止额外肢体、缺指、关节异常、360 度转头、抖动、闪烁、变形、水印、logo 和文字。

## CLI

- `interactive`：视觉计划向导，默认入口。
- `styles`：列出 realistic 风格。
- `split`：解析 visual plan 并写入 storyboard。
- `video`：按场景时长创建、轮询、下载 Agnes 视频。
- `assemble`：拼接视觉视频，混合 `--audio`/`--bgm`，并烧录 `--subtitles`。
- `status`：查看视觉场景、任务、视频和 clip 状态。
- `resume`：复用任务 ID，补齐视频并按需组装。
- `clean`：安全删除单个场景的视频或临时 clip。

旧版 TTS、逐句视频、故事分句、旧 map visual plan 和兼容向导不属于当前产品契约。

## 实现状态

- [x] visual plan 数组解析、ID/时长/描述校验。
- [x] Storyboard 纯视觉数据模型。
- [x] Agnes 创建、轮询、下载、429/5xx 重试和任务持久化。
- [x] realistic cinematic/vlog/documentary 风格。
- [x] 人体结构、运动稳定、无水印和无变形 prompt 约束。
- [x] 视觉视频拼接、独立音频、BGM 循环混音。
- [x] SRT/LRC 解析、ASS 字幕、中文 kinsoku 换行和 jieba 词边界保护。
- [x] 交互向导、status、resume、clean。
- [x] Nix flake、字体和 CI 检查。

## 验证命令

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## 后续

- 真实 API 回归素材由用户提供 key 后按需运行。
- GUI 和跨平台发布暂不实现。
