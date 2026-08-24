//! `realistic-vlog`：生活 vlog（小红书 生活方式）。
//!
//! 配方全文见 references/prompt-recipes.md §5。

use crate::models::{Platform, StyleProfile, SubtitleStyle};

use super::compose_negative;

/// 固定风格头（`{aspect}` 由画幅推导替换）。
const STYLE_DNA: &str = "realistic lifestyle vlog footage, photorealistic live-action, {aspect}, soft natural window light, bright airy exposure, warm cozy color palette, natural skin tones, authentic everyday textures, handheld vlog camera look, shallow depth of field, subtle film grain, no animation, no illustration, no cartoon, no 3D render";

/// 固定运动尾。
const MOTION_FOOTER: &str = "natural realistic motion, casual handheld camera feel, gentle everyday subject movement, realistic physics, no morphing, no warping, no lip sync, no added text, settle naturally";

/// 风格专属负向词（拼在共享基线之后；防止 vlog 变暗调/重颗粒电影感）。
const NEGATIVE_EXTRA: &str = "heavy film grain, cinematic teal-orange grade, moody dark lighting";

/// 画幅（宽, 高）：1080×1440 = 3:4 竖屏。
const CANVAS: (u32, u32) = (1080, 1440);

/// 构建风格档案。
pub fn profile() -> StyleProfile {
    StyleProfile {
        id: "realistic-vlog",
        name: "生活 vlog",
        description: "明亮生活化写实，博主实拍感，适合小红书生活方式/好物/日常",
        default_platform: Platform::Xiaohongshu,
        style_dna: STYLE_DNA,
        motion_footer: MOTION_FOOTER,
        negative: compose_negative(NEGATIVE_EXTRA),
        canvas: CANVAS,
        subtitle: SubtitleStyle {
            font: "Source Han Sans SC",
            font_file: "SourceHanSansSC-Regular.otf",
            size: 48,   // 3:4 画幅较宽，字号按比例放大一档
            outline: 3,
            color: "&H00FFFFFF",
            outline_color: "&H00000000",
        },
    }
}
