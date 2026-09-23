//! Shared versioned request identities for persistence adapters. No I/O.
use opencut_contracts::EditorError;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Debug)]
pub struct RequestIdentity {
    pub(crate) stored: String,
    pub(crate) legacy: String,
}

pub fn request_identity<T: Serialize>(
    operation: &str,
    scope: &str,
    value: &T,
) -> Result<RequestIdentity, EditorError> {
    let legacy_bytes = serde_json::to_vec(&(operation, scope, value)).map_err(|error| {
        EditorError::new(
            "SERIALIZATION_ERROR",
            format!("could not hash request: {error}"),
        )
    })?;
    let mut canonical = serde_json::to_value((operation, scope, value)).map_err(|error| {
        EditorError::new(
            "SERIALIZATION_ERROR",
            format!("could not canonicalize request: {error}"),
        )
    })?;
    canonicalize_request_value(&mut canonical, false)?;
    let canonical_bytes = serde_json::to_vec(&canonical).map_err(|error| {
        EditorError::new(
            "SERIALIZATION_ERROR",
            format!("could not hash canonical request: {error}"),
        )
    })?;
    Ok(RequestIdentity {
        stored: format!("v2:{}", hex_digest(&canonical_bytes)),
        legacy: hex_digest(&legacy_bytes),
    })
}

fn canonicalize_request_value(value: &mut Value, opaque: bool) -> Result<(), EditorError> {
    if opaque {
        return Ok(());
    }
    match value {
        Value::Array(values) => {
            for value in values {
                canonicalize_request_value(value, false)?;
            }
        }
        Value::Object(object) => {
            if object.len() == 2 && object.contains_key("ticks") && object.contains_key("timebase")
            {
                let ticks = object
                    .get("ticks")
                    .and_then(Value::as_str)
                    .and_then(|ticks| ticks.parse::<i64>().ok());
                let timebase = object
                    .get("timebase")
                    .and_then(Value::as_u64)
                    .and_then(|timebase| u32::try_from(timebase).ok());
                if let (Some(ticks), Some(timebase)) = (ticks, timebase)
                    && timebase != 0
                {
                    let divisor = gcd_u64(ticks.unsigned_abs(), u64::from(timebase));
                    object.insert(
                        "ticks".into(),
                        Value::String((ticks / divisor as i64).to_string()),
                    );
                    object.insert(
                        "timebase".into(),
                        Value::Number(serde_json::Number::from(u64::from(timebase) / divisor)),
                    );
                    return Ok(());
                }
            }
            for (key, value) in object {
                canonicalize_request_value(
                    value,
                    matches!(
                        key.as_str(),
                        "params" | "metadata" | "value" | "parameters" | "details"
                    ),
                )?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn gcd_u64(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left.max(1)
}

pub(crate) fn content_id(prefix: &str, bytes: &[u8]) -> String {
    format!("{prefix}_{}", hex_digest(bytes))
}

fn hex_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

impl RequestIdentity {
    /// New receipts persist this versioned digest.
    pub fn current_digest(&self) -> &str {
        &self.stored
    }
    /// Required only to compare existing unversioned receipts.
    pub fn legacy_digest(&self) -> &str {
        &self.legacy
    }
    /// A recovered historical request proves semantic equivalence only if its
    /// legacy digest matches the receipt. Prepared audit inputs may not match
    /// the original public request; callers must not skip this proof.
    pub fn matches(&self, stored_digest: &str, recovered: Option<&Self>) -> bool {
        stored_digest == self.stored
            || stored_digest == self.legacy
            || (!stored_digest.starts_with("v2:")
                && recovered
                    .is_some_and(|old| old.legacy == stored_digest && old.stored == self.stored))
    }
}
