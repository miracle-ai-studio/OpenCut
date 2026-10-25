//! Typed HTTP client; editing and task state stay on the authoritative host.
pub use opencut_api_contracts as api;
pub use opencut_contracts as project;
use opencut_job_contracts as jobs;
pub use opencut_media_contracts as media;

pub use api::job_results::{JobResultKind, kinds as job_kinds};
use api::job_views::JobView;
use api::{JobState, tools::Tool};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fmt,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[derive(Debug)]
pub struct ApiFailure {
    pub error: project::EditorError,
    pub status: u16,
    pub request_id: String,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub enum Error {
    Configuration(String),
    Serialization(serde_json::Error),
    /// The request may already have committed. No automatic retries are performed.
    Transport(Box<ureq::Error>),
    /// An unparseable response also leaves the mutation outcome unknown.
    Protocol {
        status: u16,
        message: String,
    },
    Api(Box<ApiFailure>),
    WaitTimeout {
        job_id: String,
    },
    ObservationCancelled,
    JobFailed(Box<JobView>),
    JobCancelled(Box<JobView>),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(message) => write!(f, "Invalid SDK configuration: {message}"),
            Self::Serialization(error) => write!(f, "Invalid request: {error}"),
            Self::Transport(error) => {
                write!(f, "Transport failed; submission outcome unknown: {error}")
            }
            Self::Protocol { status, message } => write!(
                f,
                "Invalid response ({status}): {message}; submission outcome unknown"
            ),
            Self::Api(failure) => write!(f, "{}: {}", failure.error.code, failure.error.message),
            Self::WaitTimeout { job_id } => {
                write!(f, "Stopped waiting for {job_id}; job was not cancelled")
            }
            Self::ObservationCancelled => {
                write!(f, "Caller stopped observing; job was not cancelled")
            }
            Self::JobFailed(job) => write!(f, "Job {} failed: {:?}", job.job_id, job.failure),
            Self::JobCancelled(job) => write!(f, "Job {} was cancelled", job.job_id),
        }
    }
}
impl std::error::Error for Error {}

#[derive(Debug)]
pub struct Success<T> {
    pub data: T,
    pub request_id: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Options {
    pub request_timeout: Duration,
    pub max_response_bytes: u64,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(10),
            max_response_bytes: 32 * 1024 * 1024,
        }
    }
}

/// Blocking client for native scripts. It owns no editor state and never refreshes revisions.
/// Tokens are deliberately excluded from Debug output.
pub struct Client {
    base_url: String,
    token: String,
    agent: ureq::Agent,
    options: Options,
}
impl Client {
    pub fn new(base_url: &str, token: impl Into<String>) -> Result<Self, Error> {
        Self::with_options(base_url, token, Options::default())
    }
    pub fn with_options(
        base_url: &str,
        token: impl Into<String>,
        options: Options,
    ) -> Result<Self, Error> {
        let url = url::Url::parse(base_url)
            .map_err(|_| Error::Configuration("Invalid base URL".into()))?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(Error::Configuration(
                "Use an HTTP(S) base URL without credentials, query or fragment".into(),
            ));
        }
        if options.request_timeout.is_zero() || options.max_response_bytes == 0 {
            return Err(Error::Configuration(
                "Timeout and response limit must be positive".into(),
            ));
        }
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_global(Some(options.request_timeout))
            .build()
            .new_agent();
        Ok(Self {
            base_url: url.as_str().trim_end_matches('/').into(),
            token: token.into(),
            agent,
            options,
        })
    }

    /// The registry binds each tool to its request type, including at external call sites.
    /// ```compile_fail
    /// use opencut_sdk::{Client, api::{EmptyInput, tools}};
    /// let client = Client::new("http://localhost:8788", "token").unwrap();
    /// client.call::<tools::EditApply>(&EmptyInput {});
    /// ```
    pub fn call<T: Tool>(&self, input: &T::Input) -> Result<T::Output, Error> {
        Ok(self.call_envelope::<T>(input)?.data)
    }
    /// Typed additive discovery without changing the frozen v1 capabilities tool schema.
    pub fn host_capabilities(&self) -> Result<api::capabilities::HostCapabilities, Error> {
        let resource = self.call::<api::tools::ResourceRead>(&api::context::ResourceReadInput {
            uri: "opencut://system/capabilities".into(),
        })?;
        match resource.payload {
            api::context::ResourcePayload::Capabilities(value) => Ok(*value),
            _ => Err(Error::Protocol {
                status: 200,
                message: "Expected the host capabilities resource".into(),
            }),
        }
    }
    pub fn call_envelope<T: Tool>(&self, input: &T::Input) -> Result<Success<T::Output>, Error> {
        self.call_with_timeout::<T>(input, self.options.request_timeout)
    }
    /// Full generated declaration through the dedicated discovery endpoint.
    /// This is not a bounded project-context resource.
    pub fn operation_schemas(&self) -> Result<Success<Vec<api::tools::ToolSpec>>, Error> {
        self.discovery("/api/operations")
    }
    /// All action summaries, or the exact input/output schemas of one domain.
    pub fn domain_catalog(
        &self,
        domain: Option<&str>,
    ) -> Result<Success<Vec<api::domains::DomainDescriptor>>, Error> {
        match domain {
            None => self.discovery("/api/domains"),
            Some(domain)
                if api::tools::ToolDomain::ALL
                    .iter()
                    .any(|d| d.name() == domain) =>
            {
                self.discovery(&format!("/api/domains/{domain}"))
            }
            Some(_) => Err(Error::Configuration("Unknown operation domain".into())),
        }
    }
    fn discovery<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<Success<T>, Error> {
        let mut response = self
            .agent
            .get(format!("{}{path}", self.base_url))
            .header("Authorization", &format!("Bearer {}", self.token))
            .header("Accept", "application/json")
            .call()
            .map_err(|e| Error::Transport(Box::new(e)))?;
        let status = response.status().as_u16();
        let audit_warning = response
            .headers()
            .get("x-opencut-audit-warning")
            .is_some_and(|v| v == "DENIAL_AUDIT_UNAVAILABLE");
        let body = response
            .body_mut()
            .with_config()
            .limit(self.options.max_response_bytes)
            .read_to_vec()
            .map_err(|e| Error::Transport(Box::new(e)))?;
        with_audit_warning(decode_response(status, &body), audit_warning)
    }
    /// Dynamic registered tools/domains for CLI dispatch. Typed callers should prefer `call`.
    pub fn call_registered(
        &self,
        name: &str,
        input: &serde_json::Value,
    ) -> Result<Success<serde_json::Value>, Error> {
        if api::tools::tool_spec(name).is_none() && !api::domains::is_domain_tool(name) {
            return Err(Error::Configuration("Unknown registered operation".into()));
        }
        self.call_operation(name, input, self.options.request_timeout)
    }
    /// Send a single raw chunk. No automatic retry: inspect the confirmed offset after an unknown outcome.
    pub fn upload_chunk(
        &self,
        upload_id: &str,
        offset: u64,
        bytes: &[u8],
    ) -> Result<api::uploads::UploadView, Error> {
        let valid_id = upload_id.strip_prefix("upload_").is_some_and(|v| {
            v.len() == 32
                && v.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        });
        if !valid_id
            || bytes.is_empty()
            || bytes.len() as u64 > api::uploads::MAX_CHUNK_BYTES
            || offset
                .checked_add(bytes.len() as u64)
                .is_none_or(|end| end > api::uploads::MAX_SOURCE_BYTES)
        {
            return Err(Error::Configuration(
                "Invalid upload identity or chunk range".into(),
            ));
        }
        let hash = format!("{:x}", Sha256::digest(bytes));
        let mut response = self
            .agent
            .put(format!(
                "{}/api/uploads/{upload_id}/chunks/{offset}?size_bytes={}&content_hash={hash}",
                self.base_url,
                bytes.len()
            ))
            .header("Authorization", &format!("Bearer {}", self.token))
            .header("Content-Type", "application/octet-stream")
            .header("Accept", "application/json")
            .send(bytes)
            .map_err(|e| Error::Transport(Box::new(e)))?;
        let status = response.status().as_u16();
        let audit_warning = response
            .headers()
            .get("x-opencut-audit-warning")
            .is_some_and(|v| v == "DENIAL_AUDIT_UNAVAILABLE");
        let body = response
            .body_mut()
            .with_config()
            .limit(self.options.max_response_bytes)
            .read_to_vec()
            .map_err(|e| Error::Transport(Box::new(e)))?;
        Ok(with_audit_warning(
            decode_response::<api::uploads::UploadView>(status, &body),
            audit_warning,
        )?
        .data)
    }
    fn call_with_timeout<T: Tool>(
        &self,
        input: &T::Input,
        timeout: Duration,
    ) -> Result<Success<T::Output>, Error> {
        self.call_operation(T::NAME, input, timeout)
    }
    fn call_operation<I: serde::Serialize, O: serde::de::DeserializeOwned>(
        &self,
        name: &str,
        input: &I,
        timeout: Duration,
    ) -> Result<Success<O>, Error> {
        let bytes = serde_json::to_vec(input).map_err(Error::Serialization)?;
        let mut response = self
            .agent
            .post(format!("{}/api/tools/{}", self.base_url, name))
            .header("Authorization", &format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .config()
            .timeout_global(Some(timeout))
            .build()
            .send(&bytes)
            .map_err(|e| Error::Transport(Box::new(e)))?;
        let status = response.status().as_u16();
        let audit_warning = response
            .headers()
            .get("x-opencut-audit-warning")
            .is_some_and(|v| v == "DENIAL_AUDIT_UNAVAILABLE");
        let body = response
            .body_mut()
            .with_config()
            .limit(self.options.max_response_bytes)
            .read_to_vec()
            .map_err(|e| Error::Transport(Box::new(e)))?;
        with_audit_warning(decode_response(status, &body), audit_warning)
    }

    /// Stopping observation never implicitly cancels the durable task.
    pub fn cancel_job(&self, job_id: &str) -> Result<JobView, Error> {
        self.call::<api::tools::JobRequestCancel>(&jobs::JobInput {
            job_id: job_id.into(),
        })
    }
    pub fn wait_for_job(&self, job_id: &str, options: WaitOptions<'_>) -> Result<JobView, Error> {
        self.wait_for::<api::tools::JobView>(job_id, options)
    }
    pub fn artifact_url(
        &self,
        reference: &api::job_views::ArtifactReference,
    ) -> Result<String, Error> {
        let id = format!("artifact:{}:{}", reference.job_id, reference.content_hash);
        let valid = reference.job_id.strip_prefix("job_").is_some_and(|v| {
            v.len() == 32
                && v.bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        }) && reference.content_hash.len() == 64
            && reference
                .content_hash
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase());
        if !valid || reference.artifact_id != id {
            return Err(Error::Configuration("Invalid artifact reference".into()));
        }
        Ok(format!("{}/api/artifacts/{id}", self.base_url))
    }
    pub fn project_media_url(
        &self,
        resource: &api::uploads::MediaImportJobResult,
    ) -> Result<String, Error> {
        let media_id = &resource.media_id;
        let valid_id = media_id.strip_prefix("media_").is_some_and(|value| {
            value.len() == 32
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        });
        if resource.project_id.is_empty()
            || !valid_id
            || resource.asset.uri != format!("opencut-media://{media_id}")
            || resource.asset.version.is_none()
        {
            return Err(Error::Configuration(
                "Invalid project media resource".into(),
            ));
        }
        let mut url = url::Url::parse(&self.base_url)
            .map_err(|_| Error::Configuration("Invalid base URL".into()))?;
        url.path_segments_mut()
            .map_err(|_| Error::Configuration("Invalid base URL".into()))?
            .extend([
                "v1",
                "projects",
                resource.project_id.as_str(),
                "media",
                media_id.as_str(),
                "content",
            ]);
        Ok(url.to_string())
    }
    fn wait_for<T: Tool<Input = jobs::JobInput>>(
        &self,
        job_id: &str,
        options: WaitOptions<'_>,
    ) -> Result<T::Output, Error>
    where
        T::Output: ObservedJob,
    {
        if options.poll_interval.is_zero() {
            return Err(Error::Configuration(
                "Polling interval must be positive".into(),
            ));
        }
        let start = Instant::now();
        let expired = || Error::WaitTimeout {
            job_id: job_id.into(),
        };
        loop {
            options.check_cancelled()?;
            let remaining = options
                .timeout
                .checked_sub(start.elapsed())
                .filter(|d| !d.is_zero())
                .ok_or_else(expired)?;
            let observed = self.call_with_timeout::<T>(
                &jobs::JobInput {
                    job_id: job_id.into(),
                },
                remaining.min(self.options.request_timeout),
            );
            options.check_cancelled()?;
            if start.elapsed() >= options.timeout {
                return Err(expired());
            }
            let job = observed?.data;
            match job.state() {
                JobState::Succeeded => return Ok(job),
                JobState::Failed => return Err(job.failed()),
                JobState::Cancelled => return Err(job.cancelled()),
                JobState::Queued | JobState::Running => {}
            }
            let pause = Instant::now();
            while pause.elapsed() < options.poll_interval {
                options.check_cancelled()?;
                let remaining = options
                    .timeout
                    .checked_sub(start.elapsed())
                    .filter(|d| !d.is_zero())
                    .ok_or_else(expired)?;
                std::thread::sleep(
                    remaining
                        .min(options.poll_interval.saturating_sub(pause.elapsed()))
                        .min(Duration::from_millis(25)),
                );
            }
        }
    }
}

trait ObservedJob {
    fn state(&self) -> JobState;
    fn failed(self) -> Error;
    fn cancelled(self) -> Error;
}
impl ObservedJob for api::job_views::JobView {
    fn state(&self) -> JobState {
        self.state
    }
    fn failed(self) -> Error {
        Error::JobFailed(Box::new(self))
    }
    fn cancelled(self) -> Error {
        Error::JobCancelled(Box::new(self))
    }
}

pub struct WaitOptions<'a> {
    pub timeout: Duration,
    pub poll_interval: Duration,
    pub cancelled: Option<&'a AtomicBool>,
}
impl Default for WaitOptions<'_> {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            poll_interval: Duration::from_millis(200),
            cancelled: None,
        }
    }
}
impl WaitOptions<'_> {
    fn check_cancelled(&self) -> Result<(), Error> {
        if self
            .cancelled
            .is_some_and(|flag| flag.load(Ordering::Acquire))
        {
            Err(Error::ObservationCancelled)
        } else {
            Ok(())
        }
    }
}

fn with_audit_warning<T>(
    result: Result<Success<T>, Error>,
    present: bool,
) -> Result<Success<T>, Error> {
    result.map_err(|mut error| {
        if let Error::Api(failure) = &mut error
            && present
            && !failure
                .warnings
                .iter()
                .any(|w| w == "DENIAL_AUDIT_UNAVAILABLE")
        {
            failure.warnings.push("DENIAL_AUDIT_UNAVAILABLE".into());
        }
        error
    })
}

// Require the envelope fields,including explicit data/error nulls.
// Decode the outer shape independently of Output so API failures never become DTO failures.
#[derive(Deserialize)]
struct WireEnvelope {
    request_id: String,
    data: serde_json::Value,
    error: serde_json::Value,
    warnings: Vec<String>,
}
fn decode_response<T: serde::de::DeserializeOwned>(
    status: u16,
    body: &[u8],
) -> Result<Success<T>, Error> {
    let invalid = |message: String| Error::Protocol { status, message };
    let envelope: WireEnvelope =
        serde_json::from_slice(body).map_err(|e| invalid(e.to_string()))?;
    if !envelope.error.is_null() {
        let error = serde_json::from_value(envelope.error).map_err(|e| invalid(e.to_string()))?;
        return Err(Error::Api(Box::new(ApiFailure {
            error,
            status,
            request_id: envelope.request_id,
            warnings: envelope.warnings,
        })));
    }
    if !(200..300).contains(&status) || envelope.data.is_null() {
        return Err(invalid("Inconsistent status or missing result".into()));
    }
    let data = serde_json::from_value(envelope.data).map_err(|e| invalid(e.to_string()))?;
    Ok(Success {
        data,
        request_id: envelope.request_id,
        warnings: envelope.warnings,
    })
}

/// Extract the typed result from the same shared union used by HTTP/MCP/CLI.
/// Native paths,stored generation selectors and duplicate extension channels
/// are not part of the SDK.
pub fn job_result<K: JobResultKind>(job: &JobView) -> Result<K::Result, Error> {
    let invalid = || Error::Protocol {
        status: 200,
        message: "Expected an available successful result of this kind".into(),
    };
    if job.state != JobState::Succeeded || job.kind != K::NAME || job.result_unavailable.is_some() {
        return Err(invalid());
    }
    K::result(job.result.as_ref().ok_or_else(invalid)?)
        .cloned()
        .ok_or_else(invalid)
}

#[cfg(test)]
mod project_media_url_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn resource_url_encodes_project_segment_and_rejects_forged_uri() {
        let client = Client::new("http://localhost:8788", "reader").unwrap();
        let id = format!("media_{}", "a".repeat(32));
        let mut resource: api::uploads::MediaImportJobResult = serde_json::from_value(json!({
            "media_id":id,"project_id":"team/project","upload_id":"upload_x","job_id":"job_x",
            "asset":{"id":"source","name":"source","kind":"audio",
                "uri":format!("opencut-media://{id}"),"duration":null,"metadata":{},
                "version":{"id":format!("av_{}", "b".repeat(64)),
                    "content_hash":"b".repeat(64),"size":3}}
        }))
        .unwrap();
        assert_eq!(
            client.project_media_url(&resource).unwrap(),
            format!("http://localhost:8788/api/projects/team%2Fproject/media/{id}/content")
        );
        resource.asset.uri = "/private/secret".into();
        assert!(matches!(
            client.project_media_url(&resource),
            Err(Error::Configuration(_))
        ));
    }
}

#[cfg(test)]
mod task_result_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn typed_transcription_preserves_source_and_requires_an_available_successful_result() {
        let mut view: JobView = serde_json::from_value(json!({
            "job_id":"job_example", "kind":"analysis.asset", "state":"succeeded",
            "target":null, "cancel_requested":false, "progress":null,
            "result":{"kind":"analysis.asset","data":{
                "project_id":"p", "revision":"r", "asset_id":"a",
                "asset_version":{"asset_id":"a", "content_hash":"a".repeat(64), "size":12},
                "options":{"kind":"transcribe","language":"zh","word_timestamps":false},
                "data":{"kind":"transcribe","result":{"language":"zh", "provider":"whisper.cpp", "model":"tiny",
                    "segments":[{"start":{"ticks":"0","timebase":1000},
                        "end":{"ticks":"1000","timebase":1000},"text":"你好","words":[]}]}}
            }}, "result_unavailable":null, "failure":null, "artifacts":[]
        })).unwrap();
        let result = job_result::<job_kinds::AssetAnalysis>(&view).unwrap();
        assert_eq!(result.asset_id, "a");
        let api::job_results::AssetAnalysisData::Transcribe(transcription) = result.data else {
            panic!("Wrong analysis result")
        };
        assert_eq!(transcription.segments[0].text, "你好");
        assert_eq!(transcription.segments[0].end.ticks, 1000);
        view.kind = "media.import".into();
        assert!(job_result::<job_kinds::AssetAnalysis>(&view).is_err());
        view.kind = "analysis.asset".into();
        view.state = JobState::Cancelled;
        assert!(job_result::<job_kinds::AssetAnalysis>(&view).is_err());
    }
    #[test]
    fn frame_decodes_portable_projection_without_worker_artifact_fields() {
        let mut view: JobView = serde_json::from_value(json!({
            "job_id":"job_frame", "kind":"preview.composited_frame", "state":"succeeded",
            "target":null,"cancel_requested":false,"progress":null,
            "result":{"kind":"preview.composited_frame","data":{
                "revision":"r","sequence_id":"main","requested_time":{"ticks":"0","timebase":1},
                "actual_sequence_time":{"ticks":"0","timebase":1},"alignment":"exact",
                "quality":"composited_project_frame","asset_lock":null,"asset_locking":"legacy_unlocked",
                "width":64,"height":48,"text_layouts":[]
            }}, "result_unavailable":null,"failure":null,"artifacts":[]
        })).unwrap();
        let frame = job_result::<job_kinds::CompositedFrame>(&view).unwrap();
        assert_eq!((frame.width, frame.height), (64, 48));
        assert!(job_result::<job_kinds::ProjectRender>(&view).is_err());
        view.result = None;
        assert!(job_result::<job_kinds::CompositedFrame>(&view).is_err());
    }
}
