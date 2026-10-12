use opencut_composition::project_v4::{
    BoundVisualFrameV4, DecodedVisualSourceV4, VisualFrameIdentityV4, VisualNoFrameReasonV4,
};
use opencut_contracts::Time;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualResourceKind {
    Video,
    Image,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VisualResource {
    pub asset_id: String,
    pub content_hash: String,
    pub size: String,
    pub kind: VisualResourceKind,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicVisualResourceHandle {
    pub asset_id: String,
    pub handle: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicVisualLayerPlan {
    pub element_id: String,
    pub resource: VisualResource,
    pub decode: VisualDecodeRequest,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum VisualDecodeRequest {
    Image,
    Video {
        #[serde(rename = "requestedTime")]
        requested_time: Time,
        /// Decoder API projection supplied by Rust; the browser never divides
        /// cross-process integer ticks or applies retime itself.
        #[serde(rename = "requestedTimeSeconds")]
        requested_time_seconds: f64,
        #[serde(rename = "pointProfile")]
        point_profile: String,
        #[serde(rename = "selectedVideoStream")]
        selected_video_stream: Option<u32>,
        #[serde(rename = "noFrameReason")]
        no_frame_reason: Option<VisualNoFrameReasonV4>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicVisualPlan {
    pub identity: VisualFrameIdentityV4,
    pub resources: Vec<VisualResource>,
    pub layers: Vec<ClassicVisualLayerPlan>,
}
#[derive(Debug, Deserialize)]
#[serde(tag = "op", content = "args", deny_unknown_fields)]
pub enum VisualCallback {
    #[serde(rename = "resolveVerifiedBytes")]
    Resolve(VisualResolveBytes),
    #[serde(rename = "readVerifiedBytes")]
    Read(VisualReadBytes),
    #[serde(rename = "releaseVerifiedBytes")]
    Release(VisualReleaseBytes),
    #[serde(rename = "admitDecode")]
    Admit(VisualAdmitDecode),
    #[serde(rename = "bindFrame")]
    Bind(VisualBindFrame),
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VisualResolveBytes {
    pub resource: VisualResource,
    pub handle: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VisualReadBytes {
    pub transfer_id: String,
    pub offset: String,
    pub count: u32,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VisualReleaseBytes {
    pub transfer_id: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VisualAdmitDecode {
    pub element_id: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VisualBindFrame {
    pub decoded: Vec<DecodedVisualSourceV4>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VisualDecodeAdmission {
    pub max_width: u32,
    pub max_height: u32,
    pub max_pixels: String,
    pub max_decoded_bytes: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicVisualRenderReceipt {
    pub identity: VisualFrameIdentityV4,
    pub engine_identity: String,
    pub bound: BoundVisualFrameV4,
    pub rgba_bytes: String,
    pub peak_accounted_bytes: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicVisualEngineIdentity {
    pub engine: String,
    pub browser_version: String,
    pub browser_sha256: String,
    pub module_sha256: String,
    pub shader_sha256: String,
    pub playwright_version: String,
    pub mediabunny_version: String,
    pub gpu_backend: String,
    pub gpu_device: String,
    pub texture_format: String,
    pub decoder_profile: String,
}
#[derive(Clone, Debug)]
pub struct ClassicVisualRuntimeReceipt {
    pub browser_pid: u32,
    pub identity: ClassicVisualEngineIdentity,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VisualQuotaReceipt {
    pub profile: String,
    pub max_rss_bytes: String,
    pub observed_peak_rss_bytes: String,
    pub monitor_interval_ms: u32,
    pub elapsed_ms: u64,
    pub hard_memory_enforced: bool,
}
#[derive(Debug)]
pub struct ClassicVisualExecutionReceipt {
    pub receipt: ClassicVisualRenderReceipt,
    pub quota: VisualQuotaReceipt,
    pub runtime_identity: ClassicVisualRuntimeReceipt,
}
