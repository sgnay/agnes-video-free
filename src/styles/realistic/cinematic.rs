//! `realistic-cinematic`：电影写实（TikTok 剧情/氛围）。
//!
//! 配方全文见 references/prompt-recipes.md §4。

use crate::models::{Platform, StyleProfile, SubtitleStyle};

use super::compose_negative;

/// 固定风格头（`{aspect}` 由画幅推导替换）。
const STYLE_DNA: &str = "cinematic realism, photorealistic live-action cinematography, {aspect}, natural volumetric lighting, shallow depth of field, 35mm film grain, muted cinematic color grade with soft contrast, authentic real-world textures, shot on a modern cinema camera, no animation, no illustration, no cartoon, no 3D render";

/// 固定运动尾。
const MOTION_FOOTER: &str = "natural realistic motion, slow subtle cinematic camera movement, gentle subject movement, realistic cloth and physics, no morphing, no warping, no lip sync, no added text, settle naturally";

/// 风格专属负向词（拼在共享基线之后）。
const NEGATIVE_EXTRA: &str = "flat lighting, amateur video look";

/// 画幅（宽, 高）：720×1280 = 9:16 竖屏。
const CANVAS: (u32, u32) = (720, 1280);

/// 构建风格档案。
pub fn profile() -> StyleProfile {
    StyleProfile {
        id: "realistic-cinematic",
        name: "电影写实",
        description: "电影级写实镜头，光影讲究、镜头语言明确，适合 TikTok 剧情/氛围/口播 B-roll",
        default_platform: Platform::Tiktok,
        style_dna: STYLE_DNA,
        motion_footer: MOTION_FOOTER,
        negative: compose_negative(NEGATIVE_EXTRA),
        canvas: CANVAS,
        subtitle: SubtitleStyle {
            font: "Source Han Sans SC",
            font_file: "SourceHanSansSC-Regular.otf",
            size: 40,   // 720 宽基准
            outline: 3,
            color: "&H00FFFFFF",
            outline_color: "&H00000000",
        },
    }
}
