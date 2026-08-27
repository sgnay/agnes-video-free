//! 领域模型：视觉场景计划、Storyboard、风格与画幅。

use std::str::FromStr;

#[derive(Debug, Clone, Copy)]
pub enum Platform {
    Tiktok,
    Xiaohongshu,
    Weibo,
}

impl Platform {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Tiktok => "TikTok",
            Self::Xiaohongshu => "小红书",
            Self::Weibo => "微博",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
}

impl Lang {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Zh => "zh",
            Self::En => "en",
        }
    }
}

impl FromStr for Lang {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "zh" | "cn" => Ok(Self::Zh),
            "en" => Ok(Self::En),
            other => Err(format!("不支持的语言「{other}」，可选 zh / en")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SubtitleStyle {
    pub font: &'static str,
    pub size: u32,
    pub outline: u32,
    pub color: &'static str,
    pub outline_color: &'static str,
}

#[derive(Debug, Clone)]
pub struct StyleProfile {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub default_platform: Platform,
    pub style_dna: &'static str,
    pub motion_footer: &'static str,
    pub negative: String,
    pub canvas: (u32, u32),
}

impl StyleProfile {
    pub fn aspect_line(&self) -> &'static str {
        let (width, height) = self.canvas;
        if width * 16 == height * 9 {
            "vertical 9:16 composition"
        } else if width * 4 == height * 3 {
            "vertical 3:4 composition"
        } else if width * 9 == height * 16 {
            "horizontal 16:9 composition"
        } else {
            "composition matched to canvas"
        }
    }

    pub fn style_header(&self) -> String {
        self.style_dna.replace("{aspect}", self.aspect_line())
    }

    pub fn build_prompt(&self, visual: &str) -> String {
        format!(
            "{}\n{}\n{}",
            self.style_header(),
            visual,
            self.motion_footer
        )
    }
}

/// One independent visual shot from visual_plan.v2.json.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VisualSceneSpec {
    pub id: String,
    pub visual: String,
    pub duration_sec: f64,
    /// 可选参考图（本地路径或 http(s) URL），存在时生成 ti2vid 图生视频。
    pub image: Option<String>,
    /// 可选关键帧动画参考图（至少 2 张，本地路径或 http(s) URL），存在时生成 keyframes 动画。
    #[serde(default)]
    pub keyframes: Vec<String>,
}

/// A visual scene is deliberately independent from audio and subtitle cues.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Scene {
    pub id: String,
    pub visual: String,
    pub prompt: String,
    pub negative_prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motion_video: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agnes_task_id: Option<String>,
    /// 可选参考图（本地路径或 http(s) URL），存在时该场景以 ti2vid 模式生成。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// 可选关键帧动画参考图（至少 2 张），存在时该场景以 keyframes 模式生成。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keyframes: Vec<String>,
    pub duration_sec: f64,
    pub num_frames: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Storyboard {
    pub title: String,
    pub lang: String,
    pub style: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub frame_rate_video: u32,
    pub scenes: Vec<Scene>,
}

/// Agnes accepts only 8n+1 frames, with a 1.7s..18.3s scene duration range.
pub fn num_frames_for_duration(duration_sec: f64) -> u32 {
    let target = (duration_sec * 24.0).round().max(0.0) as u64;
    let n = target.saturating_sub(1).div_ceil(8);
    (8 * n + 1).clamp(41, 441) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn num_frames_follows_8n_plus_1_with_clamps() {
        assert_eq!(num_frames_for_duration(4.82), 121);
        assert_eq!(num_frames_for_duration(5.0), 121);
        assert_eq!(num_frames_for_duration(10.0), 241);
        assert_eq!(num_frames_for_duration(1.0), 41);
        assert_eq!(num_frames_for_duration(18.5), 441);
        for duration in [1.7, 3.0, 7.3, 12.9, 18.3] {
            let frames = num_frames_for_duration(duration);
            assert!((41..=441).contains(&frames));
            assert_eq!((frames - 1) % 8, 0);
        }
    }

    #[test]
    fn visual_scene_round_trips_without_audio_fields() {
        let scene = Scene {
            id: "v01".to_string(),
            visual: "a quiet street, morning light, slow tracking shot".to_string(),
            prompt: "prompt".to_string(),
            negative_prompt: "negative".to_string(),
            motion_video: None,
            agnes_task_id: None,
            image: None,
            keyframes: vec![],
            duration_sec: 8.0,
            num_frames: 193,
        };
        let value = serde_json::to_value(scene).unwrap();
        assert!(value.get("caption").is_none());
        assert!(value.get("narration").is_none());
        assert!(value.get("image").is_none());
        assert!(value.get("keyframes").is_none());
        assert_eq!(value["duration_sec"], 8.0);
    }

    #[test]
    fn scene_serializes_reference_image_when_present() {
        let scene = Scene {
            id: "v01".to_string(),
            visual: "a quiet street, morning light, slow tracking shot".to_string(),
            prompt: "prompt".to_string(),
            negative_prompt: "negative".to_string(),
            motion_video: None,
            agnes_task_id: None,
            image: Some("refs/woman.png".to_string()),
            keyframes: vec![],
            duration_sec: 8.0,
            num_frames: 193,
        };
        let value = serde_json::to_value(&scene).unwrap();
        assert_eq!(value["image"], "refs/woman.png");
        let round_trip: Scene = serde_json::from_value(value).unwrap();
        assert_eq!(round_trip.image.as_deref(), Some("refs/woman.png"));
    }

    #[test]
    fn scene_serializes_keyframes_when_present() {
        let scene = Scene {
            id: "v01".to_string(),
            visual: "a quiet street, morning light, slow tracking shot".to_string(),
            prompt: "prompt".to_string(),
            negative_prompt: "negative".to_string(),
            motion_video: None,
            agnes_task_id: None,
            image: None,
            keyframes: vec!["refs/a.png".to_string(), "refs/b.png".to_string()],
            duration_sec: 8.0,
            num_frames: 193,
        };
        let value = serde_json::to_value(&scene).unwrap();
        assert_eq!(value["keyframes"][0], "refs/a.png");
        assert_eq!(value["keyframes"].as_array().unwrap().len(), 2);
        let round_trip: Scene = serde_json::from_value(value).unwrap();
        assert_eq!(round_trip.keyframes.len(), 2);
    }

    #[test]
    fn lang_parses_zh_and_en() {
        assert_eq!("zh".parse::<Lang>().unwrap(), Lang::Zh);
        assert_eq!("CN".parse::<Lang>().unwrap(), Lang::Zh);
        assert_eq!("en".parse::<Lang>().unwrap(), Lang::En);
        assert!("ja".parse::<Lang>().is_err());
    }
}
