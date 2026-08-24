# PLAN.md — agnes-video-free

> 一个用 Rust 编写的短视频自动生成工具：借鉴 [story-handdrawn-video](https://github.com/liangdabiao/story-handdrawn-video) 的方法论，
> 把一段中文/英文故事文本变成带旁白、字幕、背景音乐的竖屏短视频。
> 视频后端为 **Agnes Video V2.0**（国际站 `https://apihub.agnes-ai.com`，纯文生视频，当前 $0/秒），
> 旁白为 **edge-tts**（免费，无需 API key），成片组装用 **ffmpeg + libass**（无 Node.js / Chrome 依赖）。
> 目标平台：TikTok / 小红书 / 微博。

---

## 1. 概述

### 1.1 背景

原项目 `story-handdrawn-video` 已验证一条**全免费**的短视频制作路径：

```
story.txt → 分句 → edge-tts 旁白 → ffprobe 量时长 → 算 num_frames
→ Agnes 纯文生视频（异步任务+轮询）→ 下载片段 → Remotion 组装（字幕+音轨）→ 成片
```

但它存在以下局限性（本次要解决的）：

| # | 原项目局限 | 本项目方案 |
|---|---|---|
| 1 | 风格有限（仅 `crayon` 蜡笔风 / `textbook` 教材风） | 新增 **realistic 真实感风格族**（电影写实 / 生活 vlog / 纪录片），面向 TikTok / 小红书 / 微博发布 |
| 2 | 默认打中国站 `api.agnes-ai.cn`（长期维护中） | 全面切换国际站 `https://apihub.agnes-ai.com`（`/v1/videos` 创建、`/agnesapi?video_id=` 查询） |
| 3 | 使用流程不清晰：需 agent 调 skill 生成 story.txt，再手动跑一串 Python 脚本 | 提供**交互式向导**一站式完成全流程；同时保留**子命令化**的 agent skill 模式与独立 CLI |
| 4 | Python + Node.js 双技术栈，依赖重 | 纯 Rust 单二进制，仅依赖系统 ffmpeg/ffprobe |

### 1.2 目标（非目标）

**做**：
- Rust 单二进制 CLI，含交互模式、子命令模式、agent skill 模式三种用法
- 风格注册表：继承 `crayon` / `textbook`，新增 realistic 风格族，风格可扩展
- 平台预设：TikTok（9:16）、小红书（3:4）、微博（16:9 / 9:16），一键适配画幅与字幕样式
- 断点续跑：任何一步中断后 `resume` 继续，已生成的素材不重跑（沿用原项目「视频不许重跑」硬规则）

**暂不做**：
- GUI（列入 M4 计划，见 §12）
- 图生视频 / 关键帧动画（Agnes 支持，但本期聚焦纯文生视频；API 客户端预留扩展字段）

---

## 2. 从原项目继承的方法论（不可变）

以下规则经原项目实战验证，直接继承，作为本项目的行为约束：

1. **先 TTS，后视频**：旁白时长决定视频帧数，不裁不冻不补。
2. **纯文生视频**：不用参考图、不传 character_reference，靠 prompt 锁风格。
3. **字幕永远确定性渲染**：由本地工具（ffmpeg + libass）渲染，**禁止让视频模型画文字**，prompt 用 negative 排除。
4. **prompt 三段式**：`STYLE_HEADER`（固定风格头）+ `SCENE_BODY`（该场动作主体）+ `MOTION_FOOTER`（固定运动尾）+ `NEGATIVE`。
5. **`num_frames` 必须 `8n+1`**，上限 441（≈18.3s @ 24fps），下限 41（≈1.7s）。
6. **视频不许重跑**：素材存在即 skip，只有单段确实无法观看才删单段重跑。
7. **70% 质量即交付**：单段动作合理、主体对版、风格一致就过，不逐段调 prompt。
8. **句子要短**：中文单句 ≤ 36 字、英文 ≤ 120 字符，超长按逗号/连接词再切；旁白 >18s 必须拆句而不是延长帧数。

---

## 3. 总体架构

### 3.1 形态

- **单 crate 多模块**起步（一个二进制 `agnes-video-free`），GUI 阶段再拆 workspace（`crates/`）。
- 三种入口，同一套核心逻辑（`core` 模块）：
  1. **交互模式**（默认，无子命令）：对话式向导，适合人类用户。
  2. **子命令模式**：`split / tts / video / storyboard / assemble / render / all / resume / status`，适合脚本与 agent。
  3. **Agent skill 模式**：仓库内附 `SKILL.md` + `references/`，agent（Claude Code / Codex / workbuddy 等）按文档调用子命令逐步执行。

### 3.2 技术栈

| 用途 | 选型 | 说明 |
|---|---|---|
| 语言 | Rust stable（edition 2024） | 单二进制 |
| CLI | `clap`（derive） | 子命令 + 参数 |
| 异步 | `tokio` | HTTP / 轮询 / 并发 |
| HTTP | `reqwest`（rustls） | Agnes API + 下载 |
| 序列化 | `serde` / `serde_json` | storyboard.json 单一数据源（不引入 YAML 依赖） |
| 交互 | `inquire` | 选择器 / 确认 / 多行输入 |
| 错误 | `thiserror` + `anyhow` | 领域错误 + 顶层包装 |
| 日志 | `tracing` + `tracing-subscriber` | 进度与断点日志 |
| env | `dotenvy` | 读取 `.env` 中的 `AGNES_API_KEY` |
| TTS | `kothok-edge-tts` 或 `edge-tts-rust`（**spike 后定**，见 §8） | Rust 原生 edge-tts |
| 测试 | `wiremock` | mock Agnes API 做集成测试 |
| 外部工具 | 系统 `ffmpeg` / `ffprobe` | 时长探测、字幕烧录、合成 |

### 3.3 目录结构

```
agnes-video-free/
├── Cargo.toml
├── PLAN.md
├── README.md
├── SKILL.md                      # agent skill 入口（frontmatter + 工作流）
├── references/
│   ├── pipeline.md               # 流程细节（对齐代码实现）
│   └── prompt-recipes.md         # prompt 配方 + visual_plan 写法
├── src/
│   ├── main.rs                   # 入口：子命令分发 / 交互模式
│   ├── cli/
│   │   ├── mod.rs                # clap 定义
│   │   └── wizard.rs             # 交互式向导
│   ├── config.rs                 # AGNES_API_KEY 查找、工作区路径、默认值
│   ├── models.rs                 # Story / Scene / Storyboard / Style / Platform
│   ├── split.rs                  # 分句（中/英）
│   ├── styles/
│   │   ├── mod.rs                # 风格注册表（按 id 查找）
│   │   ├── crayon.rs             # 继承：Q 版手绘蜡笔风
│   │   ├── textbook.rs           # 继承：牛津教材风（M3 评估简化方案）
│   │   └── realistic/
│   │       ├── mod.rs            # realistic 风格族公共负向词/规则
│   │       ├── cinematic.rs      # 电影写实
│   │       ├── vlog.rs           # 生活 vlog
│   │       └── documentary.rs    # 纪录片
│   ├── tts/
│   │   ├── mod.rs                # TtsProvider trait（可替换实现）
│   │   └── edge.rs               # Rust 原生 edge-tts 客户端
│   ├── agnes/
│   │   ├── mod.rs                # AgnesClient：创建任务/轮询/下载
│   │   └── types.rs              # 请求/响应模型
│   ├── media/
│   │   ├── ffprobe.rs            # 音视频时长、尺寸探测
│   │   └── ffmpeg.rs             # 片段合成、concat、字幕烧录
│   ├── render/
│   │   ├── ass.rs                # ASS 字幕生成（样式/时间轴/字体）
│   │   └── assemble.rs           # 组装编排（clip → concat → burn）
│   └── pipeline.rs               # 全流程编排（供交互/子命令/skill 共用）
├── assets/
│   └── fonts/                    # MaShanZheng-Regular.ttf（OFL）+ 黑体（OFL）
├── examples/
│   ├── story.txt                 # 中文示例
│   ├── story_realistic.txt       # realistic 风格示例
│   └── visual_plan.example.json
└── tests/
    ├── split.rs
    ├── frames.rs                 # num_frames 计算
    ├── ass.rs
    ├── agnes_mock.rs             # wiremock 集成测试
    └── e2e_smoke.rs              # 全流程冒烟（真实 API，可选运行）
```

### 3.4 数据模型（storyboard.json 单一数据源）

```jsonc
{
  "title": "我的小猫",
  "lang": "zh",
  "style": "realistic-vlog",
  "platform": "tiktok",
  "width": 720, "height": 1280,
  "fps": 30, "frame_rate_video": 24,
  "scenes": [
    {
      "id": "s01",
      "caption": "下雨天，我在巷口捡到一只橘猫。",   // 分句原文
      "narration": "下雨天，我在巷口捡到一只橘猫。",  // 送入 TTS 的文本
      "visual": "a wet alley at dusk, a young woman crouching...", // visual_plan（可选）
      "prompt": "…",              // 最终三段式 prompt（dry-run 可预览）
      "negative_prompt": "…",
      "narration_audio": "audio/narration/s01.mp3",
      "motion_video": "assets/videos/s01.mp4",
      "duration_sec": 4.82,       // ffprobe 实测
      "num_frames": 121
    }
  ]
}
```

所有中间产物（audio / videos / clips / subtitles.ass）都由 storyboard.json 驱动，`resume` 时按 `id` 检查存在性。

---

## 4. 核心流程（Pipeline）

```
输入（交互粘贴 / story.txt / 现成 mp3+LRC）
        │
        ▼
① split 分句（中按 。！？；，英按 . ! ? ;，超长按 ，、/连接词再切）
        │ 每句一拍 = 一段视频
        ▼
② TTS（Rust edge-tts，串行）→ audio/narration/sXX.mp3（已存在则 skip）
        │
        ▼
③ ffprobe 每段 duration_sec → 算 num_frames
        │   target = round(dur × 24)
        │   num_frames = 8 × ceil((target−1)/8) + 1
        │   clamp(41, 441)
        ▼
④ 拼 prompt：STYLE_HEADER + SCENE_BODY(visual_plan 或原句) + MOTION_FOOTER + NEGATIVE
        │
        ▼
⑤ POST https://apihub.agnes-ai.com/v1/videos （串行，默认 concurrency=1）
        │  429 → 等 65s 重试；5xx → 指数退避 5/10/20/40s
        ▼
⑥ GET /agnesapi?video_id=<ID> 轮询（8s 间隔，单段最长 15min）
        │  完成 → metadata.url（兼容顶层 url）
        ▼
⑦ 下载 → assets/videos/sXX.mp4（已存在则 skip）
        │
        ▼
⑧ 组装（ffmpeg + libass）：
        │  每场 clip_XX.mp4 = sXX.mp4 + sXX.mp3（-shortest）
        │  concat demuxer → story_merged.mp4
        │  生成全局 subtitles.ass（时间轴按累计时长偏移）
        │  ffmpeg -vf "ass=…" 烧录字幕 → 最终成片
        ▼
⑨ out/<title>_<platform>.mp4（H.264 + AAC）
```

**dry-run**：`agnes-video-free all --dry-run` 只跑 ①②④ 的输出预览（分句 + prompt），不发任何生成请求。

---

## 5. 风格系统

### 5.1 风格注册表

每个风格是一个 `StyleProfile`：

```rust
struct StyleProfile {
    id: String,            // "crayon" | "textbook" | "realistic-cinematic" | ...
    name: String,          // 中文展示名
    description: String,
    default_platform: Platform,
    style_header: String,  // 三段式中的固定风格头
    motion_footer: String, // 固定运动尾
    negative: String,      // 负向词
    canvas: (u32, u32),    // 默认画幅，如 720×1280
    subtitle: SubtitleStyle, // 字体 / 字号 / 位置 / 描边 / 颜色
}
```

| 风格 id | 名称 | 适用 | 画幅 | 字幕 |
|---|---|---|---|---|
| `crayon`（继承） | Q 版手绘蜡笔风 | 童话 / 生活 / 日记 | 9:16 720×1280 | MaShanZheng 毛笔字 |
| `textbook`（继承，M3 简化评估） | 牛津教材风 | 英语教学 | 9:16 | MaShanZheng + 整句英文字幕 |
| `realistic-cinematic`（新） | 电影写实 | TikTok 剧情/氛围 | 9:16 720×1280 | 简洁黑体（思源黑体 OFL） |
| `realistic-vlog`（新） | 生活 vlog | 小红书 生活方式 | 3:4 1080×1440 | 黑体，居中偏下 |
| `realistic-documentary`（新） | 纪录片解说 | 微博 知识/口播 | 16:9 1280×720 | 黑体，底部安全区 |

### 5.2 realistic 风格族（本次新增，重点）

沿用三段式 prompt，但风格 DNA 完全不同。以 `realistic-cinematic` 为例：

```
[STYLE_HEADER]
cinematic realism, photorealistic live-action look, vertical 9:16,
natural volumetric lighting, shallow depth of field, 35mm film grain,
muted cinematic color grade, authentic textures, shot on modern camera,
no animation style, no illustration, no cartoon, no 3D render

[SCENE_BODY]
<visual_plan 或自动从原句生成的英文场景描述（谁+在哪+做什么）>

[MOTION_FOOTER]
natural realistic motion, subtle handheld camera movement, gentle subject
motion, realistic physics, no morphing, no text, settle naturally

[NEGATIVE]
text, letters, subtitles, captions, Chinese characters, English words,
watermark, logo, signature, cartoon, illustration, anime, 3D render,
CGI artifacts, distorted faces, extra limbs, flickering
```

**realistic 风格硬规则**（对齐原项目 textbook 的经验教训）：
- 每场写「谁 + 在哪 + 做什么」三要素叙事场景，给一个能动的元素，避免 icon/diagram/抽象概念词。
- 明确的光线（golden hour / overcast / neon night）+ 镜头运动词（slow push-in / handheld / tracking）。
- 人物一致性不承诺（纯文生视频锁不住脸），靠风格头统一画面质感；跨场脸不同属正常。
- 小红书（vlog）倾向柔和自然光 + 生活化场景；纪录片倾向客观构图 + 环境声描述。

### 5.3 平台预设

| 平台 | 默认画幅 | Agnes 标准化档位 | 说明 |
|---|---|---|---|
| TikTok | 9:16 720×1280 | 720p / 9:16 | 默认预览即交付 |
| 小红书 | 3:4 1080×1440 | 1080p / 3:4 | Agnes 支持 3:4；封面党友好 |
| 微博 | 16:9 1280×720 | 720p / 16:9 | 横版口播/纪录片 |

平台预设 = 画幅 + 字幕样式 + 推荐风格，用户可自由组合覆盖。

---

## 6. Agnes API 客户端（`src/agnes/`）

### 6.1 端点（国际站，硬编码为默认，可用配置覆盖）

| 用途 | 方法 + URL |
|---|---|
| 创建任务 | `POST https://apihub.agnes-ai.com/v1/videos` |
| 查询结果（推荐） | `GET https://apihub.agnes-ai.com/agnesapi?video_id=<VIDEO_ID>` |
| 查询结果（旧版兼容） | `GET https://apihub.agnes-ai.com/v1/videos/<TASK_ID>` |

请求头：`Authorization: Bearer $AGNES_API_KEY`、`Content-Type: application/json`。

**Key 查找顺序**：环境变量 `AGNES_API_KEY` → 当前目录 `.env` → 父目录逐级向上 `.env`（用 `dotenvy` + 手动向上查找）。`.env` 不入库。

### 6.2 创建任务 payload

```jsonc
{
  "model": "agnes-video-v2.0",
  "prompt": "<STYLE_HEADER>\n<SCENE_BODY>\n<MOTION_FOOTER>",
  "negative_prompt": "<NEGATIVE>",
  "width": 720, "height": 1280,   // 按风格/平台预设
  "num_frames": 121,              // 8n+1，≤441
  "frame_rate": 24
  // 预留：image / mode / extra_body（图生视频、关键帧，本期不用）
}
```

### 6.3 轮询与容错

- 状态机：`queued → in_progress → completed | failed`。
- 间隔 8s，单段最长等 15 分钟（`POLL_MAX_WAIT_SEC = 900`）。
- **429**：等 65s 重试（免费 key 限流 1 req/min）；**500/502/503/504**：指数退避 5/10/20/40s。
- **完成时 URL 解析兼容**：文档标准为 `metadata.url`，原项目实测中国站为顶层 `url` → 优先 `metadata.url`，回退顶层 `url`。
- 任务失败：读取 `error` 字段输出原因，**不影响**其他已完成段；单段可删后重跑。
- 下载校验：mp4 大小 > 20KB 才算有效（防全黑/全白/空文件）。
- 并发：默认 `--concurrency 1`（免费 key 限流），预留提升通道。

---

## 7. TTS（Rust 原生 edge-tts）

### 7.1 选型（spike 已完成 ✅）

候选：
- `kothok-edge-tts`（crates.io，0.2.10，复刻 Edge「Read Aloud」WebSocket 协议，400+ 神经音色，含 Sec-MS-GEC 鉴权）
- `edge-tts-rust`（crates.io，异步流式合成，面向长文本/后端负载）
- `ganlvtech/edge-tts`（GitHub，简单实现，可作协议参考）

**结论：选用 `kothok-edge-tts`**（MIT，纯 Rust 编译：tokio + tungstenite + rustls，无系统 TLS 依赖；
API 为 `EdgeTts.synthesize(text, voice, rate, lang)` 返回 MP3 帧流，自带 `Engine` trait 可替换后端，
内置 token 轮换）。`edge-tts-rust` 文档/活跃度弱于前者，未选。

**Spike 实测结果**（全部通过）：
1. `zh-CN-XiaoyiNeural`（女）/ `zh-CN-YunxiNeural`（男）/ `en-US-JennyNeural` 均出 mp3 ✅
2. 输出为 24kHz 48kbps mono mp3，ffprobe 可读、时长正确，可直接 ffmpeg 合成 ✅
3. 长句上限按 kothok 文档 ~4KB/次，远超本项目单句 36/120 字符限制 ✅
4. 实测出现瞬时 `Connection reset by peer`，已实现 `synthesize_with_retry`（3 次/1s 间隔）自愈 ✅
5. 幂等：已存在的 `<id>.mp3` 自动跳过，重跑不重复合成 ✅

### 7.2 抽象

```rust
trait TtsProvider {
    async fn synthesize(&self, text: &str, voice: &str, out: &Path) -> Result<()>;
}
```

默认实现 = Rust 原生 edge-tts；若 spike 失败，兜底实现 = 调用 `edge-tts` CLI（`Command` 封装），接口不变。

### 7.3 音色与速率

- 中文默认 `zh-CN-XiaoyiNeural`，英文默认 `en-US-JennyNeural`，男声 `zh-CN-YunxiNeural` / `en-US-GuyNeural`。
- 速率参数 `speed`（默认 1.0），写入 storyboard，供后续复用。

---

## 8. 组装渲染（ffmpeg + libass）

### 8.1 字幕（ASS）

- 生成全局 `subtitles.ass`：每场一条 Dialogue，`Start/End` 由累计时长偏移计算（字幕略早于旁白出现，视觉铺垫）。
- 样式按 `StyleProfile.subtitle`：字体（中文 MaShanZheng / 思源黑体 OFL）、字号、位置（底部安全区上方）、描边、颜色。
- 字体通过 `fontsdir` 指定，随仓库分发 OFL 字体，`--font-dir` 可覆盖（如系统字体）。
- 多行控制：单场字幕 ≤ 3 行，超长自动按词/字截断或缩小字号。

### 8.2 合成流程

```
① 每场：ffmpeg -i sXX.mp4 -i sXX.mp3 -c:v libx264 -c:a aac -shortest clip_XX.mp4
     （视频时长 ≈ 音频时长，差 1/24s 内；视频略短冻最后一帧由 -shortest 语义处理）
② concat demuxer（concat.txt 列出 clip_XX）→ story_merged.mp4
③ ffmpeg -i story_merged.mp4 -vf "ass=subtitles.ass:fontsdir=assets/fonts" -c:v libx264 -c:a aac → out/<title>_<platform>.mp4
④ 校验：ffprobe 检查最终 mp4 video/audio duration 差 < 0.5s
```

- 默认输出 720p 档（预览即成片）；`--resolution 1080` 显式要求才出 1080p（沿用原项目「默认不跑高清」原则）。
- 预留：BGM 混音（`--bgm`，amix 音量归一），M3 可选实现。

---

## 9. 交互模式设计（`agnes-video-free` 无子命令进入）

对话式向导，每步可退出（`Ctrl-C`/`q`），中途产物全部落盘：

```
┌ 欢迎：agnes-video-free ─────────────────────────────┐
│ 1. 选择风格      [crayon / textbook / realistic-*]  │  ← inquire Select
│ 2. 选择平台预设  [TikTok 9:16 / 小红书 3:4 / 微博 16:9] │
│ 3. 输入故事      [粘贴文本 / 读取文件 / 打开 $EDITOR] │
│ 4. 分句预览      [展示 scenes，可增删改每句]         │
│ 5. 语言与音色    [zh-CN-XiaoyiNeural / ...]          │
│ 6. 标题/输出目录 [默认 <cwd>/<title>/]               │
│ 7. 确认并执行    [显示估算段数/时长/耗时，dry-run 预览 prompt] │
│ 8. 执行流水线    [每段进度：TTS ✓ 视频 42% 下载 ✓]   │
│ 9. 单段失败      [选择 重试该段 / 跳过 / 终止]       │
│ 10. 完成         [输出成片路径 + 打开所在目录]       │
└─────────────────────────────────────────────────────┘
```

原则：交互模式只是 `pipeline.rs` 编排的薄壳；每个向导步骤都对应一个子命令，保证两种模式行为一致。

---

## 10. Agent Skill 模式设计

### 10.1 SKILL.md

仓库根目录 `SKILL.md`（frontmatter：`name: agnes-video-free`、`description` + 触发词：短视频、文生视频、竖屏视频、TikTok 视频、小红书视频、Agnes 视频、故事视频等）。agent 安装本仓库为 skill 后按文档工作。

### 10.2 子命令契约（agent 逐步调用，幂等可断点）

| 子命令 | 作用 | 产物 |
|---|---|---|
| `init` | 初始化工作区（目录、fonts、storyboard 骨架） | 工作区 |
| `split <story.txt>` | 分句 → 写 scenes | storyboard.json（scenes 部分） |
| `tts` | 生成旁白 mp3（存在即 skip） | audio/narration/sXX.mp3 |
| `video` | 量时长→算帧数→拼 prompt→提交→轮询→下载 | assets/videos/sXX.mp4 |
| `storyboard` | 汇总时长/帧数/prompt 快照 | storyboard.json（完整） |
| `assemble` | 合成 clip → concat → 烧字幕 | out/<title>_<platform>.mp4 |
| `all` | 全流程（等价交互模式第 7-10 步） | 成片 |
| `resume` | 从上次中断处继续（按 storyboard 状态机） | — |
| `status` | 打印各段状态表（done/skip/failed/pending） | — |
| `clean --scene s07` | 删单段素材重跑（硬规则约束下唯一允许的清理） | — |

所有子命令 `--dry-run` 只输出将要执行的步骤与 prompt，不发请求。

### 10.3 断点续跑状态机

每段状态由文件存在性 + storyboard 字段推导（无独立状态库）：

```
pending → tts_done → video_done → clip_done → assembled
```

`resume` 遍历 scenes，跳过 `video_done` 及以后的段，只跑未完成的。

---

## 11. GUI（未来计划，M4）

- **目标**：本地桌面应用，可视化全流程（选风格→贴故事→实时预览成片），面向不懂命令行的内容创作者。
- **候选方案**：
  - **Tauri 2**（Web 前端 + Rust 内核）：UI 丰富度最高，可复用现有交互逻辑与播放器生态；代价是引入 Web 前端依赖。
  - **egui**（纯 Rust 即时模式）：零 Web 依赖、单二进制；交互复杂度高的表单与预览体验弱于 Tauri。
  - 倾向 **Tauri**（预览播放、拖拽素材、进度可视化更自然），M4 启动时做 1 周 spike 定案。
- **拆分**：GUI 阶段将单 crate 拆为 workspace：`crates/agnes-core`（纯逻辑，无 CLI 依赖）、`crates/agnes-cli`、`crates/agnes-gui`。
- **范围**：风格/平台选择、故事编辑、分句预览、执行进度、成片播放与导出、历史项目管理。

---

## 12. 里程碑

### M0 — 脚手架与核心管线骨架（✅ 已完成）
- [x] `cargo init` + clap 子命令框架（split/tts/video/assemble/all；config 的 key 查找延至 M1）
- [x] 数据模型（models.rs）、`split` 分句中/英、num_frames 计算
- [x] 风格注册表骨架（realistic 三档完整；crayon/textbook 待落地，延至 M3 前）
- [x] `split` / `all --dry-run`：story.txt → scenes + prompts 预览（含英文 visual_plan 缺失校验）
- **验收**：对 examples/story_realistic.txt 输出正确分句与三段式 prompt（realistic 风格头）✅

### M1 — 素材生成（TTS ✅ / 视频客户端 ✅）
- [x] edge-tts spike 选型（结论：`kothok-edge-tts`）→ `TtsProvider` trait + `EdgeTtsProvider` 实现，中/英/男/女声实测出 mp3
- [x] `tts` 子命令：读 storyboard → 逐场合成 `audio/narration/sXX.mp3`（幂等跳过 + 重试自愈 + `--voice/--gender/--speed`）
- [x] ffprobe 封装（`src/media/ffprobe.rs`：时长/视频尺寸 JSON 探测）+ 旁白时长驱动帧数
- [x] AgnesClient（`src/agnes.rs`）：`POST /v1/videos` 创建、`GET /agnesapi?video_id=` 轮询、下载、429/5xx 退避、`metadata.url` / 顶层 `url` 兼容
- [x] `video` 子命令 + 断点续跑：已有有效 MP4 自动跳过；每场成功后立即更新 storyboard
- [x] wiremock 集成测试：请求路径、Bearer、任务创建→完成→CDN 下载链路
- [ ] 真实 API 跑通 2-3 句 demo，产出 `assets/videos/sXX.mp4`（需用户提供/配置有效 `AGNES_API_KEY`）
- **客户端验收**：本地 cargo/nix 测试通过；真实 API demo 待配置 key 后执行

### M2 — 成片组装（ffmpeg + libass）（✅ 已完成）
- [x] ASS 生成（样式/时间轴/字体）、clip 合成、concat、烧录
- [x] `assemble` 子命令：支持 storyboard 路径、音频/视频/字体目录、输出路径；Nix 包通过 `AGNES_VIDEO_FREE_FONTS` 自动回退字体目录
- [x] 每场旁白 ffprobe 实测时长与 `num_frames` 回写 storyboard；最终输出画幅校验
- [x] 字幕转义与中英文自动换行，ASS 时间轴按累计旁白时长生成
- **验收**：临时测试素材全流程产出 720×1280 H.264/AAC MP4（11.69s），字幕烧录成功，最终 ffprobe 音视频可读且时长 11.685s ✅

### M3 — 交互模式 + 风格完善 + Agent Skill
- [ ] 交互式向导（inquire）全流程
- [ ] realistic 风格族三档（cinematic/vlog/documentary）+ 平台预设
- [ ] textbook 简化方案评估（教学卡用 ASS 模拟 or 降级为字幕，决策后实现）
- [ ] SKILL.md + references 文档 + `resume/status/clean`
- **验收**：三种模式（交互/子命令/skill）均能从同一 story.txt 产出 TikTok/小红书/微博 三平台成片

### M4 — GUI + 发布（未来）
- [x] **NixOS Flake 打包**（提前完成）：`flake.nix` / `shell.nix` / `flake.lock`，参照 simple-translation 方式，`rustPlatform.buildRustPackage` + cargoLock，运行时注入 ffmpeg PATH / `SSL_CERT_FILE` / 随包字体 `AGNES_VIDEO_FREE_FONTS`；`nix build` 与 `nix run .#` 实测通过
- [ ] Tauri vs egui spike → GUI 实现（范围见 §11）
- [ ] 打包发布（cargo-dist / GitHub Actions，Win/macOS/Linux）
- [ ] 平台发布模板（标题/话题标签/封面帧导出）

---

## 13. 测试策略

| 层 | 内容 |
|---|---|
| 单元 | 分句（中/英、超长再切）、num_frames 8n+1/clamp、ASS 时间轴与样式、prompt 三段式拼接、URL 解析兼容 |
| 集成（wiremock） | Agnes 创建/轮询/失败/429 重试/下载、edge-tts 失败重试（mock 网络层） |
| E2E 冒烟 | 真实 API + 系统 ffmpeg，短故事全流程（标记 `--ignored` 默认不跑） |
| 手工验收 | 音画同步 <0.5s、字幕 ≤3 行、素材 >20KB、resume 中断恢复 |

---

## 14. 风险与对策

| 风险 | 对策 |
|---|---|
| edge-tts 协议变更 / crate 不可用 | `TtsProvider` 抽象；兜底实现调 edge-tts CLI；spike 先行 |
| Agnes 文档与实测差异（URL 字段、限流） | URL 双字段解析；429/5xx 退避；`--concurrency 1` 默认 |
| 免费 key 限流导致全流程慢 | 串行 + 断点续跑 + 明确进度提示；预留并发参数 |
| 视频模型画出乱码文字 | negative 强制排除；字幕一律本地 ASS 渲染 |
| realistic 风格下人物/场景崩坏 | 沿用「70% 即交付」；单段可删重跑；visual_plan 提供构图约束 |
| 字体版权 / 中文排版 | 仅用 OFL 字体（MaShanZheng / 思源黑体）；`--font-dir` 可换 |
| Windows 下路径/子进程编码 | 统一 UTF-8；ffmpeg 子进程用参数数组而非 shell 拼接 |

---

## 15. 开放问题（待定，不阻塞 M0）

1. **项目正式命名**：当前工作目录为 `agnes-video-free`，可沿用；也可定别名（如 `story-reel`）。
2. **realistic 风格细分**：先做 cinematic / vlog / documentary 三档，后续按平台数据加（如 `realistic-food`）。
3. **textbook 教学卡**：原教学卡是 React 组件；Rust 版用 ASS 模拟或降级为普通字幕，M3 决策。
4. **BGM 支持**：`--bgm` 混音是否进入 M3 范围（默认 M3 可选实现）。
5. **多语言旁白**：中英之外的音色（如日语）是否纳入，看社区需求。

---

## 16. 快速开始（目标形态预览）

```bash
# 交互模式（推荐人类用户）
agnes-video-free
# → 选风格/平台 → 贴故事 → 等成片

# 子命令模式（脚本/agent）
agnes-video-free init --title 我的小猫 --style realistic-vlog --platform xiaohongshu
agnes-video-free split story.txt
agnes-video-free tts
agnes-video-free video --concurrency 1
agnes-video-free assemble
agnes-video-free all --dry-run   # 先看分句和 prompt

# 环境变量
export AGNES_API_KEY=sk-...   # 或在工作区放 .env
```
