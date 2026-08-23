# Linux 桌面 OCR 应用规划 v2（Rust + GPUI · NixOS 优先）

## 1. 目标与范围

**v2（本次）**：在现有 v1（图片 OCR + tesseract）基础上扩展：

1. 系统托盘常驻（显示/隐藏窗口、剪贴板快速 OCR、打开文件、退出）
2. 来源三选一：图片文件 / PDF 文件 / 剪贴板粘贴
3. 引擎插件化：编译期 trait + 注册表，默认引擎本地 tesseract

**明确不含（后续）**：批量识别、历史记录、OCR 热力图、图像编辑、GPU 端识别引擎（trait 留扩展点，不实现）。

## 2. 现状盘点（v1 基线）

| 文件 | 现状 | v2 处置 |
|------|------|---------|
| src/main.rs | 开窗 + CLI 图片路径 | 保留，增加托盘初始化与退出链路 |
| src/app.rs | 预览 + 路径输入 + 语言切换 + 识别 | 增加来源/引擎选择、页进度、结果复制 |
| src/ocr_engine.rs | tesseract CLI 封装（spawn_blocking） | 迁移为 crates/engine-tesseract |
| src/state.rs | OcrModel + 语言枚举（eng/chi_sim/…/ita） | 增加来源/引擎/进度状态 |
| flake.nix | devShell + package + wrap tesseract/Vulkan | 增加 poppler_utils、tray 图标资源 |
| resources/ | eng.png / chi.png 样例图 | 增加 tray 图标资产 |

## 3. 技术选型（已确认）

| 项 | 选择 | 理由 |
|----|------|------|
| UI | GPUI（gpui-unofficial 1.15.0, wayland+x11） | 已用、GPU 加速 |
| 默认引擎 | Tesseract 5 CLI | 本地、无网络；nixpkgs 自带全部语言数据（含 chi_sim/chi_tra/jpn/kor） |
| 插件机制 | **编译期 trait + 注册表（workspace）** | 类型安全、无 unsafe、引擎可独立单测、按 feature 开关 |
| PDF | pdftoppm（poppler_utils）CLI 栅格化 | 与现有 CLI 封装风格一致，flake 仅加一个依赖 |
| 托盘 | ksni（纯 Rust StatusNotifierItem） | 零 native 依赖、KDE 原生；GNOME 需 AppIndicator 扩展 |
| 剪贴板 | gpui ClipboardItem 优先，wl-paste/xclip CLI 兜底 | 图像支持需 R1 实测 |
| 平台 | NixOS 优先，flake 唯一构建/打包入口 | 本机即 NixOS |

## 4. 架构

```
托盘 (ksni) ──mpsc 命令──▶ App 主窗口 (gpui)
                              │
来源: 图片文件 │ PDF(pdftoppm 逐页) │ 剪贴板(临时 PNG)
                              │
                    ┌─────────▼─────────┐
                    │ Source → PageLoader│  拆页 → Vec<单页图像>
                    └─────────┬─────────┘
                              ▼
                    EngineRegistry → OcrEngine trait
                    ├── engine-tesseract（默认）
                    └──（未来引擎扩展点）
                              ▼
                    OcrOutput → 结果区 / 剪贴板
```

## 5. 引擎插件契约（crates/ocr-api）

无 gpui 依赖的纯类型层，app 与各引擎 crate 共享。

- trait `OcrEngine: Send + Sync`：
  - `id() -> &'static str`：唯一标识
  - `name() -> &str`：展示名
  - `available() -> bool`：依赖（可执行文件/语言包）是否就绪
  - `recognize(input: &OcrInput, opts: &OcrOptions) -> Result<OcrOutput, OcrError>`
- 类型：
  - `OcrInput`：单页图像（路径或内存字节）；PDF 拆页在来源层完成，引擎只见单页
  - `OcrOptions`：语言组合、psm 等
  - `OcrOutput`：文本 + 可选逐行置信度 + 耗时
  - `OcrError`：引擎缺失 / 语言包缺失 / 无效输入，带可读 message
- `EngineRegistry`：`register(Arc<dyn OcrEngine>)` / `iter()` / `get(id)`；app 启动时注册所有编译进 binary 的引擎，默认选第一个 `available()` 的

## 6. 来源层（src/sources/）

- `image.rs`：图片文件直通（迁移现有路径逻辑）
- `pdf.rs`：`pdftoppm -png -r 200 <pdf> <tmp_prefix>` 逐页栅格化 → 逐页 OCR → 页分隔符合并；状态栏显示页进度 i/N
- `clipboard.rs`：优先 gpui `ClipboardItem`（文本/图像）；图像 → 临时 PNG → OCR；文本若是文件路径 → 按文件处理；失败兜底 `wl-paste` / `xclip` CLI

## 7. 托盘（src/tray.rs）

- ksni `TrayService` 独立线程，`StatusNotifierItem`：标题、图标、菜单
- 菜单项：显示/隐藏主窗口 ｜ 剪贴板快速 OCR ｜ 打开图片/PDF ｜ 退出
- 快速 OCR：不弹窗，后台识别，结果自动写回剪贴板
- 命令通道：`std::sync::mpsc` → app 内 `cx.spawn` 后台任务 `recv` → `cx.update` 派发（GPUI 单线程模型，跨线程无共享状态）

## 8. 主窗口 UI 变更（src/app.rs / state.rs）

- 来源选择：打开文件（图片/PDF，FileDialog 或路径输入兜底）｜ 粘贴（Ctrl+V）
- 引擎选择：下拉列出注册表内全部引擎，默认第一个可用（当前即 tesseract）
- 进度：PDF 页 i/N、识别中状态
- 结果：文本区 + 「复制」按钮

## 9. NixOS 交付（flake.nix）

- buildInputs/devShell 增加 `poppler_utils`
- tray 图标打包进 store（resources/ 随构建安装）
- 运行时 wrap 不变：tesseract、TESSDATA_PREFIX、Vulkan ICD、库路径
- 文档注明：KDE 原生托盘；GNOME 需 AppIndicator 扩展

## 10. 异步与线程模型（GPUI 规范）

- OCR / pdftoppm 均 `spawn_blocking`，返回 `Task<T, E>`
- 共享状态 `Entity<Model>` + `cx.notify()`
- 托盘命令经 mpsc + 后台任务桥接，不跨线程触碰 GPUI 状态
- 不用 tokio / Arc\<Mutex\> / Rc\<RefCell\>

## 11. 里程碑

1. **M1 workspace 重构**：根包改 app 包，新增 crates/ocr-api + crates/engine-tesseract（迁移现有 ocr_engine.rs），注册表接线；`cargo test` + `nix run` 不回归
2. **M2 来源层**：PDF 栅格化逐页识别、剪贴板读取（含 R1 图像支持验证与 CLI 兜底）
3. **M3 托盘**：ksni 接入、命令通道、剪贴板快速 OCR、退出链路（进程干净退出）
4. **M4 UI 完善**：来源/引擎选择、页进度、结果复制
5. **M5 打包与验证**：flake 更新、tray 图标、端到端冒烟（真 PDF + 剪贴板粘贴 + 托盘点击全链路）

## 12. 风险清单（R1 验证项）

- gpui 1.15 `ClipboardItem` 图像支持（Wayland/X11）→ 兜底 wl-paste / xclip
- ksni 需系统 tray host（KDE 原生；GNOME 需扩展）→ 文档注明，验证用 KDE 或独立 host
- 大 PDF 栅格化耗时 → 页进度提示 + spawn_blocking 不卡 UI
- 托盘线程与 GPUI 事件循环共存 → mpsc 桥接；验证退出时托盘线程随进程结束
- FileDialog 平台限制（沿用 v1 结论）→ 路径输入兜底

## 13. 交付物

- workspace：crates/ocr-api、crates/engine-tesseract、app 包（现有 src/）
- `nix run .#` 可运行：托盘 + 图片/PDF/剪贴板来源 + tesseract 识别
- 单元测试：ocr-api 契约、engine-tesseract（mock CLI）、pdf 拆页
- README：安装、使用、GNOME 托盘说明
