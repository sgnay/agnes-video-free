//! 全流程编排。
//!
//! M0 覆盖「分句 → scenes → prompt 预览 → storyboard.json」；TTS / 视频生成 /
//! 成片组装分别在 M1 / M2 接入。供子命令、交互模式、agent skill 共用。

use crate::models::{Lang, Scene, Storyboard, StyleProfile};
use crate::split::split_story;

/// 分句并为每场拼三段式 prompt。
///
/// M0 的 SCENE_BODY 为原句直塞（尚无 visual_plan 支持）：中文原句 Agnes 可直接理解；
/// 英文建议后续提供 visual_plan 以获得更稳定的画面。
pub fn plan_scenes(story: &str, lang: Lang, style: &StyleProfile) -> Vec<Scene> {
    split_story(story, lang)
        .into_iter()
        .enumerate()
        .map(|(i, caption)| {
            let prompt = style.build_prompt(&caption);
            Scene {
                id: format!("s{:02}", i + 1),
                caption: caption.clone(),
                narration: caption,
                prompt: Some(prompt),
                negative_prompt: Some(style.negative.clone()),
                narration_audio: None,
                motion_video: None,
                duration_sec: None,
                num_frames: None,
            }
        })
        .collect()
}

/// 汇总成 Storyboard（写入 storyboard.json 的单一数据源，PLAN.md §3.4）。
pub fn build_storyboard(
    title: &str,
    lang: Lang,
    style: &StyleProfile,
    scenes: Vec<Scene>,
) -> Storyboard {
    Storyboard {
        title: title.to_string(),
        lang: lang.label().to_string(),
        style: style.id.to_string(),
        width: style.canvas.0,
        height: style.canvas.1,
        fps: 30,
        frame_rate_video: 24,
        scenes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::styles;

    #[test]
    fn plan_scenes_builds_prompts_with_style() {
        let style = styles::by_id("realistic-cinematic").unwrap();
        let scenes = plan_scenes("下雨天。我在巷口捡到一只橘猫。", Lang::Zh, &style);
        assert_eq!(scenes.len(), 2);
        assert_eq!(scenes[0].id, "s01");
        assert_eq!(scenes[0].caption, "下雨天。");
        assert_eq!(scenes[0].narration, "下雨天。");

        let prompt = scenes[0].prompt.as_deref().unwrap();
        assert!(prompt.starts_with("cinematic realism"));
        assert!(prompt.contains("下雨天。"));
        assert!(prompt.ends_with(style.motion_footer));
        assert_eq!(
            scenes[0].negative_prompt.as_deref(),
            Some(style.negative.as_str())
        );
    }

    #[test]
    fn storyboard_carries_style_canvas() {
        let style = styles::by_id("realistic-vlog").unwrap();
        let scenes = plan_scenes("早上好。", Lang::Zh, &style);
        let sb = build_storyboard("测试", Lang::Zh, &style, scenes);
        assert_eq!(sb.style, "realistic-vlog");
        assert_eq!((sb.width, sb.height), (1080, 1440));
        assert_eq!(sb.lang, "zh");
        assert_eq!(sb.scenes.len(), 1);
    }
}
