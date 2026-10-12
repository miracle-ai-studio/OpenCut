pub use opencut_api_contracts::visual_preview_v4::{
    ClassicVisualLayerPlan, ClassicVisualPlan, VisualDecodeAdmission, VisualDecodeRequest,
    VisualResource, VisualResourceKind,
};
use opencut_composition::project_v4::{
    BoundVisualFrameV4, DecodedVisualSourceV4, VisualFrameIdentityV4,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicVisualResourceHandle {
    pub asset_id: String,
    pub handle: String,
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
