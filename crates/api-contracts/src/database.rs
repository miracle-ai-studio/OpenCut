//! SQLite operational observations; these are not admission or durability receipts.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Report {
    pub sqlite_version: String,
    pub persistent: bool,
    pub journal_mode: String,
    pub page_size_bytes: u64,
    pub page_count: String,
    pub freelist_pages: String,
    pub max_page_count: String,
    pub logical_database_bytes: String,
    /// None means unavailable; for a persistent database an absent sidecar is "0".
    pub database_file_bytes: Option<String>,
    pub wal_file_bytes: Option<String>,
    pub shm_file_bytes: Option<String>,
    pub connection_busy_timeout_ms: u64,
    pub connection_wal_autocheckpoint_pages: u64,
    pub consistency: String,
    pub enforces_total_storage_limit: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointMode {
    Passive,
    Truncate,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CheckpointInput {
    pub mode: CheckpointMode,
    #[serde(default)]
    #[schemars(range(min = 0, max = 5000))]
    pub busy_timeout_ms: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointOutcome {
    Complete,
    Blocked,
    NotApplicable,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CheckpointReport {
    pub mode: CheckpointMode,
    pub outcome: CheckpointOutcome,
    pub log_frames: Option<u64>,
    pub checkpointed_frames: Option<u64>,
    /// File size is observed after the operation; concurrent writers may change it.
    pub observed_wal_file_bytes: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Capabilities {
    pub inspect_tool: String,
    pub checkpoint_tool: String,
    pub owner_only: bool,
    pub max_checkpoint_busy_timeout_ms: u64,
    pub enforces_total_storage_limit: bool,
}
