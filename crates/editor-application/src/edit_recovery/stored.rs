//! Read-only normalization of persisted intents. No historical write protocol.
//! Verify original raw bytes/digest before projecting one canonical context.
use super::*;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FontContext {
    request: opencut_contracts::font_import::ImportFont,
    sequence_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum FontPayload {
    #[serde(rename = "font_import_v1")]
    Static { context: FontContext },
    #[serde(rename = "derived_font_import_v1")]
    Derived { context: FontContext },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TimelinePayload {
    context: TimelineContext,
}
#[derive(Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum TimelineContext {
    #[serde(rename = "timeline_v4")]
    Timeline {
        request: opencut_contracts::timeline::TimelineEditRequest,
        plan: opencut_contracts::timeline::TimelineEditPlan,
    },
}
fn original_digest(payload: &str, domain: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update(payload.as_bytes());
    format!("sha256:{:x}", hash.finalize())
}
pub fn decode_payload(
    payload: &str,
    digest: &str,
    project: &str,
) -> Result<StageEditRecovery, EditorError> {
    if payload.len() > MAX_RECORD_BYTES {
        return Err(corrupt());
    }
    let request = if exact_payload_digest(payload) == digest {
        serde_json::from_str(payload).map_err(|_| corrupt())?
    } else if original_digest(payload, b"opencut.edit-recovery.v2\0") == digest {
        serde_json::from_str(payload).map_err(|_| corrupt())?
    } else if original_digest(payload, b"opencut.edit-recovery.v1\0") == digest {
        let old: TimelinePayload = serde_json::from_str(payload).map_err(|_| corrupt())?;
        let TimelineContext::Timeline { request, plan } = old.context;
        let transaction = plan.transaction.ok_or_else(corrupt)?;
        let identity = crate::identity::request_identity(
            "project_v4_timeline",
            &request.project_id,
            &request,
        )?;
        if transaction.project_id != request.project_id
            || transaction.expected_revision != request.expected_revision
            || transaction.idempotency_key != request.idempotency_key
            || transaction.label != request.label
            || plan.original_request_digest != identity.current_digest()
        {
            return Err(corrupt());
        }
        StageEditRecovery {
            context: EditRecoveryContext {
                intent: EditRecoveryIntent::PublicApply {
                    request: Box::new(transaction),
                },
                sequence_id: Some(request.sequence_id),
            },
        }
    } else if original_digest(payload, b"opencut.edit-recovery.v3\0") == digest {
        let mut value: serde_json::Value = serde_json::from_str(payload).map_err(|_| corrupt())?;
        if value.as_object().is_none_or(|o| o.len() != 2)
            || value.get("kind").and_then(|v| v.as_str()) != Some("caption_edit_v3")
        {
            return Err(corrupt());
        }
        let context = value.get_mut("context").ok_or_else(corrupt)?;
        let tag = context
            .get_mut("intent")
            .and_then(serde_json::Value::as_object_mut)
            .and_then(|intent| intent.get_mut("kind"))
            .ok_or_else(corrupt)?;
        *tag = match tag.as_str() {
            Some("subtitle_generate_v4") => serde_json::json!("subtitle_generate"),
            Some("subtitle_import_v4") => serde_json::json!("subtitle_import"),
            _ => return Err(corrupt()),
        };
        serde_json::from_value(serde_json::json!({"context":context})).map_err(|_| corrupt())?
    } else if original_digest(payload, b"opencut.font-recovery.v1\0") == digest
        || original_digest(payload, b"opencut.derived-font-recovery.v1\0") == digest
    {
        let old: FontPayload = serde_json::from_str(payload).map_err(|_| corrupt())?;
        let (context, derived, domain): (_, _, &[u8]) = match old {
            FontPayload::Static { context } => (context, false, b"opencut.font-recovery.v1\0"),
            FontPayload::Derived { context } => {
                (context, true, b"opencut.derived-font-recovery.v1\0")
            }
        };
        if original_digest(payload, domain) != digest
            || crate::font_catalog::selection::validate(&context.request)
                .map_err(|_| corrupt())?
                .derived()
                != derived
        {
            return Err(corrupt());
        }
        StageEditRecovery {
            context: EditRecoveryContext {
                intent: EditRecoveryIntent::FontImport {
                    request: context.request,
                },
                sequence_id: context.sequence_id,
            },
        }
    } else {
        return Err(corrupt());
    };
    validate_context(&request.context).map_err(|_| corrupt())?;
    if intent(&request.context.intent)?.project_id() != project {
        return Err(corrupt());
    }
    Ok(request)
}
