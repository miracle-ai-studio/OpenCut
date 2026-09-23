use std::{fmt, io};

#[derive(Debug)]
pub enum MediaError {
    InvalidRequest(String),
    Unsupported(String),
    NotFound(String),
    OutputExists(String),
    Cancelled,
    Admission {
        code: String,
        message: String,
    },
    OutputLimit {
        limit_bytes: u64,
    },
    ToolUnavailable {
        tool: String,
        detail: String,
    },
    ProcessFailed {
        tool: String,
        status: Option<i32>,
        stderr: String,
    },
    InvalidToolOutput {
        tool: String,
        detail: String,
    },
    VerificationFailed(String),
    Io(io::Error),
}

pub type MediaResult<T> = Result<T, MediaError>;

impl MediaError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Admission { code, .. } => match code.as_str() {
                "STORAGE_BUDGET_EXCEEDED" => "STORAGE_BUDGET_EXCEEDED",
                "STORAGE_CAPACITY_LOW" => "STORAGE_CAPACITY_LOW",
                "STORAGE_BUDGET_BUSY" => "STORAGE_BUDGET_BUSY",
                "INVALID_ARGUMENT" => "INVALID_ARGUMENT",
                _ => "STORAGE_BUDGET_ERROR",
            },
            Self::OutputLimit { .. } => "MEDIA_OUTPUT_LIMIT_EXCEEDED",
            Self::InvalidRequest(_) => "INVALID_ARGUMENT",
            Self::Unsupported(_) | Self::ToolUnavailable { .. } => "CAPABILITY_UNAVAILABLE",
            Self::NotFound(_) => "MEDIA_NOT_FOUND",
            Self::OutputExists(_) => "MEDIA_OUTPUT_EXISTS",
            Self::Cancelled => "JOB_CANCELLED",
            Self::ProcessFailed { .. } => "MEDIA_PROCESS_FAILED",
            Self::InvalidToolOutput { .. } => "MEDIA_INVALID_TOOL_OUTPUT",
            Self::VerificationFailed(_) => "ARTIFACT_VERIFICATION_FAILED",
            Self::Io(_) => "MEDIA_IO_ERROR",
        }
    }
}

impl fmt::Display for MediaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Admission { message, .. } => f.write_str(message),
            Self::OutputLimit { limit_bytes } => write!(
                f,
                "Media output reached an admitted or inherited file-size limit (reserved ceiling: {limit_bytes} bytes); no artifact was published"
            ),
            Self::InvalidRequest(v) => write!(f, "invalid media request: {v}"),
            Self::Unsupported(v) => write!(f, "unsupported media capability: {v}"),
            Self::NotFound(v) => write!(f, "media input not found: {v}"),
            Self::OutputExists(v) => write!(f, "output already exists: {v}"),
            Self::Cancelled => f.write_str("media operation cancelled"),
            Self::ToolUnavailable { tool, detail } => write!(f, "{tool} unavailable: {detail}"),
            Self::ProcessFailed {
                tool,
                status,
                stderr,
            } => {
                write!(f, "{tool} failed with status {status:?}: {stderr}")
            }
            Self::InvalidToolOutput { tool, detail } => {
                write!(f, "invalid {tool} output: {detail}")
            }
            Self::VerificationFailed(v) => write!(f, "artifact verification failed: {v}"),
            Self::Io(error) => write!(f, "media I/O error: {error}"),
        }
    }
}

impl std::error::Error for MediaError {}

impl From<io::Error> for MediaError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
