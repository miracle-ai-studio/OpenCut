//! Private scope and capacity authority shared by frozen V1/V2 and font recovery.
use super::*;
use opencut_editor_application::{
    audit::ExecutionContext, edit_recovery_v1 as v1, edit_recovery_v2 as v2,
};

pub(super) struct Scope {
    pub principal: String,
    pub credential: String,
    pub owner: bool,
}
pub(super) fn scope(context: &ExecutionContext) -> Scope {
    let caller = context.caller();
    Scope {
        principal: caller.principal_id().into(),
        credential: match caller {
            access::Caller::LocalOwner => String::new(),
            access::Caller::Credential(c) => c.credential_id().into(),
        },
        owner: caller.is_owner(),
    }
}
pub(super) fn quota() -> EditorError {
    EditorError::new(
        "RECOVERY_QUOTA_EXCEEDED",
        "Unresolved recovery storage is full; resolve existing transactions first",
    )
}
pub(super) fn reserve(connection: &Connection, bytes: usize) -> Result<(), EditorError> {
    let (count, total): (u64, u64) = connection.query_row(
        "SELECT count(*),COALESCE(sum(stored_bytes),0) FROM
         (SELECT stored_bytes FROM edit_recovery_v1 UNION ALL SELECT stored_bytes FROM edit_recovery_v2)",
        [], |row| Ok((row.get(0)?, row.get(1)?)),
    ).map_err(storage_error)?;
    if bytes > v1::MAX_RECORD_BYTES
        || count >= v1::MAX_UNRESOLVED_RECORDS
        || total
            .checked_add(bytes as u64)
            .is_none_or(|value| value > v1::MAX_UNRESOLVED_BYTES)
    {
        return Err(quota());
    }
    Ok(())
}

pub(super) enum V2SlotKind {
    Edit { key: String },
    Font { key: String },
    CaptionV3 { key: String },
}
impl V2SlotKind {
    pub(super) fn key(&self) -> &str {
        match self {
            Self::Edit { key } | Self::Font { key } | Self::CaptionV3 { key } => key,
        }
    }
}
/// Inspect only a bounded, digest-verified payload. The private font envelope
/// never becomes an extra case in the frozen V2 public intent or discovery DTO.
pub(super) fn classify_v2_payload(
    payload: &str,
    digest: &str,
    project: &str,
) -> Result<V2SlotKind, EditorError> {
    if payload.len() > v1::MAX_RECORD_BYTES {
        return Err(corrupt());
    }
    let value: Value = decode_json(payload, "Stored recovery slot")?;
    if value.get("kind").and_then(Value::as_str) == Some("caption_edit_v3") {
        let input =
            opencut_editor_application::edit_recovery_v3::decode_payload(payload, digest, project)?;
        return Ok(V2SlotKind::CaptionV3 {
            key: opencut_editor_application::edit_recovery_v3::intent(&input.context)
                .key()
                .into(),
        });
    }
    if value.get("kind").is_some() {
        let input = opencut_editor_application::font_import_v1::recovery::decode_payload(
            payload, digest, project,
        )?;
        return Ok(V2SlotKind::Font {
            key: input.context.request.idempotency_key,
        });
    }
    if v2::exact_payload_digest(payload) != digest {
        return Err(corrupt());
    }
    let input: v2::StageEditRecoveryV2 = decode_json(payload, "Stored V2 recovery context")?;
    v2::prepare(&input).map_err(|_| corrupt())?;
    let intent = v2::intent(&input.context.intent);
    if intent.project_id() != project {
        return Err(corrupt());
    }
    Ok(V2SlotKind::Edit {
        key: intent.key().to_owned(),
    })
}
pub(super) fn font_protocol_required() -> EditorError {
    EditorError::new(
        "RECOVERY_PROTOCOL_REQUIRED",
        "This record requires font recovery V1 inspection and acknowledgement",
    )
}

/// Call inside the same immediate lock as insertion. Never convert or rewrite
/// the other protocol's raw JSON/digest in order to replace its unresolved edit.
pub(super) fn require_no_other_version(
    connection: &Connection,
    identity: &Scope,
    project: &str,
    key: &str,
    version: u32,
) -> Result<(), EditorError> {
    let table = match version {
        1 => "edit_recovery_v2",
        2 => "edit_recovery_v1",
        _ => unreachable!(),
    };
    let raw: Option<(String, String)> = connection.query_row(
        &format!("SELECT request_digest,context_json FROM {table} WHERE project_id=?1 AND principal_id=?2 AND credential_id=?3 AND owner=?4"),
        params![project, identity.principal, identity.credential, identity.owner],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).optional().map_err(storage_error)?;
    let Some((digest, payload)) = raw else {
        return Ok(());
    };
    if payload.len() > v1::MAX_RECORD_BYTES {
        return Err(corrupt());
    }
    let previous_key = if version == 1 {
        classify_v2_payload(&payload, &digest, project)?
            .key()
            .to_owned()
    } else {
        if v1::exact_payload_digest(&payload) != digest {
            return Err(corrupt());
        }
        let input: v1::StageEditRecoveryV1 = decode_json(&payload, "Stored V1 recovery context")?;
        let intent = v1::transaction(&input.context)?;
        if intent.project_id != project {
            return Err(corrupt());
        }
        intent.idempotency_key.clone()
    };
    Err(EditorError::new(
        if previous_key == key {
            "IDEMPOTENCY_KEY_REUSED"
        } else {
            "RECOVERY_CONFLICT"
        },
        "Resolve the original staged transaction before replacing it",
    ))
}
fn corrupt() -> EditorError {
    EditorError::new("STORAGE_ERROR", "Stored recovery context identity differs")
}

pub(super) fn caption_protocol_required() -> EditorError {
    EditorError::new(
        "RECOVERY_PROTOCOL_REQUIRED",
        "This original caption record requires edit recovery V3 inspection and acknowledgement",
    )
}
