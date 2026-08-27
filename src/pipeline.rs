//! 视觉场景流水线：visual_plan.v2.json -> prompt -> storyboard.json。

use crate::models::{
    Lang, Scene, Storyboard, StyleProfile, VisualSceneSpec, num_frames_for_duration,
};

/// 根据独立视觉计划构建 storyboard。
///
/// 每个数组项对应一个视觉镜头；音频和字幕在 assemble 阶段单独处理，
/// 因此本模块不会读取故事文本，也不会按字幕或句子切分镜头。
pub fn build_visual_storyboard(
    title: &str,
    lang: Lang,
    style: &StyleProfile,
    visual_scenes: Vec<VisualSceneSpec>,
) -> Storyboard {
    let scenes = visual_scenes
        .into_iter()
        .map(|spec| {
            let duration_sec = spec.duration_sec;
            Scene {
                id: spec.id,
                visual: spec.visual.clone(),
                prompt: style.build_prompt(&spec.visual),
                negative_prompt: style.negative.clone(),
                motion_video: None,
                agnes_task_id: None,
                image: spec.image,
                keyframes: spec.keyframes,
                duration_sec,
                num_frames: num_frames_for_duration(duration_sec),
            }
        })
        .collect();
    build_storyboard(title, lang, style, scenes)
}

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
    fn visual_storyboard_keeps_audio_and_subtitles_independent() {
        let style = styles::by_id("realistic-cinematic").unwrap();
        let storyboard = build_visual_storyboard(
            "测试",
            Lang::Zh,
            &style,
            vec![VisualSceneSpec {
                id: "v01".to_string(),
                visual: "a rainy street, morning light, slow tracking shot".to_string(),
                duration_sec: 8.0,
                image: None,
                keyframes: vec![],
            }],
        );
        let scene = &storyboard.scenes[0];
        assert_eq!(storyboard.scenes.len(), 1);
        assert_eq!(scene.duration_sec, 8.0);
        assert_eq!(scene.num_frames, 193);
        assert!(scene.prompt.contains("rainy street"));
        assert!(!scene.prompt.contains("测试"));
        let json = serde_json::to_value(scene).unwrap();
        assert!(json.get("caption").is_none());
        assert!(json.get("narration").is_none());
    }

    #[test]
    fn storyboard_carries_reference_image_for_ti2vid() {
        let style = styles::by_id("realistic-cinematic").unwrap();
        let storyboard = build_visual_storyboard(
            "测试",
            Lang::Zh,
            &style,
            vec![VisualSceneSpec {
                id: "v01".to_string(),
                visual: "a rainy street, morning light, slow tracking shot".to_string(),
                duration_sec: 8.0,
                image: Some("refs/woman.png".to_string()),
                keyframes: vec![],
            }],
        );
        assert_eq!(
            storyboard.scenes[0].image.as_deref(),
            Some("refs/woman.png")
        );
        assert_eq!(storyboard.scenes[0].num_frames, 193);
    }

    #[test]
    fn storyboard_carries_keyframes_for_animation() {
        let style = styles::by_id("realistic-cinematic").unwrap();
        let storyboard = build_visual_storyboard(
            "测试",
            Lang::Zh,
            &style,
            vec![VisualSceneSpec {
                id: "v01".to_string(),
                visual: "a smooth transition between two shots, morning light, slow pan"
                    .to_string(),
                duration_sec: 8.0,
                image: None,
                keyframes: vec![
                    "refs/a.png".to_string(),
                    "refs/b.png".to_string(),
                    "refs/c.png".to_string(),
                ],
            }],
        );
        assert_eq!(storyboard.scenes[0].keyframes.len(), 3);
        assert!(storyboard.scenes[0].image.is_none());
    }

    #[test]
    fn storyboard_carries_style_canvas() {
        let style = styles::by_id("realistic-vlog").unwrap();
        let storyboard = build_visual_storyboard(
            "测试",
            Lang::Zh,
            &style,
            vec![VisualSceneSpec {
                id: "v01".to_string(),
                visual: "a bright kitchen, morning light, slow push-in".to_string(),
                duration_sec: 4.0,
                image: None,
                keyframes: vec![],
            }],
        );
        assert_eq!(storyboard.style, "realistic-vlog");
        assert_eq!((storyboard.width, storyboard.height), (1080, 1440));
    }
}
