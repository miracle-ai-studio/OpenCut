//! Compact, domain-oriented discovery over the single operation registry.
//! The leaf operation remains the authorization, audit and compatibility unit.
use crate::tools::{ToolDomain, ToolSpec, tool_spec, tool_summaries};
use opencut_contracts::EditorError;
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

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
        .is_some_and(|name| ToolDomain::ALL.iter().any(|domain| domain.name() == name))
}

pub fn catalog(domain: Option<&str>, schemas: bool) -> Result<Vec<DomainDescriptor>, EditorError> {
    if domain.is_some_and(|name| !ToolDomain::ALL.iter().any(|known| known.name() == name)) {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Unknown operation domain",
        ));
    }
    let operations = tool_summaries();
    let mut result = Vec::new();
    for owner in ToolDomain::ALL {
        let name = owner.name();
        if domain.is_some_and(|selected| selected != name) {
            continue;
        }
        let actions = operations
            .iter()
            .filter_map(|tool| {
                (tool.domain == *owner).then(|| {
                    let full = schemas.then(|| tool_spec(tool.name).expect("registered operation"));
                    ActionDescriptor {
                        action: tool.action.to_owned(),
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
            domain: name.to_owned(),
            description: owner.description().to_owned(),
            actions,
        });
    }
    Ok(result)
}

pub fn tool_specs_compact() -> Vec<ToolSpec> {
    let catalog = catalog(None, false).expect("static domains are valid");
    catalog
        .into_iter()
        .filter(|domain| domain.domain != "workspace" && !domain.actions.is_empty())
        .map(domain_tool_spec_from_descriptor)
        .collect()
}

fn domain_tool_spec_from_descriptor(domain: DomainDescriptor) -> ToolSpec {
    let actions: Vec<_> = domain.actions.iter().map(|a| a.action.clone()).collect();
    let mut generator = schemars::SchemaGenerator::default();
    let alternatives: Vec<_> = domain
        .actions
        .iter()
        .map(|action| {
            let input = crate::tools::tool_input_schema(&action.operation, &mut generator)
                .expect("registered domain operation");
            json!({
                "description": action.description,
                "properties": {"action": {"const": action.action}, "input": input},
                "required": ["action", "input"]
            })
        })
        .collect();
    ToolSpec {
        name: format!("opencut_{}", domain.domain),
        description: format!(
            "{} Select an action; its input is defined by the matching schema alternative. Read opencut://schemas/domains/{} for per-action output schemas. Every action resolves to the same registered and independently authorized operation.",
            domain.description, domain.domain
        ),
        read_only: domain.actions.iter().all(|action| action.read_only),
        input_schema: json!({
            "type": "object",
            "properties": {
                "action": {"type": "string", "enum": actions},
                "input": {"type": "object", "description": "Typed arguments for the selected action."}
            },
            "required": ["action", "input"],
            "additionalProperties": false,
            "oneOf": alternatives,
            "$defs": generator.take_definitions(true)
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
        if operation.domain.name() == domain && operation.action == call.action {
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
        .find(|tool| tool.domain.name() == domain && tool.action == action)
        .map(|tool| tool.name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn listed_domain_schemas_include_every_registered_action_and_resolve_all_references() {
        fn references(value: &Value, root: &Value) {
            if let Some(reference) = value.get("$ref").and_then(Value::as_str) {
                assert!(reference.starts_with('#'));
                assert!(
                    root.pointer(&reference[1..]).is_some(),
                    "unresolved {reference}"
                );
            }
            match value {
                Value::Object(object) => object.values().for_each(|v| references(v, root)),
                Value::Array(array) => array.iter().for_each(|v| references(v, root)),
                _ => {}
            }
        }
        let listed = tool_specs_compact();
        assert_eq!(listed.len(), 11);
        for tool in listed {
            let domain = tool.name.strip_prefix("opencut_").unwrap();
            let expected = catalog(Some(domain), false).unwrap().pop().unwrap();
            let actual = tool.input_schema["properties"]["action"]["enum"]
                .as_array()
                .unwrap();
            assert_eq!(actual.len(), expected.actions.len());
            for action in expected.actions {
                assert!(
                    actual.contains(&json!(action.action)),
                    "missing {}",
                    action.action
                );
            }
            assert_eq!(
                tool.input_schema,
                domain_tool_spec(&tool.name).unwrap().input_schema
            );
            assert_eq!(
                tool.input_schema["oneOf"].as_array().unwrap().len(),
                actual.len()
            );
            references(&tool.input_schema, &tool.input_schema);
        }
    }

    #[test]
    fn every_public_operation_has_one_unambiguous_domain_action() {
        let mut routes = HashSet::new();
        for tool in tool_summaries() {
            assert!(ToolDomain::ALL.contains(&tool.domain));
            assert!(!tool.action.is_empty());
            assert!(
                routes.insert((tool.domain, tool.action)),
                "duplicate domain action"
            );
            let payload = json!({"probe": tool.name});
            let call = json!({"action": tool.action, "input": payload});
            assert_eq!(
                resolve(&format!("opencut_{}", tool.domain.name()), call.clone()).unwrap(),
                (tool.name.to_owned(), payload)
            );
            assert_eq!(
                operation_name(&format!("opencut_{}", tool.domain.name()), &call).as_deref(),
                Some(tool.name)
            );
        }
        assert_eq!(routes.len(), tool_summaries().len());
    }

    #[test]
    fn domain_ownership_does_not_create_cross_domain_authorization_aliases() {
        let input = json!({"kind": "frame", "request": {}});
        assert_eq!(
            resolve("opencut_render", json!({"action":"submit","input":input})).unwrap(),
            ("opencut_render_submit".to_owned(), input.clone())
        );
        let wrong = json!({"action":"render.submit","input":input});
        assert_eq!(
            resolve("opencut_task", wrong.clone()).unwrap_err().code,
            "INVALID_ARGUMENT"
        );
        assert_eq!(operation_name("opencut_task", &wrong), None);
        assert_eq!(
            resolve("opencut_analysis", json!({"action":"asset","input":{}}))
                .unwrap()
                .0,
            "opencut_analysis_asset"
        );
        assert_eq!(
            resolve(
                "opencut_catalog",
                json!({"action":"font.catalog","input":{}})
            )
            .unwrap()
            .0,
            "opencut_font_catalog"
        );
    }
}

/// Default Agent discovery is small;explicit domain expansion uses the sole registry.
pub fn agent_catalog(
    domain: Option<&str>,
    schemas: bool,
) -> Result<Vec<DomainDescriptor>, EditorError> {
    // Initial context shows common editing actions. Selecting a domain expands
    // all of its registered operations,without a development-generation list.
    const CORE: &[&str] = &[
        "opencut_font_catalog",
        "opencut_catalog_resources_list",
        "opencut_project_inspect",
        "opencut_project_plugin_plan",
        "opencut_plugin_preset_plan",
        "opencut_capabilities",
        "opencut_project_list",
        "opencut_project_create",
        "opencut_project_summary",
        "opencut_project_timeline_query",
        "opencut_edit_agent_plan",
        "opencut_edit_recipe_plan",
        "opencut_edit_branch_create",
        "opencut_edit_branch_list",
        "opencut_edit_branch_compare",
        "opencut_edit_branch_merge",
        "opencut_edit_apply",
        "opencut_edit_undo_plan",
        "opencut_edit_undo_apply",
        "opencut_edit_redo_plan",
        "opencut_edit_redo_apply",
        "opencut_media_content_search",
        "opencut_media_map_time",
        "opencut_analysis_asset",
        "opencut_source_art_evidence",
        "opencut_render_capability",
        "opencut_render_submit",
        "opencut_job_view",
        "opencut_job_request_cancel",
        "opencut_artifact_get",
        "opencut_resource_read",
        "opencut_quality_inspect",
        "opencut_quality_evidence_plan",
        "opencut_access_inspect",
    ];
    let mut catalog = catalog(domain, schemas)?;
    for group in &mut catalog {
        if domain.is_none() {
            group
                .actions
                .retain(|action| CORE.contains(&action.operation.as_str()));
        }
    }
    Ok(catalog)
}
