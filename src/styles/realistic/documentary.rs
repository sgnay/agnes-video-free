//! `realistic-documentary`：纪录片解说（微博 知识/口播）。
//!
//! 配方全文见 references/prompt-recipes.md §6。

use crate::models::{Platform, StyleProfile};

use super::compose_negative;

/// 固定风格头（`{aspect}` 由画幅推导替换）。
const STYLE_DNA: &str = "realistic documentary footage, photorealistic live-action, {aspect}, natural available light, neutral editorial color grade, sharp fine detail, steady tripod camera, observational framing, authentic real-world scenes, subtle film grain, no animation, no illustration, no cartoon, no 3D render";

/// 固定运动尾。
const MOTION_FOOTER: &str = "natural realistic motion, slow steady camera pan or tilt, natural subject movement, realistic physics, one consistent human subject, anatomically correct body, one head, two arms, two hands, five fingers per hand, keep head rotation natural and under 30 degrees, preserve face and body proportions, stable tripod-like motion, no jitter, no camera shake, no flicker, no morphing, no warping, no deformation, no watermark, no logo, no added text, settle naturally";

/// 风格专属负向词（拼在共享基线之后；防止纪录片变舞台感/慢动作）。
const NEGATIVE_EXTRA: &str =
    "dramatic stage lighting, heavy color grade, cinematic slow-motion feel";

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
    }
}
