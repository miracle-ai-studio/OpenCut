//! Versioned Classic import semantics over the unchanged bounded request shape.
//! V5 reports schema_version 5; its proposed project remains the V4 aggregate.
pub use crate::classic_import_v4::{
    ClassicAnalysisV4 as ClassicAnalysisV5, ClassicAnalyzeV4 as ClassicAnalyzeV5,
    ClassicImportReceiptV4 as ClassicImportReceiptV5, ClassicImportV4 as ClassicImportV5,
};
