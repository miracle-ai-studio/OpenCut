//! One fixed-revision preflight; actual stored content selects the existing executor.
use crate::access::SourceAccess;
use crate::media_video_jobs::AuthorizedVideoSnapshot;
pub use opencut_api_contracts::render::{
    PreflightIssue, ProjectPreflightReport, RenderPreflightInput,
};
use opencut_contracts::{
    EditorError, ProjectSnapshot, historical_visual::HistoricalVisualSnapshot,
};

/// Historical readers stay internal to execution; callers never choose a generation.
pub enum PreflightSource {
    Historical(ProjectSnapshot),
    HistoricalVisual(HistoricalVisualSnapshot),
    Editor(AuthorizedVideoSnapshot),
}
impl PreflightSource {
    fn scope(&self) -> (&str, &str) {
        match self {
            Self::Historical(s) => (&s.project.id, &s.revision),
            Self::HistoricalVisual(s) => (&s.project.base.id, &s.revision),
            Self::Editor(s) => (&s.snapshot.project.id, &s.snapshot.revision),
        }
    }
    fn historical_base(&self) -> Option<ProjectSnapshot> {
        match self {
            Self::Historical(s) => Some(s.clone()),
            Self::HistoricalVisual(s) => Some(ProjectSnapshot {
                project: s.project.base.clone(),
                revision: s.revision.clone(),
            }),
            Self::Editor(_) => None,
        }
    }
    fn has_sequence(&self, sequence: &str) -> bool {
        match self {
            Self::Historical(s) => s.project.sequences.iter().any(|s| s.id == sequence),
            Self::HistoricalVisual(s) => s.project.base.sequences.iter().any(|s| s.id == sequence),
            Self::Editor(s) => s
                .snapshot
                .project
                .sequences
                .iter()
                .any(|s| s.id == sequence),
        }
    }
}
pub struct AuthorizedPreflight {
    pub source: PreflightSource,
    pub sources: SourceAccess,
}
pub trait PreflightRead {
    /// Read the actual fixed source, current run authority and host policy together.
    fn read_preflight(
        &self,
        project: &str,
        revision: &str,
    ) -> Result<AuthorizedPreflight, EditorError>;
}
pub struct FixedPreflight {
    pub source: PreflightSource,
    pub input: RenderPreflightInput,
}
pub trait RenderPreflight {
    fn inspect(&self, request: FixedPreflight) -> Result<ProjectPreflightReport, EditorError>;
}
pub fn issue(input: &RenderPreflightInput, code: &str) -> ProjectPreflightReport {
    ProjectPreflightReport {
        revision: input.revision.clone(),
        sequence_id: input.sequence_id.clone(),
        ready: false,
        issues: vec![PreflightIssue {
            code: code.into(),
            message: public_issue_message(code).into(),
            asset_id: None,
            clip_id: None,
            cue_id: None,
        }],
    }
}
pub fn preflight(
    projects: &impl PreflightRead,
    backend: &impl RenderPreflight,
    input: RenderPreflightInput,
) -> Result<ProjectPreflightReport, EditorError> {
    let authorized = projects.read_preflight(&input.project_id, &input.revision)?;
    if authorized.source.scope() != (&input.project_id[..], &input.revision[..]) {
        return Err(EditorError::new(
            "INTERNAL_ERROR",
            "Execution reader returned a different project or revision",
        ));
    }
    if !authorized.source.has_sequence(&input.sequence_id) {
        return Ok(issue(&input, "SEQUENCE_NOT_FOUND"));
    }
    let restricted = authorized.sources == SourceAccess::ProjectContent;
    if restricted && let Some(snapshot) = authorized.source.historical_base() {
        let required = opencut_editor_core::dependencies::required_asset_ids(
            &snapshot,
            &input.sequence_id,
            opencut_editor_core::dependencies::AssetScope::All,
        )?;
        let issues: Vec<_> = snapshot
            .project
            .assets
            .iter()
            .filter(|a| required.contains(&a.id) && a.version.is_none())
            .map(|a| PreflightIssue {
                code: "ASSET_LOCK_REQUIRED".into(),
                message: public_issue_message("ASSET_LOCK_REQUIRED").into(),
                asset_id: Some(a.id.clone()),
                clip_id: None,
                cue_id: None,
            })
            .collect();
        if !issues.is_empty() {
            return Ok(ProjectPreflightReport {
                revision: input.revision,
                sequence_id: input.sequence_id,
                ready: false,
                issues,
            });
        }
    }
    let mut report = backend
        .inspect(FixedPreflight {
            source: authorized.source,
            input: input.clone(),
        })
        .map_err(|error| EditorError::new(&error.code, public_issue_message(&error.code)))?;
    if report.revision != input.revision
        || report.sequence_id != input.sequence_id
        || report.ready != report.issues.is_empty()
    {
        return Err(EditorError::new(
            "INTERNAL_ERROR",
            "Preflight adapter returned an inconsistent fixed-revision report",
        ));
    }
    for issue in &mut report.issues {
        issue.message = public_issue_message(&issue.code).into();
    }
    Ok(report)
}
/// Stable public explanations retain actionable codes and entity IDs, never
/// backend diagnostics that may contain filesystem locations or command lines.
pub(crate) fn public_issue_message(code: &str) -> &'static str {
    match code {
        "PROJECT_PROMOTION_REQUIRED" => {
            "Promote this historical revision before submitting new renders"
        }
        "SEQUENCE_NOT_FOUND" => "The requested sequence does not exist in this revision",
        "ASSET_LOCK_REQUIRED" => {
            "This sequence requires unversioned media; an owner must bind immutable content before scoped preflight"
        }
        "ASSET_INTEGRITY_ERROR" | "ASSET_VERSION_INVALID" => {
            "A referenced immutable asset failed content verification; restore or rebind verified content"
        }
        "MEDIA_NOT_FOUND" | "ASSET_UNAVAILABLE" => {
            "Referenced media is unavailable; restore or rebind its verified content"
        }
        "TOOL_UNAVAILABLE" => {
            "A required native media tool is unavailable; request host maintenance"
        }
        "CAPABILITY_UNAVAILABLE" => {
            "The requested rendering capability is unavailable on this host"
        }
        "FONT_UNAVAILABLE" | "FONT_NOT_FOUND" => {
            "A required caption font is unavailable on this host"
        }
        "INVALID_PROJECT" => "The fixed project revision failed validation",
        _ => {
            "Preflight found a rendering issue; use its code and entity identifiers to correct the project or request host maintenance"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use opencut_contracts::{CreateProject, FrameRate};
    use opencut_media_contracts::{Container, VideoCodec};
    use std::cell::Cell;
    struct Reader {
        scope: SourceAccess,
        deny: bool,
        legacy: bool,
    }
    impl PreflightRead for Reader {
        fn read_preflight(
            &self,
            id: &str,
            revision: &str,
        ) -> Result<AuthorizedPreflight, EditorError> {
            if self.deny {
                return Err(crate::access::denied());
            }
            let mut project = opencut_editor_core::create_project(&CreateProject {
                id: id.into(),
                name: "test".into(),
                sequence_id: "main".into(),
                width: 64,
                height: 48,
                fps: FrameRate {
                    numerator: 10,
                    denominator: 1,
                },
            })?;
            if self.legacy {
                project.assets.push(serde_json::from_value(serde_json::json!({"id":"a","name":"legacy","kind":"image","uri":"/private/never-read","duration":null,"metadata":{}})).unwrap());
                project.sequences[0].tracks.push(serde_json::from_value(serde_json::json!({"id":"t","name":"t","kind":"graphic","enabled":true,"muted":false,"locked":false,"params":{},"clips":[{"id":"c","asset_id":"a","timeline_range":{"start":{"ticks":"0","timebase":10},"duration":{"ticks":"10","timebase":10}},"source_range":null,"params":{},"channels":[]}]})).unwrap());
            }
            Ok(AuthorizedPreflight {
                source: PreflightSource::Historical(ProjectSnapshot {
                    project,
                    revision: revision.into(),
                }),
                sources: self.scope,
            })
        }
    }
    struct Backend {
        calls: Cell<usize>,
        fail: bool,
    }
    impl RenderPreflight for Backend {
        fn inspect(&self, request: FixedPreflight) -> Result<ProjectPreflightReport, EditorError> {
            self.calls.set(self.calls.get() + 1);
            if self.fail {
                return Err(
                    EditorError::new("ASSET_INTEGRITY_ERROR", "/private/cas-path")
                        .with_details(serde_json::json!({"path":"/private/cas-path"})),
                );
            }
            Ok(ProjectPreflightReport {
                revision: request.input.revision,
                sequence_id: request.input.sequence_id,
                ready: false,
                issues: vec![PreflightIssue {
                    code: "MEDIA_PROBE_FAILED".into(),
                    message: "ffprobe /private/cas-path".into(),
                    asset_id: Some("a".into()),
                    clip_id: Some("c".into()),
                    cue_id: None,
                }],
            })
        }
    }
    fn input() -> RenderPreflightInput {
        RenderPreflightInput {
            project_id: "p".into(),
            revision: "r".into(),
            sequence_id: "main".into(),
            options: Default::default(),
            range: None,
        }
    }
    #[test]
    fn denied_and_legacy_requests_never_enter_native_execution() {
        let backend = Backend {
            calls: Cell::new(0),
            fail: false,
        };
        assert_eq!(
            preflight(
                &Reader {
                    scope: SourceAccess::ProjectContent,
                    deny: true,
                    legacy: false
                },
                &backend,
                input()
            )
            .unwrap_err()
            .code,
            "NOT_AUTHORIZED"
        );
        let report = preflight(
            &Reader {
                scope: SourceAccess::ProjectContent,
                deny: false,
                legacy: true,
            },
            &backend,
            input(),
        )
        .unwrap();
        assert_eq!(report.issues[0].code, "ASSET_LOCK_REQUIRED");
        assert_eq!(report.issues[0].asset_id.as_deref(), Some("a"));
        assert_eq!(backend.calls.get(), 0);
    }
    #[test]
    fn public_diagnostics_preserve_entity_ids_without_native_messages_or_details() {
        let reader = Reader {
            scope: SourceAccess::ProjectContent,
            deny: false,
            legacy: false,
        };
        let report = preflight(
            &reader,
            &Backend {
                calls: Cell::new(0),
                fail: false,
            },
            input(),
        )
        .unwrap();
        assert_eq!(report.issues[0].code, "MEDIA_PROBE_FAILED");
        assert_eq!(report.issues[0].clip_id.as_deref(), Some("c"));
        assert!(
            !serde_json::to_string(&report)
                .unwrap()
                .contains("/private/")
        );
        let error = preflight(
            &reader,
            &Backend {
                calls: Cell::new(0),
                fail: true,
            },
            input(),
        )
        .unwrap_err();
        assert_eq!(error.code, "ASSET_INTEGRITY_ERROR");
        assert!(!serde_json::to_string(&error).unwrap().contains("/private/"));
        let owner = Reader {
            scope: SourceAccess::OwnerPaths,
            deny: false,
            legacy: false,
        };
        let report = preflight(
            &owner,
            &Backend {
                calls: Cell::new(0),
                fail: false,
            },
            input(),
        )
        .unwrap();
        assert!(!report.issues[0].message.contains("/private/"));
    }
}
