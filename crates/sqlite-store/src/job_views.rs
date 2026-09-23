//! Native facade delegates cursor and view behavior to portable query services.
use super::{EditorApplication, queries};
use opencut_api_contracts::job_views::*;
use opencut_contracts::EditorError;
pub use opencut_editor_application::job_views::*;
impl EditorApplication {
    pub fn job_view(&self, id: &str) -> Result<JobView, EditorError> {
        queries::job_view(&self.connection, id)
    }
    pub fn job_query(&self, input: JobQueryInput) -> Result<JobPage, EditorError> {
        queries::job_query(&self.connection, input)
    }
    pub fn job_changes(&self, input: JobChangesInput) -> Result<JobChangesPage, EditorError> {
        queries::job_changes(&self.connection, input)
    }
}
