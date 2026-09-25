//! Fixed-revision render preflight orchestration; execution is a capability port.
use crate::access::{ProjectExecutionRead, SourceAccess};
pub use opencut_api_contracts::render::{
    PreflightIssue, ProjectPreflightReport, RenderPreflightInput,
};
use opencut_contracts::{EditorError, ProjectSnapshot};
use opencut_media_contracts::{Container, VideoCodec};
pub struct FixedPreflight {
    pub snapshot: ProjectSnapshot,
    pub sequence_id: String,
    pub container: Container,
    pub video_codec: VideoCodec,
}
pub trait RenderPreflight {
    fn inspect(&self, request: FixedPreflight) -> Result<ProjectPreflightReport, EditorError>;
}
pub fn preflight(
    projects: &impl ProjectExecutionRead,
    backend: &impl RenderPreflight,
    input: RenderPreflightInput,
) -> Result<ProjectPreflightReport, EditorError> {
    let authorized = projects.execution_snapshot(&input.project_id, &input.revision)?;
    if authorized.snapshot.project.id != input.project_id
        || authorized.snapshot.revision != input.revision
    {
        return Err(EditorError::new(
            "INTERNAL_ERROR",
            "Execution reader returned a different project or revision",
        ));
    }
    let sources = authorized.sources;
    if sources == SourceAccess::ProjectContent {
        let required = match opencut_editor_core::dependencies::required_asset_ids(
            &authorized.snapshot,
            &input.sequence_id,
            opencut_editor_core::dependencies::AssetScope::All,
        ) {
            Ok(ids) => ids,
            Err(error) if error.code == "NOT_FOUND" => {
                return Ok(ProjectPreflightReport {
                    revision: input.revision,
                    sequence_id: input.sequence_id,
                    ready: false,
                    issues: vec![PreflightIssue {
                        code: "SEQUENCE_NOT_FOUND".into(),
                        message: public_issue_message("SEQUENCE_NOT_FOUND").into(),
                        asset_id: None,
                        clip_id: None,
                        cue_id: None,
                    }],
                });
            }
            Err(error) => return Err(error),
        };
        let issues: Vec<_> = authorized
            .snapshot
            .project
            .assets
            .iter()
            .filter(|asset| required.contains(&asset.id) && asset.version.is_none())
            .map(|asset| PreflightIssue {
                code: "ASSET_LOCK_REQUIRED".into(),
                message: public_issue_message("ASSET_LOCK_REQUIRED").into(),
                asset_id: Some(asset.id.clone()),
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
            snapshot: authorized.snapshot,
            sequence_id: input.sequence_id.clone(),
            container: input.container,
            video_codec: input.video_codec,
        })
        .map_err(|error| {
            if sources == SourceAccess::ProjectContent {
                EditorError::new(&error.code, public_issue_message(&error.code))
            } else {
                error
            }
        })?;
    if report.revision != input.revision
        || report.sequence_id != input.sequence_id
        || report.ready != report.issues.is_empty()
    {
        return Err(EditorError::new(
            "INTERNAL_ERROR",
            "Preflight adapter returned an inconsistent fixed-revision report",
        ));
    }
    if sources == SourceAccess::ProjectContent {
        for issue in &mut report.issues {
            issue.message = public_issue_message(&issue.code).into();
        }
    }
    Ok(report)
}
/// Stable public explanations retain actionable codes and entity IDs, never
/// backend diagnostics that may contain filesystem locations or command lines.
pub(crate) fn public_issue_message(code: &str) -> &'static str {
    match code {
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
    use crate::access::ExecutionSnapshot;
    use opencut_contracts::{CreateProject, FrameRate};
    use std::cell::Cell;
    struct Reader {
        scope: SourceAccess,
        deny: bool,
        legacy: bool,
    }
    impl ProjectExecutionRead for Reader {
        fn execution_snapshot(
            &self,
            id: &str,
            revision: &str,
        ) -> Result<ExecutionSnapshot, EditorError> {
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
            Ok(ExecutionSnapshot {
                snapshot: ProjectSnapshot {
                    project,
                    revision: revision.into(),
                },
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
                revision: request.snapshot.revision,
                sequence_id: request.sequence_id,
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
            container: Container::Mp4,
            video_codec: VideoCodec::H264,
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
    fn restricted_diagnostics_preserve_entity_ids_without_native_messages_or_details() {
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
        assert!(report.issues[0].message.contains("/private/"));
    }
}
