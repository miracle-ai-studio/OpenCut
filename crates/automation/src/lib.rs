//! Transport-independent dispatcher. Concrete services are injected by a host.
pub use opencut_api_contracts::tools::{ToolSpec, tool_spec, tool_specs};
pub use opencut_api_contracts::*;
pub use opencut_api_contracts::{context, domains, event_stream, preview};
use opencut_contracts::EditorError;
pub use opencut_editor_application::access::Caller;
pub use opencut_editor_application::audit::{CallSource, ExecutionContext};
use opencut_editor_application::tool_execution::ToolExecution;
pub use opencut_editor_application::uploads;
use serde_json::Value;
pub mod capabilities;
pub mod denials;
mod error_policy;
pub use capabilities::capabilities;
pub use error_policy::{ErrorPolicy, error_policy};

pub fn response(result: Result<Value, EditorError>) -> Response {
    Response::from_result_with_id(uuid::Uuid::new_v4().to_string(), result)
}
pub fn exit_code(error: &EditorError) -> i32 {
    error_policy(&error.code).exit_code
}

/// Revocation, size, domain resolution, authorization and decoding have one order
/// for every transport. No filesystem, database or native service is constructed.
pub fn dispatch(
    backend: &mut impl ToolExecution,
    context: ExecutionContext,
    name: &str,
    arguments: Value,
) -> Response {
    let operation = domains::operation_name(name, &arguments);
    let leaf = operation.as_deref().unwrap_or(name);
    let result = (|| {
        backend.begin(context.clone())?;
        if serde_json::to_vec(&arguments)
            .map_err(|e| EditorError::new("INVALID_ARGUMENT", e.to_string()))?
            .len()
            > 2_097_152
        {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Control request exceeds 2 MiB",
            ));
        }
        let (name, arguments) = domains::resolve(name, arguments)?;
        opencut_editor_application::access::authorize_tool(context.caller(), &name)?;
        backend.execute_tool(tools::ToolRequest::decode(&name, arguments)?)
    })();
    let mut response = Response::from_result_with_id(context.request_id().into(), result);
    if matches!(
        leaf,
        "opencut_project_v3_package_export"
            | "opencut_project_v3_package_import"
            | "opencut_project_package_export"
            | "opencut_project_package_import"
    ) && response
        .data
        .as_ref()
        .and_then(|v| v.get("fully_reproducible"))
        .and_then(Value::as_bool)
        == Some(true)
    {
        response
            .warnings
            .push("LEGACY_REPRODUCIBILITY_CLAIM_UNVERIFIED".into());
    }
    if response.error.as_ref().is_some_and(denials::is_denial)
        && backend
            .record_tool_denial(&denials::Denial {
                request_id: context.request_id(),
                source: context.actor().source,
                caller: Some(context.caller()),
                operation: denials::operation(leaf),
                reason: denials::DenialReason::AuthorizationDenied,
            })
            .is_err()
    {
        response.warnings.push(denials::FAILURE_WARNING.into());
    }
    response
}

#[cfg(test)]
mod port_tests {
    use super::*;
    use opencut_editor_application::access::VerifiedCredential;
    use std::cell::Cell;
    #[derive(Default)]
    struct MemoryBackend {
        began: usize,
        executed: usize,
        revoked: bool,
        denials: Cell<usize>,
    }
    impl ToolExecution for MemoryBackend {
        fn begin(&mut self, _: ExecutionContext) -> Result<(), EditorError> {
            self.began += 1;
            if self.revoked {
                Err(opencut_editor_application::access::denied())
            } else {
                Ok(())
            }
        }
        fn execute_tool(&mut self, request: tools::ToolRequest) -> Result<Value, EditorError> {
            self.executed += 1;
            match request {
                tools::ToolRequest::ProjectInspect(input) => {
                    Ok(serde_json::json!({"project_id":input.project_id}))
                }
                _ => Ok(Value::Null),
            }
        }
        fn record_tool_denial(&self, _: &denials::Denial<'_>) -> Result<(), EditorError> {
            self.denials.set(self.denials.get() + 1);
            Ok(())
        }
    }
    #[test]
    fn dispatch_uses_injected_memory_backend_and_checks_before_execution() {
        let mut backend = MemoryBackend::default();
        let call = |b: &mut MemoryBackend, name, value| {
            dispatch(
                b,
                ExecutionContext::local(CallSource::Embedded),
                name,
                value,
            )
        };
        let response = call(
            &mut backend,
            "opencut_project_inspect",
            serde_json::json!({"project_id":"p"}),
        );
        assert!(response.error.is_none());
        assert_eq!(backend.executed, 1);
        assert!(
            call(
                &mut backend,
                "opencut_project_inspect",
                serde_json::json!({})
            )
            .error
            .is_some()
        );
        assert_eq!(backend.executed, 1);
        backend.revoked = true;
        let denied = call(&mut backend, "opencut_capabilities", Value::Null);
        assert_eq!(denied.error.unwrap().code, "NOT_AUTHORIZED");
        assert_eq!(backend.denials.get(), 1);
        assert_eq!(backend.executed, 1);
    }
    #[test]
    fn owner_only_leaf_cannot_be_called_through_injected_backend() {
        let mut backend = MemoryBackend::default();
        let caller = Caller::Credential(VerifiedCredential::from_verified("p".into(), "c".into()));
        let result = dispatch(
            &mut backend,
            ExecutionContext::for_caller(CallSource::Http, caller),
            "opencut_project_package_reproducibility_v1",
            Value::Null,
        );
        assert_eq!(result.error.unwrap().code, "NOT_AUTHORIZED");
        assert_eq!(backend.executed, 0);
        assert_eq!(backend.denials.get(), 1);
    }
}
