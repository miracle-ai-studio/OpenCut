//! Local byte-client adapter. File paths never enter an editor/tool request.
use clap::Subcommand;
use opencut_automation::{CallSource, Caller, Response, uploads};
use opencut_contracts::EditorError;
use opencut_host_runtime::Automation;
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
    let (chunk, bytes) = read_chunk(command)?;
    let mut host = Automation::open(database)?
        .with_source(CallSource::Cli)
        .with_caller(caller);
    Ok(host.put_upload_chunk(&chunk, &bytes))
}

pub(super) fn execute_remote(
    client: &opencut_sdk::Client,
    command: &Command,
) -> Result<Response, EditorError> {
    let (chunk, bytes) = read_chunk(command)?;
    let result = client.upload_chunk(&chunk.upload_id, uploads::decimal(&chunk.offset)?, &bytes);
    Ok(match result {
        Ok(view) => opencut_automation::response(Ok(serde_json::json!(view))),
        Err(error) => crate::connection::response::<serde_json::Value>(Err(error)),
    })
}

fn read_chunk(command: &Command) -> Result<(uploads::ChunkDescriptor, Vec<u8>), EditorError> {
    let (upload_id, offset, input) = match command {
        Command::Put {
            upload_id,
            offset,
            input,
        } => (upload_id, offset, input),
    };
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
    Ok((chunk, bytes))
}
