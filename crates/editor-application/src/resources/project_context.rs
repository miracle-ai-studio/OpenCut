//! Current document projection through existing bounded query use cases.
use crate::scene::{self, SceneRead};
use opencut_api_contracts::context::ProjectContext;
use opencut_contracts::{
    EditorError, Time, TimeRange, agent_read::*, project_inspection::InspectProject,
};
use opencut_time::RationalTime as R;
pub fn read(
    store: &impl SceneRead,
    project: &str,
    revision: &str,
    scene: Option<&str>,
) -> Result<ProjectContext, EditorError> {
    let invalid = || {
        EditorError::new(
            "NOT_FOUND",
            "Sequence was not found at the fixed project revision",
        )
    };
    let summary = crate::agent_read::summary(
        store,
        &ProjectSummaryRequest {
            project_id: project.to_owned(),
            revision: Some(revision.to_owned()),
            sequence_offset: 0,
            limit: 100,
        },
    )?;
    let timeline = if let Some(scene) = scene {
        let snapshot = scene::inspect(
            store,
            &InspectProject {
                project_id: project.to_owned(),
                revision: Some(revision.to_owned()),
            },
        )?;
        let sequence = snapshot
            .project
            .sequences
            .iter()
            .find(|s| s.id == scene)
            .ok_or_else(invalid)?;
        let mut end = R::new(1, 1)?;
        for range in sequence
            .tracks
            .iter()
            .flat_map(|t| t.elements.iter().map(|e| &e.timeline_range))
            .chain(sequence.captions.iter().map(|c| &c.timeline_range))
        {
            let value = R::try_from(&opencut_time::range_end(range)?)?;
            if value.cmp_exact(end).is_gt() {
                end = value;
            }
        }
        Some(crate::agent_read::timeline(
            store,
            &TimelineQueryRequest {
                project_id: project.to_owned(),
                revision: revision.to_owned(),
                sequence_id: scene.to_owned(),
                range: TimeRange {
                    start: Time {
                        ticks: "0".into(),
                        timebase: 1,
                    },
                    duration: end.into(),
                },
                track_ids: vec![],
                include_annotations: true,
                offset: 0,
                limit: 100,
            },
        )?)
    } else {
        None
    };
    crate::agent_read::bounded(ProjectContext { summary, timeline })
}
