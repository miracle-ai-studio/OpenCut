//! Public revision attribution, not a credential or a claim of human identity.
use crate::audit::CallSource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RevisionActorsRequest {
    pub project_id: String,
    pub revisions: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RevisionActor {
    pub principal_id: String,
    pub source: CallSource,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RevisionActors {
    pub project_id: String,
    pub actors: std::collections::BTreeMap<String, Option<RevisionActor>>,
}
