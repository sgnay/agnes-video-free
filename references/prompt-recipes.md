# Prompt 配方 — realistic 真实感风格族

> 本文档是 `realistic-cinematic` / `realistic-vlog` / `realistic-documentary` 三个新增风格的**可直接落地**配方：
> 三段式组件全文（复制即用）、SCENE_BODY 写作规则、visual_plan 示例、反模式、Rust `StyleProfile` 字段映射。
> `crayon` / `textbook` 的配方继承自上游项目 [story-handdrawn-video](https://github.com/liangdabiao/story-handdrawn-video)，不在本文档重复。
>
> 目标平台：TikTok（9:16）、小红书（3:4）、微博（16:9）。后端：Agnes Video V2.0（`agnes-video-v2.0`，`https://apihub.agnes-ai.com`）。

---

## 1. 风格族定位

| 风格 id | 定位 | 一句话画面感 | 默认平台 |
|---|---|---|---|
| `realistic-cinematic` | 电影写实 | 「电影截图」：光影讲究、镜头语言明确、情绪氛围 | TikTok |
| `realistic-vlog` | 生活 vlog | 「博主实拍」：明亮、日常、亲近、空气感 | 小红书 |
| `realistic-documentary` | 纪录片解说 | 「纪实画面」：客观、稳定、信息感强 | 微博 |

三档共用同一套**写实负向词基线**（§2.3），差异只体现在风格头、运动尾和少量专属负向词上。

---

## 2. 三段式 prompt 模板（与 crayon 的差异）

与 crayon 完全相同的组装结构：

```
[STYLE_HEADER]   固定风格头（含画幅声明，由 canvas 参数化）
[SCENE_BODY]    该场场景描述（visual_plan 或从原句改写/直塞）
[MOTION_FOOTER] 固定运动尾
+ negative_prompt 单独传参
```

**与 crayon 的关键差异**：

| 项 | crayon | realistic 族 |
|---|---|---|
| 风格头负面约束 | `no realistic lighting, no 3D` | `no animation, no illustration, no cartoon, no 3D render`（**写实是本体**） |
| 光线 | 平涂色块，无真实光照 | **必须写光线词**（自然光/霓虹/黄昏…） |
| 镜头 | locked frontal camera | **必须写镜头词**（手持/推近/横移…） |
| 动效 | rigid paper cutouts, stop-motion | natural realistic motion, realistic physics |
| 字幕字体 | MaShanZheng 毛笔字 | 思源黑体（OFL），简洁可读 |
| scene body 来源 | 中文原句直塞可接受 | **强烈建议英文 visual_plan**（写实场景描述质量差时画面崩坏更明显） |

---

## 3. SCENE_BODY 写作规则（realistic 族通用）

### 3.1 必写三要素

每场 scene body 必须能回答：**谁 + 在哪 + 做什么**，并补足 **光线 + 镜头 + 一个能动的元素**。

```
[景别] of [主体] [doing 动作] in/at [具体环境], [光线], [镜头运动], [能动元素], [风格锚点]
```

示例拆解：

```
close-up of a baker pulling golden croissants from a wood-fired oven   ← 景别+主体+动作
in a rustic bakery,                                                     ← 在哪（具体环境）
warm morning light through a window,                                    ← 光线
slow push-in,                                                           ← 镜头
steam rising, flour dust floating in the air,                           ← 能动元素
cinematic mood                                                          ← 风格锚点
```

### 3.2 词汇表

**光线词**（每场选 1 个）：

| 中文 | 英文 |
|---|---|
| 黄昏金光 | golden hour sunlight |
| 阴天柔光 | soft overcast light |
| 窗户自然光 | soft window daylight |
| 暖色台灯 | warm lamp light |
| 霓虹夜光 | neon glow at night |
| 冷调蓝调时刻 | cool blue-hour light |
| 清晨薄雾 | soft morning mist |
| 烛光/炉火 | flickering candlelight / firelight |

**镜头词**（每场选 1 个）：

| 中文 | 英文 |
|---|---|
| 缓慢推近 | slow push-in |
| 手持跟拍 | handheld tracking shot |
| 稳定横移 | slow tracking shot |
| 固定广角 | steady wide shot |
| 缓慢摇镜 | slow pan |
| 缓慢推进 | slow dolly in |
| 轻微手持晃动 | subtle handheld feel |
| 低角度 | low-angle shot |

**能动元素**（每场至少 1 个，支撑句意）：

| 场景 | 元素 |
|---|---|
| 餐饮 | steam rising / water boiling / batter being whisked |
| 户外 | leaves falling / rain splashing / curtains moving in wind |
| 人物 | walking / adjusting a scarf / turning a page / pouring coffee |
| 器物 | a ribbon being tied / scissors cutting stems / a wheel spinning |

**具体环境词**（替代一切抽象背景）：

`alley` `bakery` `kitchen` `rooftop` `street market` `subway platform` `café window`
`water town` `workshop` `laboratory` `library` `train station` `mountain trail` `balcony`

### 3.3 中文句 → 英文 scene body 改写 checklist

无 visual_plan 时，可直接把中文原句塞进 scene body（Agnes 能理解中文语义），但**写实风格强烈建议改写**。按此 checklist 人工/agent 改写：

- [ ] ① 提取 主体（谁）+ 环境（在哪）+ 动作（做什么）
- [ ] ② 环境用具体名词（禁止 `pure white background`、`abstract`、`void`）
- [ ] ③ 补 1 个光线词
- [ ] ④ 补 1 个镜头词（写实风格**不允许** locked/static 默认）
- [ ] ⑤ 给 1 个能动的元素
- [ ] ⑥ 末尾加风格锚点（`cinematic mood` / `vlog aesthetic` / `documentary style`）
- [ ] ⑦ 通读：像一帧电影/照片描述，**不像**图标、示意图、概念图
- [ ] ⑧ 检查：无数字、无字母、无需要画出来的文字

**禁止词**（出现即重写）：

`icon` `diagram` `exploded-view` `thought bubble` `puzzle pieces` `infographic`
`floating arrow` `radiating lines` `floating stars` `same scene as X` `pure white background`

**prompt 内禁止**：像素坐标、百分比、分辨率数字（模型会字面画出来）。

### 3.4 人物一致性

纯文生视频锁不住脸，**跨场脸不一样是正常的**，不要为此重跑。一致性靠风格头锁「画面质感」（光线、色调、景深），不锁「五官」。需要强角色一致性的项目不在本期范围（预留图生视频/关键帧通道）。

---

## 4. 风格一：`realistic-cinematic`（电影写实 · TikTok）

### 4.1 风格 DNA

| 项 | 值 |
|---|---|
| 画幅 | 720×1280（9:16）@ 24fps 视频 |
| 光线 | 体积光、黄昏/霓虹/蓝调时刻，讲究明暗 |
| 镜头 | 慢推、手持跟拍、缓慢横移，节奏沉稳 |
| 色调 | 低饱和电影调色、柔和对比、35mm 胶片颗粒 |
| 质感 | 真实材质细节、浅景深 |
| 情绪 | 氛围叙事优先于信息传达 |

### 4.2 三段式组件（全文复制即用）

**STYLE_HEADER**（`aspect_line` 由 canvas 参数化，9:16 时如下）：

```
cinematic realism, photorealistic live-action cinematography, vertical 9:16 composition, natural volumetric lighting, shallow depth of field, 35mm film grain, muted cinematic color grade with soft contrast, authentic real-world textures, shot on a modern cinema camera, no animation, no illustration, no cartoon, no 3D render
```

**MOTION_FOOTER**：

```
natural realistic motion, slow subtle cinematic camera movement, gentle subject movement, realistic cloth and physics, no morphing, no warping, no lip sync, no added text, settle naturally
```

**NEGATIVE**（= 共享基线 + 专属）：

```
text, letters, subtitles, captions, Chinese characters, English words, numbers, watermark, logo, signature, border frame, cartoon, illustration, anime, 3D render, CGI artifacts, distorted faces, extra limbs, mutated hands, flickering, morphing, low quality, flat lighting, amateur video look
```

### 4.3 visual_plan 示例

```jsonc
// visual_plan.json — realistic-cinematic
{
  "01": "a young woman holding a black umbrella walking past neon signs on wet pavement at night, neon glow reflecting in puddles, medium tracking shot, slow handheld follow, rain splashing, muted cinematic tones",
  "02": "a baker pulling golden croissants from a wood-fired oven in a rustic bakery, steam rising, warm morning light through a window, close-up, slow push-in, flour dust floating in the air, cinematic mood",
  "03": "a man standing at the edge of a rooftop at dusk looking over a sea of city lights, wide shot, cool blue-hour light, wind moving his coat, slow dolly back, cinematic scale"
}
```

**完整组装示例**（第 01 场最终 prompt，三段式 + negative 效果即如此）：

```
cinematic realism, photorealistic live-action cinematography, vertical 9:16 composition, natural volumetric lighting, shallow depth of field, 35mm film grain, muted cinematic color grade with soft contrast, authentic real-world textures, shot on a modern cinema camera, no animation, no illustration, no cartoon, no 3D render
a young woman holding a black umbrella walking past neon signs on wet pavement at night, neon glow reflecting in puddles, medium tracking shot, slow handheld follow, rain splashing, muted cinematic tones
natural realistic motion, slow subtle cinematic camera movement, gentle subject movement, realistic cloth and physics, no morphing, no warping, no lip sync, no added text, settle naturally
```

### 4.4 反模式（本风格专属）

- ❌ 场景里出现「人正对镜头微笑摆 pose」→ 像广告片，不是电影感；用侧写/背影/环境带人。
- ❌ 情绪词直接写进 prompt（`sad`、`romantic`）→ 模型不擅长；用光线和场景传达（雨、黄昏、冷调）。
- ❌ 过度戏剧化灯光（`dramatic spotlight`）→ 崩成舞台感；用自然光源 + 体积光。

---

## 5. 风格二：`realistic-vlog`（生活 vlog · 小红书）

### 5.1 风格 DNA

| 项 | 值 |
|---|---|
| 画幅 | 1080×1440（3:4）@ 24fps 视频 |
| 光线 | 明亮自然光（窗光、清晨/午后日光），空气感 |
| 镜头 | 轻手持、亲近视角，像博主实拍 |
| 色调 | 暖调、自然肤色、高调明亮曝光、轻微胶片颗粒 |
| 质感 | 生活化真实材质（木桌、布料、食物） |
| 情绪 | 治愈、日常、种草感 |

### 5.2 三段式组件（全文复制即用）

**STYLE_HEADER**（3:4 时如下）：

```
realistic lifestyle vlog footage, photorealistic live-action, vertical 3:4 composition, soft natural window light, bright airy exposure, warm cozy color palette, natural skin tones, authentic everyday textures, handheld vlog camera look, shallow depth of field, subtle film grain, no animation, no illustration, no cartoon, no 3D render
```

**MOTION_FOOTER**：

```
natural realistic motion, casual handheld camera feel, gentle everyday subject movement, realistic physics, no morphing, no warping, no lip sync, no added text, settle naturally
```

**NEGATIVE**（= 共享基线 + 专属）：

```
text, letters, subtitles, captions, Chinese characters, English words, numbers, watermark, logo, signature, border frame, cartoon, illustration, anime, 3D render, CGI artifacts, distorted faces, extra limbs, mutated hands, flickering, morphing, low quality, heavy film grain, cinematic teal-orange grade, moody dark lighting
```

### 5.3 visual_plan 示例

```jsonc
// visual_plan.json — realistic-vlog
{
  "01": "hands pouring steamed milk into a latte with latte art at a bright windowsill, soft morning light, cozy home setting, close-up, gentle handheld, steam rising, bright airy vlog aesthetic",
  "02": "arranging fresh tulips into a glass vase on a wooden desk by a sunny window, medium shot, soft daylight, gentle handheld, trimming stems with scissors, bright airy vlog look",
  "03": "a person picking fresh vegetables at a lively morning market stall, natural daylight, warm tones, medium shot, casual handheld, vendor handing over a paper bag, everyday vlog feel"
}
```

### 5.4 反模式（本风格专属）

- ❌ 用电影感重颗粒/暗调 → 小红书要「亮、干净」；负向词已加 `heavy film grain` / `moody dark lighting`。
- ❌ 场景过于宏大（城市航拍、史诗构图）→ 生活 vlog 要「身边感」；聚焦一双手、一个桌面、一个摊位。
- ❌ 人物摆拍感 → 用「手在做某事」代替「人看着镜头」。

---

## 6. 风格三：`realistic-documentary`（纪录片解说 · 微博）

### 6.1 风格 DNA

| 项 | 值 |
|---|---|
| 画幅 | 1280×720（16:9）@ 24fps 视频 |
| 光线 | 自然可用光，中性纪实 |
| 镜头 | 稳定三脚架/云台感，缓慢摇移，观察式构图 |
| 色调 | 中性编辑调色、细节锐利、轻微颗粒 |
| 质感 | 真实场景（古镇、作坊、实验室、车站） |
| 情绪 | 客观、信息感、时间感 |

### 6.2 三段式组件（全文复制即用）

**STYLE_HEADER**（16:9 时如下）：

```
realistic documentary footage, photorealistic live-action, horizontal 16:9 composition, natural available light, neutral editorial color grade, sharp fine detail, steady tripod camera, observational framing, authentic real-world scenes, subtle film grain, no animation, no illustration, no cartoon, no 3D render
```

**MOTION_FOOTER**：

```
natural realistic motion, slow steady camera pan or tilt, natural subject movement, realistic physics, no morphing, no warping, no added text, settle naturally
```

**NEGATIVE**（= 共享基线 + 专属）：

```
text, letters, subtitles, captions, Chinese characters, English words, numbers, watermark, logo, signature, border frame, cartoon, illustration, anime, 3D render, CGI artifacts, distorted faces, extra limbs, mutated hands, flickering, morphing, low quality, dramatic stage lighting, heavy color grade, cinematic slow-motion feel
```

### 6.3 visual_plan 示例

```jsonc
// visual_plan.json — realistic-documentary
{
  "01": "an ancient Chinese water town at dawn, empty stone-paved street between traditional houses, soft morning mist over the canal, steady wide shot, slow pan across rooftops, documentary style",
  "02": "an elderly silversmith hammering a silver bracelet in a traditional workshop, tools spread on the bench, window light, medium shot, steady tripod, slow zoom in, documentary style",
  "03": "a researcher writing notes beside laboratory equipment in a bright modern lab, natural light, steady medium shot, slow tilt from equipment to notebook, neutral tones, documentary style"
}
```

### 6.4 反模式（本风格专属）

- ❌ 戏剧化/慢动作感 → 纪录片要「如实记录」；负向词已加 `dramatic stage lighting` / `cinematic slow-motion feel`。
- ❌ 抽象概念图（地图、流程图、时间轴）→ 知识口播的 B-roll 也要真实场景；抽象概念用字幕/封面帧表达。
- ❌ 手持晃动过强 → 用 `steady tripod` / `slow pan` 类词。

---

## 7. 平台 / 画幅 / 字幕映射

| 平台 | 默认风格 | 画幅 | Agnes 档位 | 字幕样式 |
|---|---|---|---|---|
| TikTok | `realistic-cinematic` | 720×1280 | 720p / 9:16 | 思源黑体，白字 + 半透明黑底条/描边，底部安全区上方 |
| 小红书 | `realistic-vlog` | 1080×1440 | 1080p / 3:4 | 思源黑体，字号按 3:4 放大一档，居中偏下 |
| 微博 | `realistic-documentary` | 1280×720 | 720p / 16:9 | 思源黑体，16:9 底部安全区 |

> 风格与平台可自由组合（如 TikTok 用 vlog 风），画幅跟随平台预设。Agnes 会把宽高比标准化映射到 480p/720p/1080p 三档，9:16 / 3:4 / 16:9 均在支持范围内。

---

## 8. 落地映射：`StyleProfile` 字段值（Rust 实现直接拷贝）

对应 `src/styles/realistic/*.rs`，字段见 PLAN.md §5.1：

```rust
struct StyleProfile {
    id: String,
    name: String,
    description: String,
    default_platform: Platform,   // Tiktok | Xiaohongshu | Weibo
    style_header: String,         // 含 aspect_line，由 canvas 参数化拼接
    motion_footer: String,
    negative: String,
    canvas: (u32, u32),
    subtitle: SubtitleStyle,
}
```

| 字段 | `realistic-cinematic` | `realistic-vlog` | `realistic-documentary` |
|---|---|---|---|
| `id` | `realistic-cinematic` | `realistic-vlog` | `realistic-documentary` |
| `name` | 电影写实 | 生活 vlog | 纪录片解说 |
| `description` | 电影级写实镜头，TikTok 剧情/氛围 | 明亮生活化写实，小红书生活方式 | 客观纪实视角，微博知识/口播 |
| `default_platform` | `Tiktok` | `Xiaohongshu` | `Weibo` |
| `canvas` | `(720, 1280)` | `(1080, 1440)` | `(1280, 720)` |
| `aspect_line` | `vertical 9:16 composition` | `vertical 3:4 composition` | `horizontal 16:9 composition` |
| `style_header` | §4.2 全文 | §5.2 全文 | §6.2 全文 |
| `motion_footer` | §4.2 全文 | §5.2 全文 | §6.2 全文 |
| `negative` | §4.2 全文 | §5.2 全文 | §6.2 全文 |
| 字幕字体 | 思源黑体（OFL） | 思源黑体（OFL） | 思源黑体（OFL） |

**实现要点**：
- `style_header` 在代码里拆成 `style_dna`（固定）+ `aspect_line`（按 canvas 生成）再拼接，避免画幅写死。
- 共享负向词基线抽为常量 `REALISTIC_NEGATIVE_BASE`，各风格 append 专属词。
- 三档共用 `REALISTIC_SCENE_RULES`（§3 校验逻辑：光线词/镜头词/能动元素存在性检查，dry-run 时提示缺失）。

---

## 9. 验收清单（realistic 风格渲染前必过）

技术项（同全项目）：
- [ ] 句长 ≤ 36 字（中）/ ≤ 120 字符（英）
- [ ] 每场 scene body 含 光线词 + 镜头词 + 能动元素（dry-run 校验提示）
- [ ] prompt 内无数字/像素/文字类描述
- [ ] negative 已含共享基线
- [ ] 字幕 ≤ 3 行、不压底部安全区、思源黑体可读
- [ ] 素材 > 20KB，非全黑/全白
- [ ] 最终 mp4 音画时长差 < 0.5s

质量项（70% 即交付，不阻塞出片）：
- 画面**写实不卡通**、色调与风格 DNA 一致（电影暗调 / vlog 明亮 / 纪录片中性）
- 主体对版、动作自然、无画面文字
- 跨场人脸不同属正常，不重跑
- 恐怖谷人脸 / 多手指 / 全黑全白 / 画面完全错误 → 单删该段 `<sid>.mp4` 重跑，其余段自动 skip
