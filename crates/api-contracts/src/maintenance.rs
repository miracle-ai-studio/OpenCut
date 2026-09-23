//! Provenance of native file publication; never a claim of a distributed transaction.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MaintenanceObject {
    MediaContent { content_hash: String },
    MediaMetadata { content_hash: String },
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceAction {
    MediaContentPublish,
    MediaMetadataPublish,
    MediaMetadataRepair,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "PublicationIntent")]
pub struct PublicationIntent<O = MaintenanceObject, A = MaintenanceAction> {
    pub action: A,
    pub object: O,
    pub before_hash: Option<String>,
    pub after_hash: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PublicationPhase {
    Prepared,
    Completed,
    ObservedApplied,
    ObservedUnchanged,
    ObservedDiverged,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "MaintenanceEvent")]
pub struct MaintenanceEvent<O = MaintenanceObject, A = MaintenanceAction> {
    pub sequence: String,
    pub publication_id: String,
    pub phase: PublicationPhase,
    pub intent: PublicationIntent<O, A>,
    pub actor: crate::audit::Actor,
    pub request_id: String,
    pub origin_request_id: String,
    pub observed_hash: Option<String>,
    pub created_at: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[schemars(rename = "MaintenanceTarget")]
pub enum MaintenanceTarget<O = MaintenanceObject> {
    Workspace,
    Object { object: O },
    Publication { id: String },
    Request { id: String },
}
fn first() -> String {
    "0".into()
}
fn limit() -> usize {
    100
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "MaintenanceQuery")]
pub struct MaintenanceQuery<O = MaintenanceObject> {
    pub target: MaintenanceTarget<O>,
    #[serde(default = "first")]
    pub after_sequence: String,
    #[serde(default = "limit")]
    pub limit: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "MaintenancePage")]
pub struct MaintenancePage<O = MaintenanceObject, A = MaintenanceAction, C = MaintenanceCoverage> {
    pub events: Vec<MaintenanceEvent<O, A>>,
    pub last_sequence: String,
    pub has_more: bool,
    pub coverage: C,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceCoverage {
    RecordedPublicationsSinceSchema6,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "ReconcileInput")]
pub struct ReconcileInput<O = MaintenanceObject> {
    pub object: O,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "ReconcileResult")]
pub struct ReconcileResult<O = MaintenanceObject> {
    pub object: O,
    pub observed_hash: Option<String>,
    pub resolved_publication_ids: Vec<String>,
}
