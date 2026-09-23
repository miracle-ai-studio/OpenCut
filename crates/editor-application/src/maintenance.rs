//! Publication protocol. Native adapters hold the object lock while invoking these ports.
use crate::access::{Caller, denied};
pub use opencut_api_contracts::maintenance::*;
use opencut_contracts::EditorError;

pub use opencut_api_contracts::plugin_publication;
use serde::{Serialize, de::DeserializeOwned};
mod sealed {
    pub trait Sealed {}
}
/// Closed native publication families share workflow without mixing wire types or cursors.
pub trait PublicationObject: sealed::Sealed + Clone + Serialize + DeserializeOwned {
    type Action: Clone + Serialize + DeserializeOwned;
    type Coverage;
    const RECONCILE_TOOL: &'static str;
    fn coverage() -> Self::Coverage;
    fn validate(&self) -> Result<(), EditorError>;
    fn matches(&self, action: &Self::Action, after: &str) -> bool;
}
fn valid_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl sealed::Sealed for MaintenanceObject {}
impl PublicationObject for MaintenanceObject {
    type Action = MaintenanceAction;
    type Coverage = MaintenanceCoverage;
    const RECONCILE_TOOL: &'static str = "opencut_maintenance_reconcile";
    fn coverage() -> Self::Coverage {
        MaintenanceCoverage::RecordedPublicationsSinceSchema6
    }
    fn validate(&self) -> Result<(), EditorError> {
        let hash = match self {
            Self::MediaContent { content_hash } | Self::MediaMetadata { content_hash } => {
                content_hash
            }
        };
        if !valid_hash(hash) {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Invalid maintenance content identity",
            ));
        }
        Ok(())
    }
    fn matches(&self, action: &Self::Action, after: &str) -> bool {
        match (self, action) {
            (Self::MediaContent { content_hash }, MaintenanceAction::MediaContentPublish) => {
                content_hash == after
            }
            (
                Self::MediaMetadata { .. },
                MaintenanceAction::MediaMetadataPublish | MaintenanceAction::MediaMetadataRepair,
            ) => true,
            _ => false,
        }
    }
}
impl sealed::Sealed for plugin_publication::Object {}
impl PublicationObject for plugin_publication::Object {
    type Action = plugin_publication::Action;
    type Coverage = plugin_publication::Coverage;
    const RECONCILE_TOOL: &'static str = "opencut_plugin_publication_reconcile";
    fn coverage() -> Self::Coverage {
        plugin_publication::Coverage::RecordedPluginPublicationsSinceSchema7
    }
    fn validate(&self) -> Result<(), EditorError> {
        let (id, version, hash) = match self {
            Self::PluginPackage {
                plugin_id,
                version,
                package_sha256,
            }
            | Self::PluginGrants {
                plugin_id,
                version,
                package_sha256,
            } => (plugin_id, version, package_sha256),
        };
        if id.len() > 255
            || !id.contains('.')
            || !id.split('.').all(|s| {
                !s.is_empty()
                    && s.bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            })
            || version.len() > 128
            || semver::Version::parse(version).is_err()
            || !valid_hash(hash)
        {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Invalid plugin publication identity",
            ));
        }
        Ok(())
    }
    fn matches(&self, action: &Self::Action, after: &str) -> bool {
        match (self, action) {
            (
                Self::PluginPackage { package_sha256, .. },
                plugin_publication::Action::PluginPackagePublish,
            ) => package_sha256 == after,
            (Self::PluginGrants { .. }, plugin_publication::Action::PluginGrantsUpdate) => true,
            _ => false,
        }
    }
}
pub type Intent<O> = PublicationIntent<O, <O as PublicationObject>::Action>;
pub type Event<O> = MaintenanceEvent<O, <O as PublicationObject>::Action>;
pub type Page<O> =
    MaintenancePage<O, <O as PublicationObject>::Action, <O as PublicationObject>::Coverage>;
pub fn validate_object<O: PublicationObject>(object: &O) -> Result<(), EditorError> {
    object.validate()
}
pub fn validate_intent<O: PublicationObject>(intent: &Intent<O>) -> Result<(), EditorError> {
    intent.object.validate()?;
    if !intent.object.matches(&intent.action, &intent.after_hash)
        || !valid_hash(&intent.after_hash)
        || intent
            .before_hash
            .as_deref()
            .is_some_and(|h| !valid_hash(h))
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Invalid publication action or fingerprints",
        ));
    }
    Ok(())
}
/// All methods must persist before returning. Native callers hold the same
/// per-object OS lock through reconcile, prepare, file publication and complete.
pub trait PublicationJournal<O: PublicationObject = MaintenanceObject> {
    fn reconcile(&self, object: &O, observed: Option<&str>) -> Result<Vec<String>, EditorError>;
    fn prepare(&self, intent: &Intent<O>) -> Result<String, EditorError>;
    fn complete(&self, publication_id: &str) -> Result<(), EditorError>;
}
/// Explicit opt-out for standalone native libraries with no workspace journal.
/// Public hosts must supply their actual journal, never this adapter.
pub struct UntrackedPublication;
impl<O: PublicationObject> PublicationJournal<O> for UntrackedPublication {
    fn reconcile(&self, _: &O, _: Option<&str>) -> Result<Vec<String>, EditorError> {
        Ok(vec![])
    }
    fn prepare(&self, _: &Intent<O>) -> Result<String, EditorError> {
        Ok(String::new())
    }
    fn complete(&self, _: &str) -> Result<(), EditorError> {
        Ok(())
    }
}
pub fn publish<T, O: PublicationObject>(
    journal: &dyn PublicationJournal<O>,
    intent: Intent<O>,
    effect: impl FnOnce() -> Result<T, EditorError>,
) -> Result<T, EditorError> {
    validate_intent(&intent)?;
    let publication_id = journal.prepare(&intent)?;
    let result = effect();
    match result {
        Ok(value) => {
            journal
                .complete(&publication_id)
                .map_err(|cause| unknown::<O>(&publication_id, cause))?;
            Ok(value)
        }
        Err(cause) if publication_id.is_empty() => Err(cause),
        Err(cause) => Err(unknown::<O>(&publication_id, cause)),
    }
}
fn unknown<O: PublicationObject>(id: &str, cause: EditorError) -> EditorError {
    EditorError::new("PUBLICATION_OUTCOME_UNKNOWN", "File publication may have occurred; inspect and reconcile the recorded object before retrying")
        .with_details(serde_json::json!({"publication_id":id,"cause":cause,"next_operation":O::RECONCILE_TOOL}))
}
pub trait MaintenanceRead<O: PublicationObject = MaintenanceObject> {
    fn read_maintenance(
        &self,
        target: &MaintenanceTarget<O>,
        after: i64,
        limit: usize,
    ) -> Result<Vec<Event<O>>, EditorError>;
}
pub fn query<O: PublicationObject>(
    store: &impl MaintenanceRead<O>,
    caller: &Caller,
    input: MaintenanceQuery<O>,
) -> Result<Page<O>, EditorError> {
    if !caller.is_owner() {
        return Err(denied());
    }
    let invalid = || EditorError::new("INVALID_ARGUMENT", "Invalid maintenance query");
    let after = input.after_sequence.parse::<i64>().map_err(|_| invalid())?;
    if after < 0 || after.to_string() != input.after_sequence || !(1..=500).contains(&input.limit) {
        return Err(invalid());
    }
    match &input.target {
        MaintenanceTarget::Object { object } => validate_object(object)?,
        MaintenanceTarget::Publication { id } | MaintenanceTarget::Request { id }
            if id.is_empty() || id.len() > 1024 =>
        {
            return Err(invalid());
        }
        _ => {}
    }
    let mut events = store.read_maintenance(&input.target, after, input.limit + 1)?;
    let has_more = events.len() > input.limit;
    events.truncate(input.limit);
    Ok(MaintenancePage {
        last_sequence: events
            .last()
            .map(|e| e.sequence.clone())
            .unwrap_or(input.after_sequence),
        events,
        has_more,
        coverage: O::coverage(),
    })
}
