//! V5 has independent replay identity while reusing the fixed-resource,
//! permission, CAS and atomic first-revision publication implementation.
use super::*;
use opencut_api_contracts::classic_import_v5::*;
use sqlite_classic_import_v4::{ImportVersion, resolve_version};

impl EditorApplication {
    pub fn classic_v5_analyze(
        &self,
        input: &ClassicAnalyzeV5,
    ) -> Result<ClassicAnalysisV5, EditorError> {
        let resources = resolve_version(
            &self.connection,
            &self.connection.1,
            input,
            ImportVersion::V5,
        )?;
        Ok(opencut_editor_application::classic_import_v5::analyze(input, &resources)?.analysis)
    }
    pub fn classic_v5_import(
        &mut self,
        input: &ClassicImportV5,
    ) -> Result<ClassicImportReceiptV5, EditorError> {
        self.classic_import_version(input, ImportVersion::V5)
    }
}

#[cfg(test)]
mod tests;
