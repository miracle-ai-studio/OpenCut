//! Host injection boundary for typed, registry-backed automation operations.
use crate::{audit::ExecutionContext, denials::Denial};
use opencut_api_contracts::tools::ToolRequest;
use opencut_contracts::EditorError;
use serde_json::Value;

pub trait ToolExecution {
    /// Bind trusted context and recheck credentials before even malformed requests.
    fn begin(&mut self, context: ExecutionContext) -> Result<(), EditorError>;
    /// Inputs are decoded before crossing this boundary. Native composition lives
    /// in the host adapter, never in the transport dispatcher.
    fn execute_tool(&mut self, request: ToolRequest) -> Result<Value, EditorError>;
    fn record_tool_denial(&self, denial: &Denial<'_>) -> Result<(), EditorError>;
}
