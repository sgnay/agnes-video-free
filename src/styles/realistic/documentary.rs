//! `realistic-documentary`：纪录片解说（微博 知识/口播）。
//!
//! 配方全文见 references/prompt-recipes.md §6。

use crate::models::{Platform, StyleProfile, SubtitleStyle};

use super::compose_negative;

/// 固定风格头（`{aspect}` 由画幅推导替换）。
const STYLE_DNA: &str = "realistic documentary footage, photorealistic live-action, {aspect}, natural available light, neutral editorial color grade, sharp fine detail, steady tripod camera, observational framing, authentic real-world scenes, subtle film grain, no animation, no illustration, no cartoon, no 3D render";

/// 固定运动尾。
const MOTION_FOOTER: &str = "natural realistic motion, slow steady camera pan or tilt, natural subject movement, realistic physics, no morphing, no warping, no added text, settle naturally";

/// 风格专属负向词（拼在共享基线之后；防止纪录片变舞台感/慢动作）。
const NEGATIVE_EXTRA: &str = "dramatic stage lighting, heavy color grade, cinematic slow-motion feel";

/// 画幅（宽, 高）：1280×720 = 16:9 横屏。
const CANVAS: (u32, u32) = (1280, 720);

/// 构建风格档案。
pub fn profile() -> StyleProfile {
    StyleProfile {
        id: "realistic-documentary",
        name: "纪录片解说",
        description: "客观纪实视角，稳定观察式构图，适合微博知识/历史/口播 B-roll",
        default_platform: Platform::Weibo,
        style_dna: STYLE_DNA,
        motion_footer: MOTION_FOOTER,
        negative: compose_negative(NEGATIVE_EXTRA),
        canvas: CANVAS,
        subtitle: SubtitleStyle {
            font: "Source Han Sans SC",
            font_file: "SourceHanSansSC-Regular.otf",
            size: 36,   // 720 宽基准（横屏）
            outline: 3,
            color: "&H00FFFFFF",
            outline_color: "&H00000000",
        },
    }
}
