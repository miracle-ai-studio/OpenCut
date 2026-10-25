//! Exact fixed-head write recovery. Host-authenticated scope owns the record;
//! neither its locator nor a project revision grants permission to replay it.
use crate::{
    TransactionResult,
    project_history::{HistoryCommit, RedoRequest, UndoRequest},
    project_public::ApplyProject,
    scene_ops::RestoreProject,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditRecoveryIntent {
    PublicApply {
        request: Box<ApplyProject>,
    },
    Undo {
        request: UndoRequest,
    },
    Redo {
        request: RedoRequest,
    },
    Restore {
        request: RestoreProject,
    },
    SubtitleGenerate {
        request: crate::subtitles::GenerateCaptions,
    },
    SubtitleImport {
        request: crate::subtitles::SubtitleImport,
    },
    FontImport {
        request: crate::font_import::ImportFont,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditRecoveryContext {
    pub intent: EditRecoveryIntent,
    /// Optional UI location only. History and restore remain project-scoped;
    /// this field does not select or transform the executable request.
    pub sequence_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StageEditRecovery {
    pub context: EditRecoveryContext,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadEditRecovery {
    pub project_id: String,
    pub recovery_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AckEditRecovery {
    pub project_id: String,
    pub recovery_id: String,
}

/// Preserve the original public reply, including selective-history identity.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditRecoveryReceipt {
    PublicApply {
        transaction: TransactionResult,
    },
    Undo {
        commit: HistoryCommit,
    },
    Redo {
        commit: HistoryCommit,
    },
    Restore {
        transaction: TransactionResult,
    },
    SubtitleGenerate {
        caption: crate::subtitles::CaptionReceipt,
    },
    SubtitleImport {
        caption: crate::subtitles::CaptionReceipt,
    },
    FontImport {
        transaction: TransactionResult,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditRecoveryRecord {
    pub recovery_id: String,
    pub project_id: String,
    /// Digest of the canonical executable context. Historical raw payloads and
    /// their original digest remain verified and unchanged in private storage.
    pub request_digest: String,
    pub context: EditRecoveryContext,
    pub receipt: Option<EditRecoveryReceipt>,
}

/// A single canonical record, including records read from historical storage.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditRecoveryInspection {
    pub record: Option<EditRecoveryRecord>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EditRecoveryResolution {
    Saved,
    NotSaved,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditRecoveryAck {
    pub recovery_id: String,
    pub resolution: EditRecoveryResolution,
    pub receipt: Option<EditRecoveryReceipt>,
}
