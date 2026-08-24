//! TTS 模块：旁白合成。
//!
//! 默认后端为 Rust 原生 edge-tts（`kothok-edge-tts`，复刻微软 Edge「朗读」WebSocket
//! 协议，免费、无需 API key）。通过 [`TtsProvider`] trait 抽象，可替换实现
//! （如 CLI 兜底）。选型结论见 PLAN.md M1 备注。

pub mod edge;

use std::fmt;
use std::path::Path;

use crate::models::Lang;

/// 中文默认女声。
pub const VOICE_ZH: &str = "zh-CN-XiaoyiNeural";
/// 中文默认男声。
pub const VOICE_ZH_MALE: &str = "zh-CN-YunxiNeural";
/// 英文默认女声。
pub const VOICE_EN: &str = "en-US-JennyNeural";
/// 英文默认男声。
pub const VOICE_EN_MALE: &str = "en-US-GuyNeural";

/// 按语言与性别取默认音色。
pub fn default_voice_with_gender(lang: Lang, male: bool) -> &'static str {
    match (lang, male) {
        (Lang::Zh, false) => VOICE_ZH,
        (Lang::Zh, true) => VOICE_ZH_MALE,
        (Lang::En, false) => VOICE_EN,
        (Lang::En, true) => VOICE_EN_MALE,
    }
}

/// 按语言取 BCP-47 语言标签（edge-tts 协议需要）。
pub fn lang_tag(lang: Lang) -> &'static str {
    match lang {
        Lang::Zh => "zh-CN",
        Lang::En => "en-US",
    }
}

/// 语速（1.0 = 正常）→ SSML prosody rate（1.2 → "+20%"，0.8 → "-20%"）。
pub fn rate_from_speed(speed: f64) -> String {
    format!("{:+}%", ((speed - 1.0) * 100.0).round() as i32)
}

/// TTS 后端抽象：合成一段文本为 mp3 写入 `out`。
pub trait TtsProvider {
    /// `text` 合成文本；`voice` 音色 short-name；`rate` SSML 语速（如 `+0%`）；
    /// `lang` BCP-47 语言标签。
    async fn synthesize(
        &self,
        text: &str,
        voice: &str,
        rate: &str,
        lang: &str,
        out: &Path,
    ) -> Result<(), TtsError>;
}

/// TTS 错误。
#[derive(Debug)]
pub enum TtsError {
    /// 后端返回错误（如网络失败、鉴权过期）。
    Backend(String),
    /// 合成成功但没有返回音频数据。
    EmptyAudio(String),
    /// 写文件失败。
    Io(std::io::Error),
}

impl fmt::Display for TtsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TtsError::Backend(e) => write!(f, "edge-tts 后端错误: {e}"),
            TtsError::EmptyAudio(t) => write!(f, "合成结果为空音频（文本: {t}）"),
            TtsError::Io(e) => write!(f, "IO 错误: {e}"),
        }
    }
}

impl std::error::Error for TtsError {}

/// 带重试的合成：网络抖动（连接重置等瞬时错误）时最多重试 `max_attempts` 次，
/// 间隔 `backoff_secs` 秒。已写出的 mp3 不会重写（写入只发生在成功后）。
pub async fn synthesize_with_retry<P: TtsProvider>(
    provider: &P,
    text: &str,
    voice: &str,
    rate: &str,
    lang: &str,
    out: &Path,
    max_attempts: u32,
    backoff_secs: u64,
) -> Result<(), TtsError> {
    let mut last_err = None;
    for attempt in 1..=max_attempts {
        match provider.synthesize(text, voice, rate, lang, out).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = Some(e);
                if attempt < max_attempts {
                    tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
                }
            }
        }
    }
    Err(last_err.unwrap_or(TtsError::Backend("未知错误".to_string())))
}

/// 重新导出后端实现。
pub use edge::EdgeTtsProvider;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_from_speed_maps_to_ssml() {
        assert_eq!(rate_from_speed(1.0), "+0%");
        assert_eq!(rate_from_speed(1.2), "+20%");
        assert_eq!(rate_from_speed(0.8), "-20%");
    }

    #[test]
    fn default_voices_by_lang_and_gender() {
        assert_eq!(default_voice_with_gender(Lang::Zh, false), "zh-CN-XiaoyiNeural");
        assert_eq!(default_voice_with_gender(Lang::Zh, true), "zh-CN-YunxiNeural");
        assert_eq!(default_voice_with_gender(Lang::En, false), "en-US-JennyNeural");
        assert_eq!(default_voice_with_gender(Lang::En, true), "en-US-GuyNeural");
        assert_eq!(lang_tag(Lang::Zh), "zh-CN");
        assert_eq!(lang_tag(Lang::En), "en-US");
    }
}
