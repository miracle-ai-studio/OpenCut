use super::*;
use opencut_contracts::{AssetVersion, CreateProject, FrameRate, project_v4::*};

struct EventFixture(Vec<StoredEvent>);
impl ProjectEventRead for EventFixture {
    fn read_project_changes(
        &self,
        _: Option<&EventAnchor>,
        _: Option<&str>,
        _: usize,
    ) -> Result<Vec<StoredEvent>, EditorError> {
        Ok(self.0.clone())
    }
}
impl crate::queries::WorkspaceRead for EventFixture {
    fn read_head_revision(&self, _: &str) -> Result<String, EditorError> {
        unreachable!()
    }
    fn read_projects(&self) -> Result<Vec<opencut_contracts::ProjectSnapshot>, EditorError> {
        unreachable!()
    }
    fn read_revision_summary(
        &self,
        _: &str,
        _: &str,
    ) -> Result<crate::RevisionSummary, EditorError> {
        unreachable!()
    }
    fn read_project_events(&self, _: i64, _: usize) -> Result<Vec<StoredEvent>, EditorError> {
        Ok(self.0.clone())
    }
}
impl crate::projects::ProjectRead for EventFixture {
    fn authorize_edit(
        &self,
        _: &str,
        _: crate::projects::EditIntent<'_>,
    ) -> Result<(), EditorError> {
        unreachable!()
    }
    fn head(&self, _: &str) -> Result<opencut_contracts::ProjectSnapshot, EditorError> {
        unreachable!()
    }
    fn revision(
        &self,
        _: &str,
        _: &str,
    ) -> Result<opencut_contracts::ProjectSnapshot, EditorError> {
        unreachable!()
    }
    fn replay_create(
        &self,
        _: &CreateProject,
        _: &str,
    ) -> Result<Option<opencut_contracts::ProjectSnapshot>, EditorError> {
        unreachable!()
    }
    fn replay_edit(
        &self,
        _: &str,
        _: crate::projects::EditIntent<'_>,
    ) -> Result<Option<opencut_contracts::TransactionResult>, EditorError> {
        unreachable!()
    }
}
impl JobRead for EventFixture {
    fn read_job(&self, _: &str) -> Result<crate::JobRecord, EditorError> {
        unreachable!()
    }
    fn read_job_for_cursor(&self, _: &str) -> Result<(crate::JobRecord, String), EditorError> {
        unreachable!()
    }
    fn read_jobs(&self) -> Result<Vec<crate::JobRecord>, EditorError> {
        unreachable!()
    }
    fn read_job_page(
        &self,
        _: Option<&str>,
        _: Option<&str>,
        _: usize,
    ) -> Result<crate::queries::JobPageSlice, EditorError> {
        unreachable!()
    }
    fn read_job_changes(
        &self,
        _: Option<&EventAnchor>,
        _: Option<&str>,
        _: usize,
    ) -> Result<Vec<crate::JobEvent>, EditorError> {
        unreachable!()
    }
    fn read_job_events(
        &self,
        _: i64,
        _: usize,
        _: Option<&str>,
    ) -> Result<Vec<crate::JobEvent>, EditorError> {
        unreachable!()
    }
}

#[test]
fn v4_creation_and_copy_events_hide_complete_internal_snapshots() {
    let mut document = crate::project_v4::new_document(&CreateProject {
        id: "private-payload-id".into(),
        name: "/private/source/project".into(),
        sequence_id: "scene".into(),
        width: 320,
        height: 180,
        fps: FrameRate {
            numerator: 24,
            denominator: 1,
        },
    })
    .unwrap();
    document.fonts.push(FontResourceV4 {
        id: "font".into(),
        family: "Example".into(),
        weight: 400,
        style: FontStyleV4::Normal,
        face_index: 0,
        content: AssetVersion {
            id: "av".into(),
            content_hash: "0".repeat(64),
            size: 1,
        },
        origin: ResourceOriginV4::LegacyLocator {
            uri: "/Users/private/fonts/source.ttf".into(),
        },
        license: None,
    });
    let private = serde_json::to_value(ProjectSnapshotV4 {
        project: document,
        revision: "private-revision".into(),
    })
    .unwrap();
    let scope = EventScope::Projects {
        project_id: Some("p".into()),
    };
    for kind in ["project.v4_created", "project.v4_copied"] {
        let event = StoredEvent {
            sequence: 1,
            event_id: "evt".into(),
            project_id: "p".into(),
            revision: "rev".into(),
            event_type: kind.into(),
            payload: private.clone(),
        };
        let fixture = EventFixture(vec![event]);
        let page = changes(
            &fixture,
            EventChangesInput {
                scope: scope.clone(),
                cursor: None,
                limit: 10,
            },
        )
        .unwrap();
        let change = page.events.into_iter().next().unwrap();
        let EventData::Project(public) = change.event else {
            panic!("project event expected")
        };
        assert_eq!(
            public.payload,
            serde_json::json!({"project_id":"p","revision":"rev","schema_version":4})
        );
        assert_eq!(public.event_type, kind);
        let direct = crate::queries::project_events(&fixture, 0, 10).unwrap();
        assert_eq!(direct[0].payload, public.payload);
        assert_eq!(direct[0].project_id, "p");
        assert_eq!(direct[0].revision, "rev");
        assert_eq!(direct[0].sequence, 1);
        let serialized = serde_json::to_string(&public).unwrap();
        assert!(!serialized.contains("LegacyLocator"));
        assert!(!serialized.contains("legacy_locator"));
        assert!(!serialized.contains("/private"));
        assert!(!serialized.contains("private-revision"));
    }
    let event = StoredEvent {
        sequence: 2,
        event_id: "legacy".into(),
        project_id: "p".into(),
        revision: "rev".into(),
        event_type: "project.created".into(),
        payload: private.clone(),
    };
    let fixture = EventFixture(vec![event.clone()]);
    let page = changes(
        &fixture,
        EventChangesInput {
            scope,
            cursor: None,
            limit: 10,
        },
    )
    .unwrap();
    let EventData::Project(legacy) = page.events.into_iter().next().unwrap().event else {
        panic!("project event expected")
    };
    assert_eq!(legacy.payload, private);
    assert_eq!(
        crate::queries::project_events(&fixture, 0, 10).unwrap(),
        vec![event]
    );
}
