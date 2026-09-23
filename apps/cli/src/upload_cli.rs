//! Local byte-client adapter. File paths never enter an editor/tool request.
use clap::Subcommand;
use opencut_automation::{Automation, CallSource, Caller, Response, uploads};
use opencut_contracts::EditorError;
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
};
#[derive(Subcommand)]
pub(super) enum Command {
    /// Read one local raw chunk (or stdin) and confirm it at an explicit upload offset.
    Put {
        upload_id: String,
        #[arg(long)]
        offset: String,
        #[arg(long)]
        input: PathBuf,
    },
}
pub(super) fn execute(
    database: &Path,
    caller: Caller,
    command: &Command,
) -> Result<Response, EditorError> {
    let Command::Put {
        upload_id,
        offset,
        input,
    } = command;
    uploads::validate_id(upload_id)?;
    uploads::decimal(offset)?;
    let reader: Box<dyn Read> = if input.as_os_str() == "-" {
        Box::new(std::io::stdin())
    } else {
        Box::new(std::fs::File::open(input).map_err(|_| {
            EditorError::new("INVALID_ARGUMENT", "Local chunk input could not be read")
        })?)
    };
    let mut bytes = Vec::new();
    reader
        .take(uploads::MAX_CHUNK_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| EditorError::new("INVALID_ARGUMENT", "Local chunk input could not be read"))?;
    let chunk = uploads::ChunkDescriptor {
        upload_id: upload_id.clone(),
        offset: offset.clone(),
        size_bytes: bytes.len().to_string(),
        content_hash: format!("{:x}", Sha256::digest(&bytes)),
    };
    uploads::validate_chunk(&chunk)?;
    Ok(Automation::open(database)?
        .with_source(CallSource::Cli)
        .with_caller(caller)
        .put_upload_chunk(&chunk, &bytes))
}
