//! Conservative, bounded reference roots for published Job artifacts.
//! Publication provenance (the producing Job result), audit records and
//! idempotency receipts are not live dependency roots. A collector must also
//! resolve `native_paths` by inode under its publication gate before deletion.
use super::*;
use opencut_api_contracts::artifact_retention::{ReferenceKind, ReferenceRoot};
use opencut_editor_application::gc::{
    MAX_REFERENCE_BYTES, MAX_REFERENCE_DOCUMENT_BYTES, MAX_REFERENCE_MS, MAX_REFERENCE_ROWS,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};
use url::Url;

pub(super) struct References {
    pub roots: BTreeSet<ReferenceRoot>,
    pub native_paths: BTreeMap<String, BTreeSet<ReferenceRoot>>,
    pub examined_rows: usize,
}

fn limit() -> EditorError {
    EditorError::new(
        "RESOURCE_TOO_LARGE",
        "Artifact reference scan exceeded its bound; collection must not proceed",
    )
}

fn candidate_path(value: &str) -> Result<Option<String>, EditorError> {
    let path = if value.starts_with("file:") {
        let url = Url::parse(value).map_err(|_| invalid_native_reference())?;
        if url.scheme() != "file" || url.query().is_some() || url.fragment().is_some() {
            return Err(invalid_native_reference());
        }
        url.to_file_path()
            .map_err(|_| invalid_native_reference())?
            .to_str()
            .ok_or_else(invalid_native_reference)?
            .to_owned()
    } else {
        value.to_owned()
    };
    if path.chars().any(char::is_control) && Path::new(&path).is_absolute() {
        return Err(invalid_native_reference());
    }
    Ok(Path::new(&path).is_absolute().then_some(path))
}

fn invalid_native_reference() -> EditorError {
    EditorError::new(
        "INVALID_STORED_REFERENCE",
        "Stored native source reference cannot be resolved",
    )
}

fn inspect_value(
    value: &Value,
    artifact_id: &str,
    published_path: &str,
    native_paths: &mut BTreeSet<String>,
) -> Result<bool, EditorError> {
    let mut matched = false;
    let mut pending = vec![value];
    while let Some(current) = pending.pop() {
        match current {
            Value::String(text) => {
                matched |= text.contains(artifact_id) || text.contains(published_path);
                if let Some(path) = candidate_path(text)? {
                    native_paths.insert(path);
                }
            }
            Value::Array(values) => pending.extend(values),
            Value::Object(values) => {
                for (key, value) in values {
                    matched |= key.contains(artifact_id) || key.contains(published_path);
                    if let Some(path) = candidate_path(key)? {
                        native_paths.insert(path);
                    }
                    pending.push(value);
                }
            }
            _ => {}
        }
    }
    Ok(matched)
}

pub(super) fn scan(
    connection: &Connection,
    producing_job_id: &str,
    artifact_id: &str,
    published_path: &str,
) -> Result<References, EditorError> {
    if !Path::new(published_path).is_absolute() {
        return Err(EditorError::new(
            "INVALID_STORED_RESULT",
            "Published artifact path is not absolute",
        ));
    }
    let mut roots = BTreeSet::new();
    let mut native_paths: BTreeMap<String, BTreeSet<ReferenceRoot>> = BTreeMap::new();
    let mut examined_rows = 0usize;
    let mut examined_bytes = 0usize;
    let started = Instant::now();
    // Keep this list aligned with every persisted authority that may express
    // a media dependency. Journal, audit and request replay rows describe past
    // actions, not ongoing use of the bytes.
    for (kind, sql) in [
        (
            ReferenceKind::Revision,
            "SELECT revision,document_json FROM revisions",
        ),
        (
            ReferenceKind::JobInput,
            "SELECT job_id,input_json FROM jobs",
        ),
        (
            ReferenceKind::JobResult,
            "SELECT job_id,result_json FROM jobs WHERE result_json IS NOT NULL",
        ),
        (
            ReferenceKind::ProjectMedia,
            "SELECT media_id,asset_json FROM project_media",
        ),
        (
            ReferenceKind::PluginPublication,
            "SELECT publication_id,object_json FROM plugin_publications",
        ),
        (
            ReferenceKind::PluginPublication,
            "SELECT publication_id,intent_json FROM plugin_publications",
        ),
        (
            ReferenceKind::ProjectPluginGrant,
            "SELECT project_id || ':' || plugin_id || ':' || version,grant_json FROM project_plugin_grants",
        ),
    ] {
        let mut statement = connection.prepare(sql).map_err(storage_error)?;
        let mut rows = statement.query([]).map_err(storage_error)?;
        while let Some(row) = rows.next().map_err(storage_error)? {
            let id: String = row.get(0).map_err(storage_error)?;
            let document: String = row.get(1).map_err(storage_error)?;
            examined_rows += 1;
            examined_bytes = examined_bytes.saturating_add(document.len());
            if examined_rows > MAX_REFERENCE_ROWS
                || document.len() > MAX_REFERENCE_DOCUMENT_BYTES
                || examined_bytes > MAX_REFERENCE_BYTES
                || started.elapsed().as_millis() > u128::from(MAX_REFERENCE_MS)
            {
                return Err(limit());
            }
            let value: Value = decode_json(&document, "artifact reference root")?;
            // The producing result is the publication fact. Counting it as a
            // dependency would make explicit release permanently ineligible.
            if kind == ReferenceKind::JobResult && id == producing_job_id {
                continue;
            }
            let root = ReferenceRoot {
                kind,
                resource_id: id,
            };
            let mut document_paths = BTreeSet::new();
            if inspect_value(&value, artifact_id, published_path, &mut document_paths)? {
                roots.insert(root.clone());
            }
            for path in document_paths {
                native_paths.entry(path).or_default().insert(root.clone());
            }
            if started.elapsed().as_millis() > u128::from(MAX_REFERENCE_MS) {
                return Err(limit());
            }
        }
    }
    Ok(References {
        roots,
        native_paths,
        examined_rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn scans_live_authorities_but_not_the_producing_result() {
        let app = EditorApplication::open(":memory:").unwrap();
        let artifact = format!("artifact:job_{}:{}", "a".repeat(32), "b".repeat(64));
        let producer = format!("job_{}", "a".repeat(32));
        let path = "/workspace/artifacts/producer/attempt-1/render.mp4";
        app.connection
            .execute(
                "INSERT INTO projects(project_id,name,head_revision) VALUES ('p','Project','r')",
                [],
            )
            .unwrap();
        app.connection
            .execute(
                "INSERT INTO revisions(revision,project_id,document_json) VALUES ('r','p',?1)",
                [json!({"assets":[{"uri":format!("file://{path}")}]}).to_string()],
            )
            .unwrap();
        app.connection.execute("INSERT INTO jobs(job_id,state,attempt,kind,input_json,result_json,cancel_requested) VALUES (?1,'succeeded',1,'render.export','{}',?2,0)",params![producer,json!({"artifact":{"path":path}}).to_string()]).unwrap();
        app.connection.execute("INSERT INTO jobs(job_id,state,attempt,kind,input_json,result_json,cancel_requested) VALUES ('consumer','queued',0,'render.export',?1,NULL,0)",[json!({"segments":[{"input":path}]}).to_string()]).unwrap();
        app.connection.execute("INSERT INTO jobs(job_id,state,attempt,kind,input_json,result_json,cancel_requested) VALUES ('derived','succeeded',1,'artifact.verify','{}',?1,0)",[json!({"source":artifact}).to_string()]).unwrap();
        app.connection.execute("INSERT INTO upload_sessions(upload_id,project_id,idempotency_key,request_hash,name,size_bytes,content_hash,next_offset) VALUES ('u','p','k','h','asset',1,'h',1)",[]).unwrap();
        app.connection.execute("INSERT INTO project_media(media_id,project_id,upload_id,job_id,asset_json) VALUES ('m','p','u',?1,?2)",params![producer,json!({"source":path}).to_string()]).unwrap();
        app.connection.execute("INSERT INTO plugin_publications(publication_id,object_json,intent_json,origin_request_id,pending) VALUES ('pub',?1,'{}','req',0)",[json!({"input":artifact}).to_string()]).unwrap();
        app.connection.execute("INSERT INTO project_plugin_grants(project_id,plugin_id,version,revision,grant_json) VALUES ('p','plugin','1',1,?1)",[json!({"source":artifact}).to_string()]).unwrap();

        let found = scan(&app.connection, &producer, &artifact, path).unwrap();
        let expected = [
            (ReferenceKind::Revision, "r"),
            (ReferenceKind::JobInput, "consumer"),
            (ReferenceKind::JobResult, "derived"),
            (ReferenceKind::ProjectMedia, "m"),
            (ReferenceKind::PluginPublication, "pub"),
            (ReferenceKind::ProjectPluginGrant, "p:plugin:1"),
        ];
        assert_eq!(
            found.roots,
            expected
                .into_iter()
                .map(|(kind, resource_id)| ReferenceRoot {
                    kind,
                    resource_id: resource_id.into(),
                })
                .collect()
        );
        assert_eq!(found.examined_rows, 10);
        assert!(found.native_paths.contains_key(path));
        assert!(
            !found.roots.iter().any(|root| {
                root.kind == ReferenceKind::JobResult && root.resource_id == producer
            })
        );
    }

    #[test]
    fn malformed_authoritative_document_fails_closed() {
        let app = EditorApplication::open(":memory:").unwrap();
        app.connection
            .execute(
                "INSERT INTO projects(project_id,name,head_revision) VALUES ('p','Project','r')",
                [],
            )
            .unwrap();
        app.connection.execute("INSERT INTO revisions(revision,project_id,document_json) VALUES ('r','p','{broken')",[]).unwrap();
        assert!(
            scan(
                &app.connection,
                "producer",
                "artifact:producer:hash",
                "/artifact.mp4"
            )
            .is_err()
        );
    }

    #[test]
    fn file_uri_decodes_aliases_and_rejects_ambiguous_authority() {
        assert_eq!(
            candidate_path("file:///workspace/clip%20one.mp4").unwrap(),
            Some("/workspace/clip one.mp4".into())
        );
        assert_eq!(
            candidate_path("/workspace/clip%20one.mp4").unwrap(),
            Some("/workspace/clip%20one.mp4".into())
        );
        assert!(candidate_path("file://remote.example/workspace/clip.mp4").is_err());
        assert!(candidate_path("file:///workspace/clip.mp4#fragment").is_err());
    }
}
