//! CLI transport selection. Both paths submit the same registered operation.
use opencut_automation::{Caller, Response};
use opencut_contracts::EditorError;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone)]
pub enum Connection {
    Local { database: PathBuf, caller: Caller },
    Remote(Arc<opencut_sdk::Client>),
}

pub fn remote(
    endpoint: &str,
    credential: Option<&Path>,
) -> Result<Arc<opencut_sdk::Client>, EditorError> {
    let path = credential.ok_or_else(|| {
        EditorError::new(
            "INVALID_ARGUMENT",
            "--endpoint requires --credential-file for the host bearer token",
        )
    })?;
    if std::fs::metadata(path)
        .map_err(|_| EditorError::new("INVALID_ARGUMENT", "Could not read credential file"))?
        .len()
        > 16_384
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Credential file is too large",
        ));
    }
    let token = std::fs::read_to_string(path)
        .map_err(|_| EditorError::new("INVALID_ARGUMENT", "Could not read credential file"))?;
    if token.trim().is_empty() {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Credential file is empty",
        ));
    }
    opencut_sdk::Client::new(endpoint, token.trim())
        .map(Arc::new)
        .map_err(|e| EditorError::new("INVALID_ARGUMENT", e.to_string()))
}

pub fn response<T: serde::Serialize>(
    result: Result<opencut_sdk::Success<T>, opencut_sdk::Error>,
) -> Response {
    match result {
        Ok(result) => {
            let data = serde_json::to_value(result.data).map_err(|_| {
                EditorError::new("INVALID_RESPONSE", "Operation result could not be encoded")
            });
            let mut response = Response::from_result_with_id(result.request_id, data);
            response.warnings = result.warnings;
            response
        }
        Err(opencut_sdk::Error::Api(failure)) => {
            let mut response =
                Response::from_result_with_id(failure.request_id, Err(failure.error));
            response.warnings = failure.warnings;
            response
        }
        Err(opencut_sdk::Error::Configuration(message)) => {
            opencut_automation::response(Err(EditorError::new("INVALID_ARGUMENT", message)))
        }
        Err(error) => opencut_automation::response(Err(EditorError::new(
            "TRANSPORT_ERROR",
            error.to_string(),
        ))),
    }
}

impl Connection {
    pub fn call(&self, name: &str, input: Value) -> Response {
        match self {
            Self::Local { database, caller } => {
                match opencut_host_runtime::Automation::open(database) {
                    Ok(app) => app
                        .with_source(opencut_automation::CallSource::Cli)
                        .with_caller(caller.clone())
                        .call(name, input),
                    Err(error) => opencut_automation::response(Err(error)),
                }
            }
            Self::Remote(client) => response(client.call_registered(name, &input)),
        }
    }

    pub fn data<T: DeserializeOwned>(&self, name: &str, input: Value) -> Result<T, EditorError> {
        let response = self.call(name, input);
        if let Some(error) = response.error {
            return Err(error);
        }
        serde_json::from_value(
            response
                .data
                .ok_or_else(|| EditorError::new("INVALID_RESPONSE", "Missing operation result"))?,
        )
        .map_err(|_| EditorError::new("INVALID_RESPONSE", "Operation returned an invalid result"))
    }
}
