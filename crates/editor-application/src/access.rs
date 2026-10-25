//! Shared policy: trusted identity claims are host values, not request DTOs.
pub use opencut_api_contracts::access::*;
use opencut_contracts::{EditorError, Operation, TransactionRequest};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Caller {
    LocalOwner,
    Credential(VerifiedCredential),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedCredential {
    principal_id: String,
    credential_id: String,
}
impl VerifiedCredential {
    /// Native authentication adapters call this only after matching a secret's
    /// digest against an enabled principal and nonrevoked credential.
    pub fn from_verified(principal_id: String, credential_id: String) -> Self {
        Self {
            principal_id,
            credential_id,
        }
    }
    pub fn principal_id(&self) -> &str {
        &self.principal_id
    }
    pub fn credential_id(&self) -> &str {
        &self.credential_id
    }
}
impl Caller {
    pub fn principal_id(&self) -> &str {
        match self {
            Self::LocalOwner => "local-owner",
            Self::Credential(c) => c.principal_id(),
        }
    }
    pub fn is_owner(&self) -> bool {
        matches!(self, Self::LocalOwner)
    }
}
pub fn denied() -> EditorError {
    EditorError::new(
        "NOT_AUTHORIZED",
        "The current credential is not authorized for this resource or action",
    )
}

/// Restricted hosts use an explicit allow-list. New tools remain owner-only
/// until their data and native I/O boundaries have been reviewed.
pub fn authorize_tool(caller: &Caller, name: &str) -> Result<(), EditorError> {
    if caller.is_owner()
        || matches!(
            name,
            "opencut_capabilities"
                | "opencut_audio_preview_capability"
                | "opencut_audio_preview_recovery_stage"
                | "opencut_audio_preview_recovery_inspect"
                | "opencut_audio_preview_recovery_dispatch"
                | "opencut_audio_preview_recovery_ack"
                | "opencut_audio_preview_recovery_retire"
                | "opencut_audio_preview_create"
                | "opencut_audio_preview_inspect"
                | "opencut_audio_preview_control"
                | "opencut_audio_preview_events"
                | "opencut_audio_preview_acquire"
                | "opencut_visual_preview_frame"
                | "opencut_visual_preview_cache_prepare"
                | "opencut_visual_preview_edit_draft_frame"
                | "opencut_element_transform_sample"
                | "opencut_visual_preview_resource_open"
                | "opencut_visual_preview_resource_read"
                | "opencut_visual_preview_resource_release"
                | "opencut_visual_preview_font_authorize"
                | "opencut_visual_preview_font_batch_authorize"
                | "opencut_source_art_get"
                | "opencut_source_art_frame"
                | "opencut_source_art_evidence"
                | "opencut_source_art_evidence_cancel"
                | "opencut_source_art_cancel"
                | "opencut_source_art_waveform_display_get"
                | "opencut_access_inspect"
                | "opencut_project_list"
                | "opencut_project_inspect"
                | "opencut_project_history"
                | "opencut_project_history_navigation"
                | "opencut_project_restore"
                | "opencut_project_summary"
                | "opencut_project_revision_actors"
                | "opencut_edit_agent_plan"
                | "opencut_edit_recipe_plan"
                | "opencut_project_timeline_query"
                | "opencut_artifact_get"
                | "opencut_project_resource_inspect"
                | "opencut_media_import_plan"
                | "opencut_text_plan"
                | "opencut_mask_geometry"
                | "opencut_scene_geometry"
                | "opencut_subtitle_classic_ass_plan"
                | "opencut_brand_resources_list"
                | "opencut_brand_resources_source"
                | "opencut_brand_resources_import"
                | "opencut_catalog_resources_list"
                | "opencut_catalog_resources_import"
                | "opencut_catalog_resources_generate"
                | "opencut_keyframe_legacy_conversion_plan"
                | "opencut_keyframe_plan"
                | "opencut_keyframe_sample"
                | "opencut_element_properties_plan"
                | "opencut_edit_timeline_plan"
                | "opencut_graphic_plan"
                | "opencut_text_canvas_plan"
                | "opencut_track_remove_plan"
                | "opencut_font_catalog"
                | "opencut_font_source"
                | "opencut_font_import"
                | "opencut_font_derived_proof"
                | "opencut_font_synthesis_proof"
                | "opencut_font_derived_recovery_stage"
                | "opencut_font_derived_recovery_read"
                | "opencut_font_derived_recovery_ack"
                | "opencut_analysis_sequence_transcribe"
                | "opencut_analysis_sequence_transcribe_capability"
                | "opencut_subtitle_generate"
                | "opencut_subtitle_import"
                | "opencut_subtitle_export"
                | "opencut_subtitle_text_plan"
                | "opencut_subtitle_ass_plan"
                | "opencut_subtitle_text_export"
                | "opencut_font_recovery_stage"
                | "opencut_font_recovery_inspect"
                | "opencut_font_recovery_ack"
                | "opencut_project_source_audio_plan"
                | "opencut_project_audio_gain_plan"
                | "opencut_edit_plan"
                | "opencut_timeline_copy"
                | "opencut_timeline_paste_plan"
                | "opencut_timeline_duplicate_plan"
                | "opencut_edit_recovery_stage"
                | "opencut_edit_recovery_inspect"
                | "opencut_edit_recovery_ack"
                | "opencut_edit_apply"
                | "opencut_edit_undo_plan"
                | "opencut_edit_undo_apply"
                | "opencut_edit_redo_plan"
                | "opencut_edit_redo_apply"
                | "opencut_edit_branch_create"
                | "opencut_edit_branch_list"
                | "opencut_edit_branch_compare"
                | "opencut_edit_branch_merge"
                | "opencut_job_view"
                | "opencut_job_query"
                | "opencut_job_changes"
                | "opencut_render_capability"
                | "opencut_render_submit"
                | "opencut_quality_inspect"
                | "opencut_quality_evidence_plan"
                | "opencut_media_content_search"
                | "opencut_analysis_asset"
                | "opencut_media_map_time"
                | "opencut_job_submit"
                | "opencut_analysis_speaker_turns"
                | "opencut_analysis_transcribe"
                | "opencut_analysis_query"
                | "opencut_analysis_evidence_query"
                | "opencut_analysis_silence"
                | "opencut_analysis_shots"
                | "opencut_analysis_waveform"
                | "opencut_edit_control_get"
                | "opencut_preview_create"
                | "opencut_preview_inspect"
                | "opencut_preview_control"
                | "opencut_preview_events"
                | "opencut_preview_acquire"
                | "opencut_job_request_cancel"
                | "opencut_event_changes"
                | "opencut_audit_query"
                | "opencut_resource_read"
                | "opencut_render_preflight"
                | "opencut_upload_begin"
                | "opencut_upload_inspect"
                | "opencut_upload_complete"
                | "opencut_upload_abort"
                | "opencut_upload_query"
                | "opencut_upload_retention"
                | "opencut_upload_release"
                | "opencut_upload_quota"
                | "opencut_project_media_inspect"
                | "opencut_project_media_query"
                | "opencut_plugin_grants"
                | "opencut_plugin_preset_inspect"
                | "opencut_plugin_preset_plan"
                | "opencut_project_plugin_list"
                | "opencut_project_plugin_inspect"
                | "opencut_project_plugin_plan"
                | "opencut_project_readonly_manifest"
                | "opencut_catalog_resources_recipe"
        )
    {
        Ok(())
    } else {
        Err(denied())
    }
}
pub fn validate_policy(policy: &PrincipalPolicy) -> Result<(), EditorError> {
    let valid_id =
        |id: &str| !id.is_empty() && id.len() <= 1024 && !id.chars().any(char::is_control);
    let revision = policy
        .expected_revision
        .parse::<i64>()
        .ok()
        .filter(|r| *r >= 0 && r.to_string() == policy.expected_revision);
    if !valid_id(&policy.principal_id)
        || policy.principal_id.starts_with("local-")
        || policy.grants.len() > 1000
        || revision.is_none_or(|r| r == i64::MAX)
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Invalid principal policy",
        ));
    }
    let mut projects = HashSet::new();
    for grant in &policy.grants {
        let mask = grant.actions.iter().fold(0, |mask, a| mask | a.mask());
        if !valid_id(&grant.project_id)
            || !projects.insert(&grant.project_id)
            || mask & ProjectAction::Read.mask() == 0
            || grant.actions.len() > 7
            || mask.count_ones() as usize != grant.actions.len()
        {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Project grants require unique actions including read",
            ));
        }
    }
    Ok(())
}
/// A restricted caller may bind only an authoritative resource of this project.
/// The lookup runs inside the caller's current authorization/transaction boundary.
pub fn restricted_edit(
    request: &TransactionRequest,
    lookup: &impl Fn(&str) -> Result<Option<opencut_contracts::Asset>, EditorError>,
) -> Result<(), EditorError> {
    for op in &request.operations {
        if let Operation::BindAsset { asset } | Operation::ReplaceAsset { asset } = op {
            let id = asset
                .uri
                .strip_prefix("opencut-media://")
                .ok_or_else(denied)?;
            crate::uploads::validate_media_id(id).map_err(|_| denied())?;
            let mut authoritative = lookup(id)?.ok_or_else(denied)?;
            // Logical timeline identity and display name are editor-owned.
            authoritative.id = asset.id.clone();
            authoritative.name = asset.name.clone();
            if &authoritative != asset {
                return Err(denied());
            }
        }
    }
    Ok(())
}

pub trait AuthorizationRead {
    fn current_grants(&self, caller: &Caller) -> Result<Vec<ProjectGrant>, EditorError>;
}
pub fn require(
    store: &impl AuthorizationRead,
    caller: &Caller,
    project: &str,
    action: ProjectAction,
) -> Result<(), EditorError> {
    if caller.is_owner() {
        return Ok(());
    }
    if store
        .current_grants(caller)?
        .iter()
        .any(|g| g.project_id == project && g.actions.contains(&action))
    {
        Ok(())
    } else {
        Err(denied())
    }
}

/// Trusted native execution policy derived from the host-bound identity, never JSON.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceAccess {
    OwnerPaths,
    ProjectContent,
}
pub struct ExecutionSnapshot {
    pub snapshot: opencut_contracts::ProjectSnapshot,
    pub sources: SourceAccess,
}
/// Obtain run_jobs authorization and the requested immutable project revision
/// under one consistent read boundary. No storage lock survives this call.
pub trait ProjectExecutionRead {
    fn execution_snapshot(
        &self,
        project_id: &str,
        revision: &str,
    ) -> Result<ExecutionSnapshot, EditorError>;
}

#[cfg(test)]
mod recovery_authorization_tests {
    use super::*;

    #[test]
    fn recovery_tools_remain_owner_only_before_project_or_media_resolution() {
        let restricted = Caller::Credential(VerifiedCredential::from_verified(
            "agent".into(),
            "credential".into(),
        ));
        for name in [
            "opencut_project_package_choose_location",
            "opencut_project_package_export",
            "opencut_project_package_inspect",
            "opencut_project_package_import",
            "opencut_classic_analyze",
            "opencut_classic_import",
        ] {
            assert_eq!(
                authorize_tool(&restricted, name).unwrap_err().code,
                "NOT_AUTHORIZED"
            );
            assert!(authorize_tool(&Caller::LocalOwner, name).is_ok());
        }
    }
}
