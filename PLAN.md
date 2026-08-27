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
  "duration_sec": 8.0,
  "image": "refs/woman.png",
  "keyframes": ["refs/a.png", "refs/b.png"]
}
```

`image` 可选（本地路径或 http(s) URL），存在时该场景以 ti2vid 图生视频模式生成：请求体携带 `image` 与 `mode: "ti2vid"`，模型以该图为首帧条件。`keyframes` 可选（至少 2 张），存在时以 keyframes 模式生成：请求体 `extra_body.image` 数组 + `extra_body.mode: "keyframes"`，本地文件以完整 data URI 传递。`image` 与 `keyframes` 二选一。`split --image` 可给所有既没有 `image` 也没有 `keyframes` 的场景填充同一张参考图；场景自带字段优先。

`duration_sec` 范围为 1.7 到 18.3 秒，帧数满足 `8n+1`，范围为 41 到 441。相邻场景应重复人物外观描述。程序自动加入：

- 单一人物、一个头、两只手、五指和正确人体结构；
- 自然且小幅的头部转动，保持脸部和身体比例；
- 稳定连续的相机和主体运动；
- 禁止额外肢体、缺指、关节异常、360 度转头、抖动、闪烁、变形、水印、logo 和文字。

## CLI

- `interactive`：视觉计划向导，默认入口。
- `styles`：列出 realistic 风格。
- `split`：解析 visual plan 并写入 storyboard，`--image` 填充全局参考图（ti2vid），`--keyframes` 填充全局关键帧（逗号分隔，至少 2 张）。
- `video`：按场景时长创建、轮询、下载 Agnes 视频。
- `assemble`：拼接视觉视频，混合 `--audio`/`--bgm`，并烧录 `--subtitles`。
- `status`：查看视觉场景、任务、视频和 clip 状态。`--watch` 自动轮询刷新（`--interval` 指定秒数），全部 clip 完成后自动退出。
- `resume`：复用任务 ID，补齐视频并按需组装。
- `clean`：安全删除单个场景的视频或临时 clip。

旧版 TTS、逐句视频、故事分句、旧 map visual plan 和兼容向导不属于当前产品契约。

## 实现状态

- [x] visual plan 数组解析、ID/时长/描述校验。
- [x] Storyboard 纯视觉数据模型。
- [x] Agnes 创建、轮询、下载、429/5xx 重试和任务持久化。
- [x] ti2vid 图生视频：visual_plan 每场景可选 image、split --image 全局参考图、本地文件 base64/URL 传参。
- [x] keyframes 关键帧动画：每场景可选 keyframes 数组（≥2 张）、extra_body 请求、data URI/URL 传参。
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

## 真实 API 回归（可选，需要 AGNES_API_KEY）

在 `.env` 或环境变量中配置 `AGNES_API_KEY` 后，可用下面一组命令验证三种生成模式（文生 / ti2vid / keyframes）。每次运行会消耗真实配额，建议用 2 秒短场景。

```bash
# 1. 准备测试目录和三张纯色素材
rm -rf /tmp/agnes-regression && mkdir -p /tmp/agnes-regression/refs /tmp/agnes-regression/out
ffmpeg -y -loglevel error -f lavfi -i "color=c=purple:s=512x512" -frames:v 1 /tmp/agnes-regression/refs/kf-purple.png
ffmpeg -y -loglevel error -f lavfi -i "color=c=red:s=512x512" -frames:v 1 /tmp/agnes-regression/refs/kf-red.png
ffmpeg -y -loglevel error -f lavfi -i "color=c=blue:s=512x512" -frames:v 1 /tmp/agnes-regression/refs/kf-blue.png
```

```json
// /tmp/agnes-regression/plan.json
{
  "scenes": [
    {
      "id": "v01",
      "visual": "a soft purple gradient background with gentle slow breathing light, subtle film grain, stable composition",
      "duration_sec": 2.0,
      "image": "refs/kf-purple.png"
    },
    {
      "id": "v02",
      "visual": "a smooth cinematic color transition from the red frame to the blue frame, maintaining a stable solid color gradient, natural motion",
      "duration_sec": 2.0,
      "keyframes": ["refs/kf-red.png", "refs/kf-blue.png"]
    },
    {
      "id": "v03",
      "visual": "a sunny park with green trees and a small pond, light breeze, slow lateral pan, natural daylight",
      "duration_sec": 2.0
    }
  ]
}
```

```bash
# 2. 生成并下载三个场景的视频
cd /tmp/agnes-regression
agnes-video-free split --visual-plan plan.json --style realistic-cinematic --lang zh --title "回归" --out storyboard.json
agnes-video-free video --storyboard storyboard.json --out-dir out --poll-timeout 600
```

```bash
# 3. 抽查内容（平均色按 1x1 缩放帧取整帧均值）
for f in v01 v02 v03; do
  ffprobe -v error -show_entries format=duration,size -show_entries stream=codec_name,width,height,r_frame_rate -of default=noprint_wrappers=1 "out/$f.mp4"
done
# v01 首帧平均色应接近紫色参考图 ≈ (126, 0, 122)
ffmpeg -v error -i out/v01.mp4 -frames:v 1 -vf scale=1:1 -f rawvideo -pix_fmt rgb24 - | od -An -tu1 | head -1
# v02 首帧平均色应接近红 ≈ (253, 1, 2)，尾帧接近蓝 ≈ (5, 10, 226)
ffmpeg -v error -i out/v02.mp4 -frames:v 1 -vf scale=1:1 -f rawvideo -pix_fmt rgb24 - | od -An -tu1 | head -1
ffmpeg -v error -sseof -0.2 -i out/v02.mp4 -frames:v 1 -vf scale=1:1 -f rawvideo -pix_fmt rgb24 - | od -An -tu1 | head -1
# v03 首/中/尾帧平均色应彼此有差异（存在实际内容与运动）
ffmpeg -v error -i out/v03.mp4 -frames:v 1 -vf scale=1:1 -f rawvideo -pix_fmt rgb24 - | od -An -tu1 | head -1
```

2026-08-27 已按此流程通过真实 API 验证：三种模式均成功生成，ti2vid 首帧贴合参考图，keyframes 红→蓝过渡真实发生，文生视频帧间有差异。服务端会把请求分辨率归一化到 480p/720p/1080p 档，以响应中的 `size`/`seconds` 为准。

## 后续

- GUI 和跨平台发布暂不实现。
- 真实 API 回归素材与步骤已固化在“真实 API 回归”章节，换 key 后可直接重跑。
