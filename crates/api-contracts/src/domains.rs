//! Compact, domain-oriented discovery over the versioned v1 operation registry.
//! The leaf operation remains the authorization, audit and compatibility unit.
use crate::tools::{ToolSpec, tool_spec, tool_summaries};
use opencut_contracts::EditorError;
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const DOMAINS: &[(&str, &str)] = &[
    (
        "system",
        "Discover host capabilities and supported operations.",
    ),
    (
        "project",
        "Create, inspect and restore versioned editing projects.",
    ),
    ("edit", "Validate and atomically commit timeline changes."),
    ("media", "Transfer, inspect and repair project media."),
    (
        "task",
        "Submit, observe and cancel durable media and render tasks.",
    ),
    (
        "observe",
        "Read resources, project events and mutation history.",
    ),
    ("plugin", "Install and invoke constrained editing plugins."),
    ("access", "Inspect access and authorization audit records."),
    (
        "workspace",
        "Owner-only database, storage and publication maintenance.",
    ),
];

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DomainCall {
    pub action: String,
    pub input: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ActionDescriptor {
    pub action: String,
    pub operation: String,
    pub description: String,
    pub read_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DomainDescriptor {
    pub domain: String,
    pub description: String,
    pub actions: Vec<ActionDescriptor>,
}

pub fn is_domain_tool(name: &str) -> bool {
    name.strip_prefix("opencut_")
        .is_some_and(|name| DOMAINS.iter().any(|(domain, _)| *domain == name))
}

fn route(operation: &str) -> Option<(&'static str, String)> {
    let suffix = operation.strip_prefix("opencut_")?;
    let (domain, action) = if suffix == "capabilities" {
        ("system", "capabilities".to_owned())
    } else if suffix.starts_with("analysis_") || suffix.starts_with("source_art_") {
        ("media", suffix.replace('_', "."))
    } else if suffix.starts_with("preview_")
        || suffix.starts_with("audio_preview_v4_")
        || suffix.starts_with("visual_preview_v4_")
    {
        ("task", suffix.replace('_', "."))
    } else if suffix.starts_with("subtitle_") || suffix.starts_with("timeline_v4_") || suffix.starts_with("text_v1_") {
        ("edit", suffix.replace('_', "."))
    } else if suffix.starts_with("classic_") {
        ("project", suffix.replace('_', "."))
    } else if suffix.starts_with("project_media_")
        || suffix.starts_with("upload_")
        || suffix.starts_with("media_")
        || suffix.starts_with("font_v1_")
        || suffix.starts_with("font_recovery_v1_")
    {
        ("media", suffix.replace('_', "."))
    } else if suffix.starts_with("project_plugin_") || suffix.starts_with("plugin_") {
        if suffix.starts_with("plugin_publication_") {
            ("workspace", suffix.replace('_', "."))
        } else if let Some(rest) = suffix.strip_prefix("plugin_") {
            ("plugin", rest.replace('_', "."))
        } else {
            ("plugin", suffix.replace('_', "."))
        }
    } else if let Some(rest) = suffix.strip_prefix("project_") {
        ("project", rest.replace('_', "."))
    } else if suffix.starts_with("revision_") {
        ("project", suffix.replace('_', "."))
    } else if let Some(rest) = suffix.strip_prefix("edit_") {
        ("edit", rest.replace('_', "."))
    } else if let Some(rest) = suffix.strip_prefix("job_") {
        ("task", rest.replace('_', "."))
    } else if suffix == "render_v3_preflight" {
        ("task", "render.v3.preflight".to_owned())
    } else if suffix == "render_preflight" {
        ("task", "render.preflight".to_owned())
    } else if suffix.starts_with("event_")
        || suffix.starts_with("events_")
        || suffix.starts_with("audit_")
        || suffix.starts_with("resource_")
    {
        ("observe", suffix.replace('_', "."))
    } else if let Some(rest) = suffix.strip_prefix("access_") {
        ("access", rest.replace('_', "."))
    } else if suffix.starts_with("denial_") {
        ("access", suffix.replace('_', "."))
    } else if suffix.starts_with("database_")
        || suffix.starts_with("storage_")
        || suffix.starts_with("cas_gc_")
        || suffix.starts_with("artifact_retention_")
        || suffix.starts_with("artifact_references_")
        || suffix.starts_with("artifact_gc_")
        || suffix.starts_with("maintenance_")
    {
        ("workspace", suffix.replace('_', "."))
    } else {
        return None;
    };
    Some((domain, action))
}

pub fn catalog(domain: Option<&str>, schemas: bool) -> Result<Vec<DomainDescriptor>, EditorError> {
    if domain.is_some_and(|name| !DOMAINS.iter().any(|(known, _)| *known == name)) {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Unknown operation domain",
        ));
    }
    let operations = tool_summaries();
    let mut result = Vec::new();
    for (name, description) in DOMAINS {
        if domain.is_some_and(|selected| selected != *name) {
            continue;
        }
        let actions = operations
            .iter()
            .filter_map(|tool| {
                let (owner, action) = route(tool.name)?;
                (owner == *name).then(|| {
                    let full = schemas.then(|| tool_spec(tool.name).expect("registered operation"));
                    ActionDescriptor {
                        action,
                        operation: tool.name.to_owned(),
                        description: tool.description.to_owned(),
                        read_only: tool.read_only,
                        input_schema: full.as_ref().map(|spec| spec.input_schema.clone()),
                        output_schema: full.map(|spec| spec.output_schema),
                    }
                })
            })
            .collect();
        result.push(DomainDescriptor {
            domain: (*name).to_owned(),
            description: (*description).to_owned(),
            actions,
        });
    }
    Ok(result)
}

pub fn tool_specs_compact() -> Vec<ToolSpec> {
    let catalog = catalog(None, false).expect("static domains are valid");
    catalog
        .into_iter()
        .map(domain_tool_spec_from_descriptor)
        .collect()
}

fn domain_tool_spec_from_descriptor(domain: DomainDescriptor) -> ToolSpec {
    let actions: Vec<_> = domain.actions.iter().map(|a| a.action.clone()).collect();
    ToolSpec {
        name: format!("opencut_{}", domain.domain),
        description: format!(
            "{} Select an action in this domain. Read opencut://schemas/domains/{} for exact per-action input and output schemas. Legacy operation names remain callable for compatibility.",
            domain.description, domain.domain
        ),
        read_only: domain.actions.iter().all(|action| action.read_only),
        input_schema: json!({
            "type": "object",
            "properties": {
                "action": {"type": "string", "enum": actions},
                "input": {"type": "object", "description": "Arguments for the selected action; consult its domain schema."}
            },
            "required": ["action", "input"],
            "additionalProperties": false
        }),
        output_schema: serde_json::to_value(schema_for!(crate::Response<Value>))
            .expect("response schema is JSON"),
    }
}

pub fn domain_tool_spec(name: &str) -> Option<ToolSpec> {
    let domain = name.strip_prefix("opencut_")?;
    let descriptor = catalog(Some(domain), false).ok()?.pop()?;
    Some(domain_tool_spec_from_descriptor(descriptor))
}

/// Resolve only to a registered leaf operation. Never authorize the broad domain name.
pub fn resolve(name: &str, arguments: Value) -> Result<(String, Value), EditorError> {
    if !is_domain_tool(name) {
        return Ok((name.to_owned(), arguments));
    }
    let call: DomainCall = serde_json::from_value(arguments)
        .map_err(|error| EditorError::new("INVALID_ARGUMENT", error.to_string()))?;
    if !call.input.is_object() {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Domain input must be an object",
        ));
    }
    let domain = name.strip_prefix("opencut_").expect("checked domain name");
    for operation in tool_summaries() {
        if route(operation.name)
            .is_some_and(|(owner, action)| owner == domain && action == call.action)
        {
            return Ok((operation.name.to_owned(), call.input));
        }
    }
    Err(EditorError::new(
        "INVALID_ARGUMENT",
        format!("Unknown action '{}' in domain '{domain}'", call.action),
    ))
}

/// Identify the leaf operation for denial audit without cloning media-control JSON.
pub fn operation_name(name: &str, arguments: &Value) -> Option<String> {
    if !is_domain_tool(name) {
        return Some(name.to_owned());
    }
    let domain = name.strip_prefix("opencut_")?;
    let action = arguments.get("action")?.as_str()?;
    tool_summaries()
        .iter()
        .find(|tool| {
            route(tool.name)
                .is_some_and(|(owner, candidate)| owner == domain && candidate == action)
        })
        .map(|tool| tool.name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_public_operation_has_one_unambiguous_domain_action() {
        let mut routes = HashSet::new();
        for tool in tool_summaries() {
            let (domain, action) = route(tool.name).expect("public operation is classified");
            assert!(routes.insert((domain, action)), "duplicate domain action");
        }
        assert_eq!(routes.len(), tool_summaries().len());
    }
}
