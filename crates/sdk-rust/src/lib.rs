//! Typed HTTP client; editing and task state stay on the authoritative host.
pub use opencut_api_contracts as api;
pub use opencut_contracts as project;
pub use opencut_job_contracts as jobs;
pub use opencut_media_contracts as media;

use api::{JobRecord, JobState, tools::Tool};
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
    JobFailed(Box<JobRecord>),
    JobCancelled(Box<JobRecord>),
    PublicJobFailed(Box<api::job_views::JobView>),
    PublicJobCancelled(Box<api::job_views::JobView>),
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
            Self::JobFailed(job) => write!(f, "Job {} failed: {:?}", job.job_id, job.error),
            Self::JobCancelled(job) => write!(f, "Job {} was cancelled", job.job_id),
            Self::PublicJobFailed(job) => write!(f, "Job {} failed: {:?}", job.job_id, job.failure),
            Self::PublicJobCancelled(job) => write!(f, "Job {} was cancelled", job.job_id),
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
            || bytes.len() > 4_194_304
            || offset
                .checked_add(bytes.len() as u64)
                .is_none_or(|end| end > 1_073_741_824)
        {
            return Err(Error::Configuration(
                "Invalid upload identity or chunk range".into(),
            ));
        }
        let hash = format!("{:x}", Sha256::digest(bytes));
        let mut response = self
            .agent
            .put(format!(
                "{}/v1/uploads/{upload_id}/chunks/{offset}?size_bytes={}&content_hash={hash}",
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
        let bytes = serde_json::to_vec(input).map_err(Error::Serialization)?;
        let mut response = self
            .agent
            .post(format!("{}/v1/tools/{}", self.base_url, T::NAME))
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

    /// Durable cancellation is always explicit; stopping wait_for_job does not call this.
    pub fn cancel_job(&self, job_id: &str) -> Result<JobRecord, Error> {
        self.call::<api::tools::JobCancel>(&jobs::JobInput {
            job_id: job_id.into(),
        })
    }
    /// Cancellation is checked between blocking HTTP calls (bounded by request_timeout)
    /// and at most every 25 ms while sleeping. Timeout covers requests and polling.
    pub fn wait_for_job(&self, job_id: &str, options: WaitOptions<'_>) -> Result<JobRecord, Error> {
        self.wait_for::<api::tools::JobGet>(job_id, options)
    }
    pub fn wait_for_public_job(
        &self,
        job_id: &str,
        options: WaitOptions<'_>,
    ) -> Result<api::job_views::JobView, Error> {
        self.wait_for::<api::tools::JobView>(job_id, options)
    }
    pub fn cancel_public_job(&self, job_id: &str) -> Result<api::job_views::JobView, Error> {
        self.call::<api::tools::JobRequestCancel>(&jobs::JobInput {
            job_id: job_id.into(),
        })
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
        Ok(format!("{}/v1/artifacts/{id}", self.base_url))
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
impl ObservedJob for JobRecord {
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
impl ObservedJob for api::job_views::JobView {
    fn state(&self) -> JobState {
        self.state
    }
    fn failed(self) -> Error {
        Error::PublicJobFailed(Box::new(self))
    }
    fn cancelled(self) -> Error {
        Error::PublicJobCancelled(Box::new(self))
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

// Require the envelope fields even though legacy schema permits optional Option fields.
// Decode the outer shape independently of Output so API failures never become DTO failures.
#[derive(Deserialize)]
struct WireEnvelope {
    api_version: String,
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
    if envelope.api_version != "1" {
        return Err(invalid("Unsupported API version".into()));
    }
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

/// Read a concrete result without confusing a historical success with current artifact availability.
pub fn job_result<K: jobs::JobKind>(job: &JobRecord) -> Result<K::Result, Error> {
    if job.state != JobState::Succeeded || job.kind != K::NAME {
        return Err(Error::Protocol {
            status: 200,
            message: "Job is unfinished or has a different kind".into(),
        });
    }
    serde_json::from_value(job.result.clone().unwrap_or(serde_json::Value::Null)).map_err(|e| {
        Error::Protocol {
            status: 200,
            message: e.to_string(),
        }
    })
}

/// Decode a version 1 extension result (for example MediaImport) from the portable public job view.
pub fn extended_job_result<K: jobs::JobKind>(
    job: &api::job_views::JobView,
) -> Result<K::Result, Error> {
    let invalid = || Error::Protocol {
        status: 200,
        message: "Expected an available successful version 1 extension result of this kind".into(),
    };
    if job.state != JobState::Succeeded
        || job.kind != K::NAME
        || job.result.is_some()
        || job.result_unavailable.is_some()
    {
        return Err(invalid());
    }
    let result = job.extended_result.as_ref().ok_or_else(invalid)?;
    if result.kind != K::NAME || result.schema_version != 1 {
        return Err(invalid());
    }
    serde_json::from_value(result.data.clone()).map_err(|e| Error::Protocol {
        status: 200,
        message: e.to_string(),
    })
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
            format!("http://localhost:8788/v1/projects/team%2Fproject/media/{id}/content")
        );
        resource.asset.uri = "/private/secret".into();
        assert!(matches!(
            client.project_media_url(&resource),
            Err(Error::Configuration(_))
        ));
    }
}
