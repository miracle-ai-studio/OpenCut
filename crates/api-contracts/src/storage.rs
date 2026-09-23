//! Bounded observation, not a quota admission decision or a garbage-collection plan.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const MAX_ENTRIES: usize = 100_000;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InspectInput {
    #[serde(default = "default_entries")]
    #[schemars(range(min = 1, max = MAX_ENTRIES))]
    pub max_entries: usize,
}
fn default_entries() -> usize {
    MAX_ENTRIES
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Database,
    AssetStore,
    Media,
    Uploads,
    Artifacts,
    Plugins,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Usage {
    pub category: Category,
    /// False also when the budget was exhausted before inspecting this category.
    pub present: bool,
    pub complete: bool,
    pub regular_files: usize,
    pub directories: usize,
    pub hardlink_aliases: usize,
    /// Unique regular-file lengths. Decimal strings avoid client integer truncation.
    pub logical_bytes: String,
    /// st_blocks * 512 for unique regular files and directories, not exclusive space.
    pub allocated_bytes: String,
    pub omissions: Omissions,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct Omissions {
    pub symlinks: usize,
    pub other_file_types: usize,
    pub other_devices: usize,
    pub io_errors: usize,
    pub depth_limited: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EntryLimit,
    TimeLimit,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UsageReport {
    pub schema_version: u32,
    pub consistency: String,
    pub complete: bool,
    pub visited_entries: usize,
    pub stop_reason: Option<StopReason>,
    pub categories: Vec<Usage>,
    pub logical_bytes: String,
    pub allocated_bytes: String,
    pub volume_available_bytes: Option<String>,
    pub volume_capacity_bytes: Option<String>,
    pub volume_observed: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Capabilities {
    pub inspect_tool: String,
    pub available: bool,
    pub owner_only: bool,
    pub max_entries: usize,
    pub max_depth: usize,
    pub cooperative_time_limit_ms: u64,
    pub max_concurrent_per_process: usize,
    pub enforces_quota: bool,
    pub garbage_collection: bool,
}
