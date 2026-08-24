//! realistic 真实感风格族。
//!
//! 三档风格共享同一负向词基线与场景校验规则（对应 references/prompt-recipes.md §2.3 / §3）：
//! - [`cinematic`]：电影写实（TikTok，9:16）
//! - [`vlog`]：生活 vlog（小红书，3:4）
//! - [`documentary`]：纪录片解说（微博，16:9）

pub mod cinematic;
pub mod documentary;
pub mod vlog;

use crate::models::StyleProfile;

/// 写实风格族共享负向词基线（prompt-recipes.md §2.3）。
pub const REALISTIC_NEGATIVE_BASE: &str = "text, letters, subtitles, captions, Chinese characters, English words, numbers, watermark, logo, signature, border frame, cartoon, illustration, anime, 3D render, CGI artifacts, distorted faces, extra limbs, mutated hands, flickering, morphing, low quality";

/// 在共享基线上追加风格专属负向词（各风格构造器调用）。
fn compose_negative(extra: &str) -> String {
    format!("{REALISTIC_NEGATIVE_BASE}, {extra}")
}

/// 三个 realistic 风格档案。
pub fn profiles() -> Vec<StyleProfile> {
    vec![
        cinematic::profile(),
        vlog::profile(),
        documentary::profile(),
    ]
}

// ---------------------------------------------------------------------------
// SCENE_BODY 校验规则（prompt-recipes.md §3.2 / §3.3）
// ---------------------------------------------------------------------------

/// 光线词表（每场至少 1 个）。
const LIGHT_WORDS: &[&str] = &[
    "light",
    "sunlight",
    "daylight",
    "glow",
    "mist",
    "lamplight",
    "candlelight",
    "firelight",
    "overcast",
    "neon",
    "blue-hour",
    "dawn",
    "dusk",
    "golden hour",
    "morning",
    "window light",
];

/// 镜头词表（每场至少 1 个）。
const CAMERA_WORDS: &[&str] = &[
    "shot",
    "close-up",
    "wide shot",
    "medium shot",
    "push-in",
    "tracking",
    "handheld",
    "pan",
    "dolly",
    "zoom",
    "tilt",
    "low-angle",
    "following",
    "over-the-shoulder",
];

/// 能动元素词表（每场至少 1 个，支撑句意）。
const MOTION_WORDS: &[&str] = &[
    "steam",
    "rising",
    "rain",
    "splash",
    "falling",
    "walking",
    "pouring",
    "flicker",
    "floating",
    "wind",
    "moving",
    "boiling",
    "whisk",
    "trimming",
    "tying",
    "hammering",
    "writing",
    "handing",
    "turning",
    "adjusting",
    "passing",
    "spinning",
    "curtain",
    "waves",
    "blowing",
    "swaying",
];

/// 禁止词表（出现即重写，prompt-recipes.md §3.3）。
const BANNED_WORDS: &[&str] = &[
    "icon",
    "diagram",
    "exploded-view",
    "thought bubble",
    "puzzle pieces",
    "infographic",
    "floating arrow",
    "radiating lines",
    "floating stars",
    "pure white background",
    "same scene as",
];

/// 对 SCENE_BODY 做写实风格规则校验，返回缺失/违规项列表（空 = 通过）。
/// dry-run 时用于提示「光线词 / 镜头词 / 能动元素」缺失。
pub fn validate_scene_body(body: &str) -> Vec<String> {
    let lower = body.to_lowercase();
    let mut issues = Vec::new();

    if !LIGHT_WORDS.iter().any(|w| lower.contains(w)) {
        issues.push("缺少光线词（light / sunlight / neon / dawn…任选其一）".to_string());
    }
    if !CAMERA_WORDS.iter().any(|w| lower.contains(w)) {
        issues.push("缺少镜头词（close-up / push-in / handheld…任选其一）".to_string());
    }
    if !MOTION_WORDS.iter().any(|w| lower.contains(w)) {
        issues.push("缺少能动元素（steam / falling / walking…任选其一）".to_string());
    }
    for banned in BANNED_WORDS {
        if lower.contains(banned) {
            issues.push(format!("包含禁止词：「{banned}」"));
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_includes_shared_base_and_style_extra() {
        let c = cinematic::profile();
        // 共享基线关键词在
        assert!(c.negative.contains("distorted faces"));
        assert!(c.negative.contains("3D render"));
        // 风格专属词在
        assert!(c.negative.contains("flat lighting"));
        // 三档都以共享基线开头
        for p in profiles() {
            assert!(
                p.negative.starts_with(REALISTIC_NEGATIVE_BASE),
                "{} 未以共享基线开头",
                p.id
            );
        }
    }

    #[test]
    fn aspect_line_matches_canvas() {
        assert_eq!(
            cinematic::profile().aspect_line(),
            "vertical 9:16 composition"
        );
        assert_eq!(vlog::profile().aspect_line(), "vertical 3:4 composition");
        assert_eq!(
            documentary::profile().aspect_line(),
            "horizontal 16:9 composition"
        );
    }

    #[test]
    fn style_header_replaces_aspect_placeholder() {
        let h = cinematic::profile().style_header();
        assert!(!h.contains("{aspect}"));
        assert!(h.contains("vertical 9:16 composition"));
        assert!(h.starts_with("cinematic realism"));
    }

    #[test]
    fn build_prompt_is_three_part() {
        let p = cinematic::profile();
        let body =
            "a cat beside a sunny window, close-up, steam rising from a cup, warm morning light";
        let prompt = p.build_prompt(body);
        let parts: Vec<&str> = prompt.split('\n').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[1], body);
        assert_eq!(parts[2], p.motion_footer);
    }

    #[test]
    fn scene_validation_flags_missing_and_banned() {
        // 三要素全缺
        let issues = validate_scene_body("a cat sitting");
        assert!(issues.iter().any(|i| i.contains("光线")), "{issues:?}");
        assert!(issues.iter().any(|i| i.contains("镜头")), "{issues:?}");
        assert!(issues.iter().any(|i| i.contains("能动")), "{issues:?}");

        // 三要素齐全 → 通过
        let ok = validate_scene_body(
            "a cat beside a sunny window, close-up, steam rising from a cup, warm morning light",
        );
        assert!(ok.is_empty(), "{ok:?}");

        // 禁止词命中
        let banned =
            validate_scene_body("an icon diagram with a thought bubble, pure white background");
        assert!(banned.iter().any(|i| i.contains("禁止词")), "{banned:?}");
    }
}
