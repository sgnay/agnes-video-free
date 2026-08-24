//! 领域模型：目标平台、字幕样式、风格档案、语言、storyboard 数据源。
//!
//! 对应 PLAN.md §3.4 / §5.1 与 references/prompt-recipes.md §8 的字段映射。

use std::str::FromStr;

/// 目标发布平台（决定默认画幅与字幕样式，见 prompt-recipes.md §7）。
#[derive(Debug, Clone, Copy)]
pub enum Platform {
    Tiktok,
    Xiaohongshu,
    Weibo,
}

impl Platform {
    /// 展示名。
    pub fn label(&self) -> &'static str {
        match self {
            Platform::Tiktok => "TikTok",
            Platform::Xiaohongshu => "小红书",
            Platform::Weibo => "微博",
        }
    }
}

/// 故事语言。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
}

impl Lang {
    /// 中文单句 softLimit（字符数）。
    pub const ZH_SOFT_LIMIT: usize = 36;
    /// 英文单句 softLimit（字符数）。
    pub const EN_SOFT_LIMIT: usize = 120;

    /// storyboard 中的标识。
    pub fn label(&self) -> &'static str {
        match self {
            Lang::Zh => "zh",
            Lang::En => "en",
        }
    }

    /// 单句 softLimit（字符数），超长会按逗号/连接词再切。
    pub fn soft_limit(&self) -> usize {
        match self {
            Lang::Zh => Lang::ZH_SOFT_LIMIT,
            Lang::En => Lang::EN_SOFT_LIMIT,
        }
    }
}

impl FromStr for Lang {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "zh" | "cn" => Ok(Lang::Zh),
            "en" => Ok(Lang::En),
            other => Err(format!("不支持的语言「{other}」，可选 zh / en")),
        }
    }
}

/// 字幕样式（ASS 渲染参数）。
#[derive(Debug, Clone)]
pub struct SubtitleStyle {
    /// libass 使用的字体族名（fontconfig 匹配）。
    pub font: &'static str,
    /// 随仓库分发的字体文件（相对 `assets/fonts/`）。
    pub font_file: &'static str,
    /// 基准字号（以 720 宽画幅为基准，组装时按实际画幅缩放）。
    pub size: u32,
    /// 描边宽度（像素）。
    pub outline: u32,
    /// 文字颜色（ASS 格式 `&HAABBGGRR`）。
    pub color: &'static str,
    /// 描边颜色（ASS 格式 `&HAABBGGRR`）。
    pub outline_color: &'static str,
}

/// 风格档案：三段式 prompt 的静态配置。
///
/// `style_dna` 中的 `{aspect}` 占位符由 [`StyleProfile::style_header`]
/// 替换为按画幅推导的画幅声明行，避免画幅写死。
#[derive(Debug, Clone)]
pub struct StyleProfile {
    /// 风格 id，如 `realistic-cinematic`。
    pub id: &'static str,
    /// 中文展示名。
    pub name: &'static str,
    /// 一句话定位。
    pub description: &'static str,
    /// 默认发布平台。
    pub default_platform: Platform,
    /// 固定风格头（不含画幅声明，含 `{aspect}` 占位符）。
    pub style_dna: &'static str,
    /// 固定运动尾。
    pub motion_footer: &'static str,
    /// 完整负向词（由构造器拼入共享基线 + 风格专属词）。
    pub negative: String,
    /// 画幅（宽, 高）。
    pub canvas: (u32, u32),
    /// 字幕样式。
    pub subtitle: SubtitleStyle,
}

impl StyleProfile {
    /// 根据画幅推导画幅声明行（9:16 / 3:4 / 16:9）。
    pub fn aspect_line(&self) -> &'static str {
        let (w, h) = self.canvas;
        if w * 16 == h * 9 {
            "vertical 9:16 composition"
        } else if w * 4 == h * 3 {
            "vertical 3:4 composition"
        } else if w * 9 == h * 16 {
            "horizontal 16:9 composition"
        } else {
            "composition matched to canvas"
        }
    }

    /// 三段式中的完整 STYLE_HEADER（风格 DNA + 画幅声明）。
    pub fn style_header(&self) -> String {
        self.style_dna.replace("{aspect}", self.aspect_line())
    }

    /// 组装单场完整 prompt：`STYLE_HEADER + SCENE_BODY + MOTION_FOOTER`。
    pub fn build_prompt(&self, scene_body: &str) -> String {
        format!(
            "{}\n{}\n{}",
            self.style_header(),
            scene_body,
            self.motion_footer
        )
    }
}

/// 一场 = 一句旁白 + 一段视频（PLAN.md §3.4）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Scene {
    pub id: String,
    /// 分句原文。
    pub caption: String,
    /// 送入 TTS 的文本。
    pub narration: String,
    /// 最终三段式 prompt（M0 起由 pipeline 生成）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// 负向词。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub negative_prompt: Option<String>,
}

/// 项目数据源（单一 storyboard.json），PLAN.md §3.4。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Storyboard {
    pub title: String,
    pub lang: String,
    pub style: String,
    pub width: u32,
    pub height: u32,
    /// 成片画布 fps（组装用）。
    pub fps: u32,
    /// Agnes 视频帧率。
    pub frame_rate_video: u32,
    pub scenes: Vec<Scene>,
}

/// 按旁白时长（秒）计算 Agnes 视频帧数。
///
/// 规则：24fps，`num_frames = 8n + 1`，上限 441（≈18.3s）、下限 41（≈1.7s）。
/// 见 PLAN.md §4 与上游 pipeline.md 的换算公式。
#[allow(dead_code)] // M1 接入 ffprobe 实测时长后使用
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
        // 上游示例：4.82s 旁白 → 121 帧
        assert_eq!(num_frames_for_duration(4.82), 121);
        assert_eq!(num_frames_for_duration(5.0), 121);
        assert_eq!(num_frames_for_duration(10.0), 241);
        // 上下限
        assert_eq!(num_frames_for_duration(1.0), 41);
        assert_eq!(num_frames_for_duration(18.5), 441);

        // 全区间抽查：必须满足 8n+1 且在 [41, 441]
        for dur in [1.7, 3.0, 7.3, 12.9, 18.3] {
            let n = num_frames_for_duration(dur);
            assert!((41..=441).contains(&n), "帧数越界: {n}");
            assert_eq!((n - 1) % 8, 0, "帧数 {n} 不满足 8n+1");
        }
    }

    #[test]
    fn lang_parses_zh_and_en() {
        assert_eq!("zh".parse::<Lang>().unwrap(), Lang::Zh);
        assert_eq!("CN".parse::<Lang>().unwrap(), Lang::Zh);
        assert_eq!("en".parse::<Lang>().unwrap(), Lang::En);
        assert!("ja".parse::<Lang>().is_err());
    }
}
