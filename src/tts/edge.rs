//! Rust 原生 edge-tts 后端（`kothok-edge-tts`）。
//!
//! 每次 `synthesize` 调用都会：生成 DRM 鉴权 token → 打开到
//! `speech.platform.bing.com` 的 WebSocket → 发送 SSML → 流式接收 MP3 帧。
//! 使用前需调用一次 `kothok_edge_tts::init_tls()`（幂等）。

use std::path::Path;

use kothok_edge_tts::{EdgeTts, Engine, TtsEvent};

use super::{TtsError, TtsProvider};

/// kothok-edge-tts 后端。无状态：每次合成新建 WebSocket 连接。
pub struct EdgeTtsProvider;

impl TtsProvider for EdgeTtsProvider {
    async fn synthesize(
        &self,
        text: &str,
        voice: &str,
        rate: &str,
        lang: &str,
        out: &Path,
    ) -> Result<(), TtsError> {
        let events = EdgeTts
            .synthesize(text, voice, rate, lang)
            .await
            .map_err(|e| TtsError::Backend(e.to_string()))?;

        // 输出为 audio-24khz-48kbitrate-mono-mp3 帧，拼成完整 mp3。
        let mut audio = Vec::new();
        for event in events {
            if let TtsEvent::Audio(bytes) = event {
                audio.extend_from_slice(&bytes);
            }
        }
        if audio.is_empty() {
            return Err(TtsError::EmptyAudio(text.to_string()));
        }

        std::fs::write(out, &audio).map_err(TtsError::Io)?;
        Ok(())
    }
}
