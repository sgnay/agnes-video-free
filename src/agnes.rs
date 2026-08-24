//! Agnes Video V2.0 API 客户端。
//!
//! 默认网关：`https://apihub.agnes-ai.com`。
//! - 创建：`POST /v1/videos`
//! - 查询：`GET /agnesapi?video_id=...`
//!
//! 依据官方文档：视频 API 是异步任务；`num_frames` 必须 ≤ 441 且满足 `8n+1`。

use std::path::Path;
use std::time::{Duration, Instant};

use reqwest::{Response, StatusCode, Url};
use serde::{Deserialize, Serialize};

/// Agnes 国际站 API 根地址。
pub const DEFAULT_BASE_URL: &str = "https://apihub.agnes-ai.com";
/// Agnes Video V2.0 模型名。
pub const MODEL: &str = "agnes-video-v2.0";
/// 防止错误响应/空文件伪装成视频的最小大小。
pub const MIN_VIDEO_BYTES: usize = 20 * 1024;

/// Agnes 客户端配置。
#[derive(Debug, Clone)]
pub struct AgnesOptions {
    pub poll_interval: Duration,
    pub poll_timeout: Duration,
    /// 每个请求遇到 429 或 5xx 时的最大重试次数（不含首次请求）。
    pub max_retries: u32,
    /// 免费 key 默认 1 req/min；可在测试或付费额度下覆盖。
    pub retry_429_delay: Duration,
    pub retry_5xx_delay: Duration,
}

impl Default for AgnesOptions {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(8),
            poll_timeout: Duration::from_secs(900),
            max_retries: 4,
            retry_429_delay: Duration::from_secs(65),
            retry_5xx_delay: Duration::from_secs(5),
        }
    }
}

/// Agnes Video API 客户端。
pub struct AgnesClient {
    http: reqwest::Client,
    api_key: String,
    base_url: Url,
    options: AgnesOptions,
}

/// 创建任务请求体。
#[derive(Debug, Clone, Serialize)]
pub struct CreateVideoRequest {
    pub model: String,
    pub prompt: String,
    pub negative_prompt: String,
    pub width: u32,
    pub height: u32,
    pub num_frames: u32,
    pub frame_rate: u32,
}

impl CreateVideoRequest {
    pub fn new(
        prompt: impl Into<String>,
        negative_prompt: impl Into<String>,
        width: u32,
        height: u32,
        num_frames: u32,
        frame_rate: u32,
    ) -> Self {
        Self {
            model: MODEL.to_string(),
            prompt: prompt.into(),
            negative_prompt: negative_prompt.into(),
            width,
            height,
            num_frames,
            frame_rate,
        }
    }
}

/// 创建/查询响应中可能出现的元数据。
#[derive(Debug, Clone, Deserialize, Default)]
pub struct VideoMetadata {
    pub url: Option<String>,
}

/// Agnes 原始响应。
#[derive(Debug, Clone, Deserialize)]
pub struct VideoResponse {
    pub id: Option<String>,
    pub task_id: Option<String>,
    pub video_id: Option<String>,
    pub status: Option<String>,
    pub metadata: Option<VideoMetadata>,
    /// 兼容旧网关把 URL 放在顶层的实际返回格式。
    pub url: Option<String>,
    #[serde(default)]
    pub error: Option<serde_json::Value>,
}

impl VideoResponse {
    /// 新版推荐使用 video_id；依次兼容 task_id 和 id。
    pub fn video_id(&self) -> Option<&str> {
        self.video_id
            .as_deref()
            .or(self.task_id.as_deref())
            .or(self.id.as_deref())
    }

    /// 兼容官方文档的 metadata.url 与旧实现的顶层 url。
    pub fn result_url(&self) -> Option<&str> {
        self.metadata
            .as_ref()
            .and_then(|m| m.url.as_deref())
            .or(self.url.as_deref())
    }
}

/// 已创建的任务。
#[derive(Debug, Clone)]
pub struct VideoTask {
    pub video_id: String,
}

/// 已完成视频的结果。
#[derive(Debug, Clone)]
pub struct VideoResult {
    pub url: String,
}

/// Agnes API 错误。
#[derive(Debug)]
pub enum AgnesError {
    InvalidBaseUrl(String),
    ClientBuild(String),
    Request(reqwest::Error),
    Http { status: StatusCode, body: String },
    Decode(String),
    MissingVideoId,
    FailedTask(String),
    MissingResultUrl,
    Timeout { video_id: String },
    Io(std::io::Error),
    VideoTooSmall { path: String, bytes: usize },
}

impl std::fmt::Display for AgnesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidBaseUrl(e) => write!(f, "Agnes API 地址无效: {e}"),
            Self::ClientBuild(e) => write!(f, "创建 HTTP 客户端失败: {e}"),
            Self::Request(e) => write!(f, "Agnes API 网络错误: {e}"),
            Self::Http { status, body } => write!(f, "Agnes API HTTP {status}: {body}"),
            Self::Decode(e) => write!(f, "Agnes API 响应解析失败: {e}"),
            Self::MissingVideoId => write!(f, "Agnes 创建任务响应缺少 video_id/task_id/id"),
            Self::FailedTask(e) => write!(f, "Agnes 视频任务失败: {e}"),
            Self::MissingResultUrl => write!(f, "Agnes 完成响应缺少 metadata.url 或顶层 url"),
            Self::Timeout { video_id } => write!(f, "Agnes 视频任务轮询超时: {video_id}"),
            Self::Io(e) => write!(f, "视频文件写入失败: {e}"),
            Self::VideoTooSmall { path, bytes } => {
                write!(
                    f,
                    "下载的视频过小: {path}（{bytes} bytes，至少需要 {MIN_VIDEO_BYTES}）"
                )
            }
        }
    }
}

impl std::error::Error for AgnesError {}

impl AgnesClient {
    /// 使用自定义 API 地址和参数创建客户端（测试与自托管代理使用）。
    pub fn with_options(
        api_key: impl Into<String>,
        base_url: &str,
        options: AgnesOptions,
    ) -> Result<Self, AgnesError> {
        let base_url = format!("{}/", base_url.trim_end_matches('/'));
        let base_url =
            Url::parse(&base_url).map_err(|e| AgnesError::InvalidBaseUrl(e.to_string()))?;
        let http = reqwest::Client::builder()
            .build()
            .map_err(|e| AgnesError::ClientBuild(e.to_string()))?;
        Ok(Self {
            http,
            api_key: api_key.into(),
            base_url,
            options,
        })
    }

    fn url(&self, path: &str) -> Result<Url, AgnesError> {
        self.base_url
            .join(path)
            .map_err(|e| AgnesError::InvalidBaseUrl(e.to_string()))
    }

    /// 创建一个 Agnes Video V2.0 异步任务。
    pub async fn create_video(
        &self,
        request: &CreateVideoRequest,
    ) -> Result<VideoTask, AgnesError> {
        let url = self.url("v1/videos")?;
        let response = self
            .send_with_retry(|| {
                self.http
                    .post(url.clone())
                    .bearer_auth(&self.api_key)
                    .json(request)
            })
            .await?;
        let body = response
            .json::<VideoResponse>()
            .await
            .map_err(|e| AgnesError::Decode(e.to_string()))?;
        let video_id = body
            .video_id()
            .ok_or(AgnesError::MissingVideoId)?
            .to_string();
        Ok(VideoTask { video_id })
    }

    /// 轮询任务直到 completed / failed / 超时；pending 等排队状态会继续等待。
    pub async fn wait_for_video(&self, video_id: &str) -> Result<VideoResult, AgnesError> {
        let started = Instant::now();
        loop {
            if started.elapsed() >= self.options.poll_timeout {
                return Err(AgnesError::Timeout {
                    video_id: video_id.to_string(),
                });
            }
            let response = self.get_video(video_id).await?;
            let status = response
                .status
                .as_deref()
                .unwrap_or("unknown")
                .to_ascii_lowercase();
            match status.as_str() {
                "completed" => {
                    let url = response
                        .result_url()
                        .ok_or(AgnesError::MissingResultUrl)?
                        .to_string();
                    return Ok(VideoResult { url });
                }
                "failed" => {
                    let reason = response
                        .error
                        .map(|e| e.to_string())
                        .unwrap_or_else(|| "未提供错误详情".to_string());
                    return Err(AgnesError::FailedTask(reason));
                }
                status if Self::is_processing_status(status) => {
                    tokio::time::sleep(self.options.poll_interval).await;
                }
                other => {
                    let detail = response
                        .error
                        .map(|error| format!("，error={error}"))
                        .unwrap_or_default();
                    return Err(AgnesError::FailedTask(format!(
                        "未知任务状态「{other}」（video_id={video_id}{detail}）"
                    )));
                }
            }
        }
    }

    /// 判断任务是否仍在排队或处理中。
    fn is_processing_status(status: &str) -> bool {
        matches!(
            status,
            "pending" | "submitted" | "queued" | "running" | "in_progress" | "processing"
        )
    }

    /// 查询一次任务状态（内部也公开，便于 status 命令与测试）。
    pub async fn get_video(&self, video_id: &str) -> Result<VideoResponse, AgnesError> {
        let mut url = self.url("agnesapi")?;
        url.query_pairs_mut().append_pair("video_id", video_id);
        let response = self
            .send_with_retry(|| self.http.get(url.clone()).bearer_auth(&self.api_key))
            .await?;
        response
            .json::<VideoResponse>()
            .await
            .map_err(|e| AgnesError::Decode(e.to_string()))
    }

    /// 下载完成视频到目标路径，并以临时文件+rename 方式避免留下半文件。
    pub async fn download_video(&self, url: &str, out: &Path) -> Result<(), AgnesError> {
        let parsed = Url::parse(url).map_err(|e| AgnesError::InvalidBaseUrl(e.to_string()))?;
        // metadata.url 通常是无需鉴权的 CDN 地址；不要把 Agnes API key 转发给第三方 CDN。
        let response = self
            .send_with_retry(|| self.http.get(parsed.clone()))
            .await?;
        let bytes = response.bytes().await.map_err(AgnesError::Request)?;
        if bytes.len() < MIN_VIDEO_BYTES {
            return Err(AgnesError::VideoTooSmall {
                path: out.display().to_string(),
                bytes: bytes.len(),
            });
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(AgnesError::Io)?;
        }
        let tmp = out.with_extension("mp4.part");
        std::fs::write(&tmp, &bytes).map_err(AgnesError::Io)?;
        std::fs::rename(&tmp, out).map_err(AgnesError::Io)?;
        Ok(())
    }

    async fn send_with_retry<F>(&self, mut build: F) -> Result<Response, AgnesError>
    where
        F: FnMut() -> reqwest::RequestBuilder,
    {
        for attempt in 0..=self.options.max_retries {
            let response = build().send().await.map_err(AgnesError::Request)?;
            let status = response.status();
            if status.is_success() {
                return Ok(response);
            }
            let retryable = status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error();
            if retryable && attempt < self.options.max_retries {
                let delay = if status == StatusCode::TOO_MANY_REQUESTS {
                    self.options.retry_429_delay
                } else {
                    self.options
                        .retry_5xx_delay
                        .saturating_mul(2_u32.saturating_pow(attempt))
                };
                tokio::time::sleep(delay).await;
                continue;
            }
            let body = response.text().await.unwrap_or_default();
            return Err(AgnesError::Http { status, body });
        }
        unreachable!("retry loop always returns")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_uses_expected_model_and_parameters() {
        let req = CreateVideoRequest::new("prompt", "negative", 720, 1280, 121, 24);
        let value = serde_json::to_value(req).unwrap();
        assert_eq!(value["model"], MODEL);
        assert_eq!(value["num_frames"], 121);
        assert_eq!(value["frame_rate"], 24);
    }

    #[test]
    fn response_accepts_metadata_and_top_level_urls() {
        let metadata: VideoResponse = serde_json::from_value(serde_json::json!({
            "video_id": "video-1",
            "status": "completed",
            "metadata": {"url": "https://cdn/video-1.mp4"}
        }))
        .unwrap();
        assert_eq!(metadata.video_id(), Some("video-1"));
        assert_eq!(metadata.result_url(), Some("https://cdn/video-1.mp4"));

        let top_level: VideoResponse = serde_json::from_value(serde_json::json!({
            "id": "task-2",
            "status": "completed",
            "url": "https://cdn/task-2.mp4"
        }))
        .unwrap();
        assert_eq!(top_level.video_id(), Some("task-2"));
        assert_eq!(top_level.result_url(), Some("https://cdn/task-2.mp4"));
    }

    #[test]
    fn pending_and_running_statuses_are_pollable() {
        for status in [
            "pending",
            "submitted",
            "queued",
            "running",
            "in_progress",
            "processing",
        ] {
            assert!(
                AgnesClient::is_processing_status(status),
                "状态未识别: {status}"
            );
        }
        assert!(!AgnesClient::is_processing_status("failed"));
        assert!(!AgnesClient::is_processing_status("completed"));
    }

    #[test]
    fn options_have_documented_defaults() {
        let options = AgnesOptions::default();
        assert_eq!(options.poll_interval, Duration::from_secs(8));
        assert_eq!(options.poll_timeout, Duration::from_secs(900));
        assert_eq!(options.retry_429_delay, Duration::from_secs(65));
    }

    #[tokio::test]
    async fn pending_status_is_polled_until_completed() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use wiremock::matchers::{method, path, query_param};
        use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

        struct PendingThenCompleted {
            calls: Arc<AtomicUsize>,
        }

        impl Respond for PendingThenCompleted {
            fn respond(&self, _request: &Request) -> ResponseTemplate {
                let call = self.calls.fetch_add(1, Ordering::SeqCst);
                if call == 0 {
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "video_id": "video-pending",
                        "status": "pending"
                    }))
                } else {
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "video_id": "video-pending",
                        "status": "completed",
                        "metadata": {"url": "https://cdn.example/video-pending.mp4"}
                    }))
                }
            }
        }

        let server = MockServer::start().await;
        let calls = Arc::new(AtomicUsize::new(0));
        Mock::given(method("GET"))
            .and(path("/agnesapi"))
            .and(query_param("video_id", "video-pending"))
            .respond_with(PendingThenCompleted {
                calls: Arc::clone(&calls),
            })
            .expect(2)
            .mount(&server)
            .await;

        let client = AgnesClient::with_options(
            "test-key",
            &server.uri(),
            AgnesOptions {
                poll_interval: Duration::from_millis(1),
                poll_timeout: Duration::from_secs(2),
                ..AgnesOptions::default()
            },
        )
        .unwrap();
        let result = client.wait_for_video("video-pending").await.unwrap();
        assert_eq!(result.url, "https://cdn.example/video-pending.mp4");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn create_poll_and_download_use_documented_api_flow() {
        use wiremock::matchers::{header, method, path, query_param};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        let video_bytes = vec![0x4d; MIN_VIDEO_BYTES];
        Mock::given(method("POST"))
            .and(path("/v1/videos"))
            .and(header("authorization", "Bearer test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "task-123",
                "task_id": "task-123",
                "video_id": "video-123",
                "status": "queued"
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/agnesapi"))
            .and(query_param("video_id", "video-123"))
            .and(header("authorization", "Bearer test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "video_id": "video-123",
                "status": "completed",
                "metadata": {"url": format!("{}/files/video.mp4", server.uri())}
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/files/video.mp4"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(video_bytes.clone()))
            .expect(1)
            .mount(&server)
            .await;

        let client = AgnesClient::with_options(
            "test-key",
            &server.uri(),
            AgnesOptions {
                poll_interval: Duration::from_millis(1),
                poll_timeout: Duration::from_secs(2),
                ..AgnesOptions::default()
            },
        )
        .unwrap();
        let request = CreateVideoRequest::new("prompt", "negative", 720, 1280, 121, 24);
        let task = client.create_video(&request).await.unwrap();
        assert_eq!(task.video_id, "video-123");
        let result = client.wait_for_video(&task.video_id).await.unwrap();
        assert!(result.url.ends_with("/files/video.mp4"));

        let out =
            std::env::temp_dir().join(format!("agnes-video-free-test-{}.mp4", std::process::id()));
        client.download_video(&result.url, &out).await.unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), video_bytes);
        std::fs::remove_file(out).unwrap();
    }
}
