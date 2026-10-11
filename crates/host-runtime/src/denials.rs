//! Native access to the shared refusal audit policy.
pub use opencut_automation::denials::*;
use opencut_contracts::EditorError;
use opencut_sqlite_store::EditorApplication;
use std::path::Path;

pub fn record_at(database: &Path, denial: &Denial<'_>) -> Result<(), EditorError> {
    EditorApplication::open_managed(database)?.record_denial(denial)
}
