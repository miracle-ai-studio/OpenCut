//! URI resolution and projections over the same project/job read ports as tools.
use crate::queries::JobRead;
use opencut_api_contracts::{
    capabilities::HostCapabilities, context::*, domains, tools::tool_specs,
};
use opencut_contracts::EditorError;
use serde_json::json;

pub mod project_context;

pub enum ContextSnapshot {
    Current(ProjectContext),
    Legacy(opencut_contracts::ProjectSnapshot),
    HistoricalVisual(opencut_contracts::historical_visual::HistoricalVisualSnapshot),
}
pub trait ProjectContextRead {
    fn project_context(
        &self,
        project: &str,
        revision: Option<&str>,
        sequence: Option<&str>,
    ) -> Result<ContextSnapshot, EditorError>;
}

pub const STATIC_RESOURCES: &[(&str, &str)] = &[
    ("opencut://system/capabilities", "capabilities"),
    ("opencut://schemas/tools", "tool-schemas"),
    ("opencut://schemas/domains", "domain-catalog"),
    ("opencut://schemas/event-stream", "event-stream-schema"),
];
pub const TEMPLATES: &[(&str, &str, &str)] = &[
    (
        "opencut://projects/{project}/branches/{branch}/summary",
        "project-head",
        "Current branch summary; only main is currently supported. Subscribable.",
    ),
    (
        "opencut://projects/{project}/revisions/{revision}/summary",
        "project-revision",
        "Bounded immutable project summary; continue with the returned fixed revision.",
    ),
    (
        "opencut://projects/{project}/branches/{branch}/revisions/{revision}/summary",
        "project-revision",
        "Immutable revision summary; branch must be main.",
    ),
    (
        "opencut://projects/{project}/revisions/{revision}/sequences/{sequence}/timeline",
        "revision-timeline",
        "First bounded timeline page; continue through project.timeline.query at the fixed revision.",
    ),
    (
        "opencut://jobs/{job}",
        "job-view",
        "Public task state without execution inputs or private paths. Subscribable.",
    ),
    (
        "opencut://artifacts/{artifact}",
        "artifact-description",
        "Content-bound artifact metadata, not bytes or proof of current availability.",
    ),
    (
        "opencut://schemas/domains/{domain}",
        "domain-schema",
        "Actions and exact input/output schemas for one editing domain.",
    ),
    (
        "opencut://schemas/operations/{name}",
        "operation-schema",
        "Current edit operation name and its exact input variant schema.",
    ),
];
#[derive(Debug, Clone)]
pub enum Target {
    Capabilities,
    Tools,
    Domains(Option<String>),
    EventSchema,
    Summary {
        project: String,
        revision: Option<String>,
    },
    Timeline {
        project: String,
        revision: String,
        sequence: String,
    },
    Job(String),
    Artifact(String),
    Operation {
        name: String,
    },
}
impl Target {
    pub fn mutable(&self) -> bool {
        matches!(self, Self::Summary { revision: None, .. } | Self::Job(_))
    }
}
fn invalid() -> EditorError {
    EditorError::new(
        "INVALID_ARGUMENT",
        "Invalid OpenCut resource URI; use a published template and percent-encode variable segments",
    )
}
fn missing() -> EditorError {
    EditorError::new("NOT_FOUND", "Resource was not found")
}
pub fn component(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(byte) {
                (*byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}
pub(crate) fn decode(value: &str) -> Result<String, EditorError> {
    if value.is_empty() {
        return Err(invalid());
    }
    let bytes = value.as_bytes();
    let mut decoded = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let pair = bytes.get(i + 1..i + 3).ok_or_else(invalid)?;
            let text = std::str::from_utf8(pair).map_err(|_| invalid())?;
            decoded.push(u8::from_str_radix(text, 16).map_err(|_| invalid())?);
            i += 3;
        } else if bytes[i].is_ascii_alphanumeric() || b"-._~".contains(&bytes[i]) {
            decoded.push(bytes[i]);
            i += 1;
        } else {
            return Err(invalid());
        }
    }
    if decoded.len() > 1024 {
        return Err(invalid());
    }
    String::from_utf8(decoded).map_err(|_| invalid())
}
pub fn target(uri: &str) -> Result<Target, EditorError> {
    if uri.len() > 16_384 {
        return Err(invalid());
    }
    let parts = uri
        .strip_prefix("opencut://")
        .ok_or_else(invalid)?
        .split('/')
        .map(decode)
        .collect::<Result<Vec<_>, _>>()?;
    let parts = parts.iter().map(String::as_str).collect::<Vec<_>>();
    match parts.as_slice() {
        ["system", "capabilities"] => Ok(Target::Capabilities),
        ["schemas", "tools"] => Ok(Target::Tools),
        ["schemas", "domains"] => Ok(Target::Domains(None)),
        ["schemas", "domains", domain] => Ok(Target::Domains(Some((*domain).to_owned()))),
        ["schemas", "event-stream"] => Ok(Target::EventSchema),
        ["projects", p, "branches", b, "summary"] if *b == "main" => Ok(Target::Summary {
            project: p.to_string(),
            revision: None,
        }),
        ["projects", p, "branches", b, "revisions", r, "summary"] if *b == "main" => {
            Ok(Target::Summary {
                project: p.to_string(),
                revision: Some(r.to_string()),
            })
        }
        ["projects", p, "revisions", r, "summary"] => Ok(Target::Summary {
            project: p.to_string(),
            revision: Some(r.to_string()),
        }),
        ["projects", p, "revisions", r, "sequences", s, "timeline"] => Ok(Target::Timeline {
            project: p.to_string(),
            revision: r.to_string(),
            sequence: s.to_string(),
        }),
        ["jobs", j] => Ok(Target::Job(j.to_string())),
        ["artifacts", a] => Ok(Target::Artifact(a.to_string())),
        ["schemas", "operations", n] => Ok(Target::Operation {
            name: n.to_string(),
        }),
        ["projects", _, "branches", _, "summary"]
        | ["projects", _, "branches", _, "revisions", _, "summary"] => Err(missing()),
        _ => Err(invalid()),
    }
}
pub fn read(
    store: &(impl ProjectContextRead + JobRead),
    input: ResourceReadInput,
    capabilities: impl FnOnce() -> HostCapabilities,
) -> Result<ResourceView, EditorError> {
    let payload = match target(&input.uri)? {
        Target::Capabilities => ResourcePayload::Capabilities(Box::new(capabilities())),
        Target::Tools => ResourcePayload::Tools(tool_specs()),
        Target::Domains(domain) => {
            ResourcePayload::Domains(domains::agent_catalog(domain.as_deref(), domain.is_some())?)
        }
        Target::EventSchema => {
            ResourcePayload::EventSchema(opencut_api_contracts::event_stream::schema())
        }
        Target::Operation { name } => {
            let declaration = capabilities().api.edit_operation_schema;
            let variants = declaration
                .get("oneOf")
                .and_then(|v| v.as_array())
                .ok_or_else(|| {
                    EditorError::new("INTERNAL_ERROR", "Operation registry schema is unavailable")
                })?;
            let mut schema = variants
                .iter()
                .find(|v| {
                    v.pointer("/properties/type/const").and_then(|s| s.as_str()) == Some(&name)
                })
                .cloned()
                .ok_or_else(missing)?;
            schema["$defs"] = declaration["$defs"].clone();
            schema["$schema"] = json!("https://json-schema.org/draft/2020-12/schema");
            ResourcePayload::OperationSchema(OperationSchema { name, schema })
        }
        Target::Summary { project, revision } => {
            match store.project_context(&project, revision.as_deref(), None)? {
                ContextSnapshot::Current(context) => {
                    ResourcePayload::ProjectContext(Box::new(context))
                }
                snapshot => {
                    let (p, revision) = match snapshot {
                        ContextSnapshot::Legacy(s) => (s.project, s.revision),
                        ContextSnapshot::HistoricalVisual(s) => (s.project.base, s.revision),
                        ContextSnapshot::Current(_) => {
                            unreachable!("current projection handled above")
                        }
                    };
                    let sequences = p
                        .sequences
                        .into_iter()
                        .map(|s| SequenceSummary {
                            sequence_id: s.id,
                            name: s.name,
                            width: s.width,
                            height: s.height,
                            fps: s.fps,
                            track_count: s.tracks.len(),
                            clip_count: s.tracks.iter().map(|t| t.clips.len()).sum(),
                            caption_count: s.caption_cues.len(),
                        })
                        .collect();
                    ResourcePayload::ProjectSummary(ProjectSummary {
                        project_id: p.id,
                        name: p.name,
                        branch: "main".into(),
                        revision,

                        default_sequence_id: p.default_sequence_id,
                        sequences,
                        asset_count: p.assets.len(),
                    })
                }
            }
        }
        Target::Timeline {
            project,
            revision,
            sequence: sequence_id,
        } => match store.project_context(&project, Some(&revision), Some(&sequence_id))? {
            ContextSnapshot::Current(context) => ResourcePayload::ProjectContext(Box::new(context)),
            ContextSnapshot::Legacy(snapshot) => {
                let sequence = snapshot
                    .project
                    .sequences
                    .into_iter()
                    .find(|s| s.id == sequence_id)
                    .ok_or_else(missing)?;
                ResourcePayload::Timeline(RevisionTimeline {
                    project_id: project,
                    revision: snapshot.revision,
                    sequence,
                })
            }
            ContextSnapshot::HistoricalVisual(snapshot) => {
                let visual = snapshot
                    .project
                    .visual
                    .into_iter()
                    .find(|v| v.sequence_id == sequence_id)
                    .unwrap_or(
                        opencut_contracts::historical_visual::HistoricalVisualSequenceVisual {
                            sequence_id: sequence_id.clone(),
                            shapes: vec![],
                            effects: vec![],
                            masks: vec![],
                            transitions: vec![],
                        },
                    );
                let sequence = snapshot
                    .project
                    .base
                    .sequences
                    .into_iter()
                    .find(|s| s.id == sequence_id)
                    .ok_or_else(missing)?;
                ResourcePayload::VisualTimeline(VisualTimeline {
                    project_id: project,
                    revision: snapshot.revision,

                    sequence,
                    visual,
                    keyframe_curves: snapshot
                        .project
                        .keyframe_curves
                        .into_iter()
                        .filter(|curve| curve.sequence_id == sequence_id)
                        .collect(),
                })
            }
        },
        Target::Job(job) => ResourcePayload::Job(Box::new(crate::queries::job_view(store, &job)?)),
        Target::Artifact(artifact) => {
            let (job, _) = crate::job_views::parse_artifact_id(&artifact)?;
            let view = crate::queries::job_view(store, job)?;
            ResourcePayload::Artifact(
                view.artifacts
                    .into_iter()
                    .find(|a| a.artifact_id == artifact)
                    .ok_or_else(missing)?,
            )
        }
    };
    let view = ResourceView {
        uri: input.uri,
        mime_type: "application/json".into(),
        payload,
    };
    if serde_json::to_vec(&view)
        .map_err(|_| EditorError::new("INTERNAL_ERROR", "Resource serialization failed"))?
        .len()
        > 2_097_152
    {
        return Err(EditorError::new(
            "RESOURCE_TOO_LARGE",
            "Resource exceeds 2 MiB; use bounded query tools",
        ));
    }
    Ok(view)
}
