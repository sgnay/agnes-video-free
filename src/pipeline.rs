//! 全流程编排。
//!
//! M0 覆盖「分句 → scenes → prompt 预览 → storyboard.json」；TTS / 视频生成 /
//! 成片组装分别在 M1 / M2 接入。供子命令、交互模式、agent skill 共用。

use std::collections::BTreeMap;

use crate::models::{Lang, Scene, Storyboard, StyleProfile};
use crate::split::split_story;

/// 分句并为每场拼三段式 prompt；没有 visual plan 时兼容使用原句。
pub fn plan_scenes(story: &str, lang: Lang, style: &StyleProfile) -> Vec<Scene> {
    plan_scenes_with_visual_plan(story, lang, style, None)
}

/// 分句并使用可选 visual plan 覆盖每场的 `SCENE_BODY`。
///
/// visual plan 的 key 可以是 `s01` 或 `01`；未匹配的场景继续使用分句原文。
pub fn plan_scenes_with_visual_plan(
    story: &str,
    lang: Lang,
    style: &StyleProfile,
    visual_plan: Option<&BTreeMap<String, String>>,
) -> Vec<Scene> {
    split_story(story, lang)
        .into_iter()
        .enumerate()
        .map(|(i, caption)| {
            let id = format!("s{:02}", i + 1);
            let visual = visual_plan.and_then(|plan| {
                plan.get(&id)
                    .or_else(|| plan.get(id.trim_start_matches('s')))
            });
            let visual = visual.map(|value| value.trim().to_string());
            let visual = visual.filter(|value| !value.is_empty());
            let prompt = style.build_prompt(visual.as_deref().unwrap_or(&caption));
            Scene {
                id,
                caption: caption.clone(),
                narration: caption,
                visual,
                prompt: Some(prompt),
                negative_prompt: Some(style.negative.clone()),
                narration_audio: None,
                motion_video: None,
                agnes_task_id: None,
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
        assert!(scenes[0].visual.is_none());

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
    fn visual_plan_overrides_scene_body_and_accepts_short_ids() {
        let style = styles::by_id("realistic-cinematic").unwrap();
        let mut plan = BTreeMap::new();
        plan.insert(
            "01".to_string(),
            "a woman walking through a rainy neon alley at night, slow tracking shot".to_string(),
        );
        let scenes = plan_scenes_with_visual_plan(
            "下雨天。我在巷口捡到一只橘猫。",
            Lang::Zh,
            &style,
            Some(&plan),
        );
        assert_eq!(
            scenes[0].visual.as_deref(),
            Some("a woman walking through a rainy neon alley at night, slow tracking shot")
        );
        let prompt = scenes[0].prompt.as_deref().unwrap();
        assert!(prompt.contains("a woman walking through a rainy neon alley"));
        assert!(!prompt.contains("下雨天。"));
        assert!(scenes[1].visual.is_none());
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
