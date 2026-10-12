//! Pure bookmark rules. No storage, host resource, UI or rendering dependency.
//! Classic reference: timeline/bookmarks/utils.ts and commands/scene/*-bookmark.ts.
use opencut_contracts::{
    EditorError, FrameRate, ProjectDocument, Time,
    project_v4::{ProjectDocumentV4, SettingsProfileV4},
    scene_bookmarks::{BookmarkOperationV1, SceneBookmarkV1},
};
use opencut_time::{FrameRounding, RationalTime, align_to_frame};
use std::{cmp::Ordering, collections::BTreeSet};

pub const MAX_BOOKMARKS: usize = 10_000;
pub const MAX_NOTE_BYTES: usize = 4096;

fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}

fn valid_id(id: &str) -> bool {
    !id.trim().is_empty() && id.len() <= 128
}

fn nonnegative(value: &Time) -> Result<RationalTime, EditorError> {
    let value = RationalTime::try_from(value)?;
    if value.ticks() < 0 {
        return Err(invalid("Bookmark time and duration must be nonnegative"));
    }
    Ok(value)
}

trait BookmarkScope {
    fn frame_rate(&self, sequence_id: &str) -> Option<&FrameRate>;
}
impl BookmarkScope for ProjectDocument {
    fn frame_rate(&self, sequence_id: &str) -> Option<&FrameRate> {
        self.sequences
            .iter()
            .find(|s| s.id == sequence_id)
            .map(|s| &s.fps)
    }
}
impl BookmarkScope for ProjectDocumentV4 {
    fn frame_rate(&self, sequence_id: &str) -> Option<&FrameRate> {
        if !self.sequences.iter().any(|s| s.id == sequence_id) {
            return None;
        }
        match &self.settings {
            SettingsProfileV4::ClassicGlobal { settings } => Some(&settings.fps),
            SettingsProfileV4::LegacyPerScene { formats } => formats
                .iter()
                .find(|f| f.sequence_id == sequence_id)
                .map(|f| &f.fps),
        }
    }
}

pub fn validate(
    project: &ProjectDocument,
    bookmarks: &[SceneBookmarkV1],
) -> Result<(), EditorError> {
    validate_for_scope(project, bookmarks)
}

fn validate_for_scope(
    project: &impl BookmarkScope,
    bookmarks: &[SceneBookmarkV1],
) -> Result<(), EditorError> {
    if bookmarks.len() > MAX_BOOKMARKS {
        return Err(invalid("Project exceeds the 10000 bookmark limit"));
    }
    let mut ids = BTreeSet::new();
    for bookmark in bookmarks {
        if !valid_id(&bookmark.id) || !ids.insert(&bookmark.id) {
            return Err(invalid("Bookmark IDs must be distinct and bounded"));
        }
        if project.frame_rate(&bookmark.sequence_id).is_none() {
            return Err(invalid("Bookmark scene does not exist"));
        }
        let start = nonnegative(&bookmark.time)?;
        if let Some(duration) = &bookmark.duration {
            start.checked_add(nonnegative(duration)?)?;
        }
        if bookmark
            .note
            .as_ref()
            .is_some_and(|n| n.len() > MAX_NOTE_BYTES)
        {
            return Err(invalid("Bookmark note exceeds 4096 UTF-8 bytes"));
        }
        if bookmark.color.as_ref().is_some_and(|c| {
            c.len() != 7
                || !c.starts_with('#')
                || !c.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
        }) {
            return Err(invalid("Bookmark color must be #RRGGBB"));
        }
    }
    Ok(())
}

fn frame_time(
    project: &impl BookmarkScope,
    sequence_id: &str,
    time: &Time,
) -> Result<Time, EditorError> {
    nonnegative(time)?;
    let fps = project
        .frame_rate(sequence_id)
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Bookmark scene does not exist"))?;
    align_to_frame(time, fps, FrameRounding::Nearest)
}

fn index(bookmarks: &[SceneBookmarkV1], sequence_id: &str, id: &str) -> Result<usize, EditorError> {
    bookmarks
        .iter()
        .position(|b| b.id == id && b.sequence_id == sequence_id)
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Bookmark does not exist in this scene"))
}

/// Return a complete proposed collection; a later invalid operation publishes nothing.
/// A move may retain equal-time bookmarks, matching Classic's non-merging behavior.
pub fn apply(
    project: &ProjectDocument,
    bookmarks: &[SceneBookmarkV1],
    operations: &[BookmarkOperationV1],
) -> Result<Vec<SceneBookmarkV1>, EditorError> {
    crate::validate_project(project)?;
    apply_to_scope(project, bookmarks, operations)
}

/// Mutate revision-owned annotations using the aggregate's real format scope.
/// No legacy document projection or independent bookmark store is created.
pub fn apply_bookmarks_v4(
    project: &ProjectDocumentV4,
    operations: &[BookmarkOperationV1],
) -> Result<ProjectDocumentV4, EditorError> {
    crate::project_v4::validate_project_v4(project)?;
    let proposed = plan_bookmarks_v4(project, operations)?;
    let mut result = project.clone();
    for scene in &mut result.sequences {
        scene.bookmarks = proposed
            .iter()
            .filter(|b| b.sequence_id == scene.id)
            .cloned()
            .collect();
    }
    crate::project_v4::validate_project_v4(&result)?;
    Ok(result)
}

/// Plan only annotations within a larger aggregate transaction. The application
/// validates the starting snapshot and complete final aggregate; temporary
/// unrelated resource changes must not invalidate this collection-only step.
pub fn plan_bookmarks_v4(
    project: &ProjectDocumentV4,
    operations: &[BookmarkOperationV1],
) -> Result<Vec<SceneBookmarkV1>, EditorError> {
    let bookmarks: Vec<_> = project
        .sequences
        .iter()
        .flat_map(|s| s.bookmarks.iter().cloned())
        .collect();
    apply_to_scope(project, &bookmarks, operations)
}

fn apply_to_scope(
    project: &impl BookmarkScope,
    bookmarks: &[SceneBookmarkV1],
    operations: &[BookmarkOperationV1],
) -> Result<Vec<SceneBookmarkV1>, EditorError> {
    validate_for_scope(project, bookmarks)?;
    if operations.is_empty() || operations.len() > 128 {
        return Err(invalid("Choose 1 to 128 bookmark operations"));
    }
    let mut proposed = bookmarks.to_vec();
    for operation in operations {
        match operation {
            BookmarkOperationV1::Toggle {
                sequence_id,
                time,
                bookmark_id,
            } => {
                if !valid_id(bookmark_id) {
                    return Err(invalid("Bookmark ID is invalid"));
                }
                let time = frame_time(project, sequence_id, time)?;
                if let Some(at) = proposed.iter().position(|b| {
                    b.sequence_id == *sequence_id
                        && RationalTime::try_from(&b.time)
                            .expect("validated bookmark time")
                            .cmp_exact(RationalTime::try_from(&time).expect("aligned time"))
                            == Ordering::Equal
                }) {
                    proposed.remove(at);
                } else {
                    if proposed.iter().any(|b| b.id == *bookmark_id) {
                        return Err(invalid("Bookmark ID already exists"));
                    }
                    proposed.push(SceneBookmarkV1 {
                        id: bookmark_id.clone(),
                        sequence_id: sequence_id.clone(),
                        time,
                        duration: None,
                        color: None,
                        note: None,
                    });
                }
            }
            BookmarkOperationV1::Put { bookmark } => {
                let mut bookmark = bookmark.clone();
                bookmark.time = frame_time(project, &bookmark.sequence_id, &bookmark.time)?;
                if let Some(at) = proposed.iter().position(|b| b.id == bookmark.id) {
                    if proposed[at].sequence_id != bookmark.sequence_id {
                        return Err(invalid("Bookmark update cannot change its scene"));
                    }
                    proposed[at] = bookmark;
                } else {
                    proposed.push(bookmark);
                }
            }
            BookmarkOperationV1::Move {
                sequence_id,
                bookmark_id,
                time,
            } => {
                let at = index(&proposed, sequence_id, bookmark_id)?;
                proposed[at].time = frame_time(project, sequence_id, time)?;
            }
            BookmarkOperationV1::Delete {
                sequence_id,
                bookmark_id,
            } => {
                let at = index(&proposed, sequence_id, bookmark_id)?;
                proposed.remove(at);
            }
        }
        validate_for_scope(project, &proposed)?;
    }
    proposed.sort_by(|a, b| {
        a.sequence_id.cmp(&b.sequence_id).then_with(|| {
            RationalTime::try_from(&a.time)
                .expect("validated time")
                .cmp_exact(RationalTime::try_from(&b.time).expect("validated time"))
        })
    });
    Ok(proposed)
}

/// Classic notes include both interval boundaries, even for a zero-duration marker.
/// Blank notes are retained in the project but excluded from the auxiliary HUD.
pub fn active_notes<'a>(
    bookmarks: &'a [SceneBookmarkV1],
    sequence_id: &str,
    time: &Time,
) -> Result<Vec<&'a SceneBookmarkV1>, EditorError> {
    let now = nonnegative(time)?;
    let mut result = Vec::new();
    for bookmark in bookmarks {
        if bookmark.sequence_id != sequence_id
            || bookmark.note.as_ref().is_none_or(|n| n.trim().is_empty())
        {
            continue;
        }
        let start = nonnegative(&bookmark.time)?;
        let end = match &bookmark.duration {
            Some(d) => start.checked_add(nonnegative(d)?)?,
            None => start,
        };
        if now.cmp_exact(start) != Ordering::Less && now.cmp_exact(end) != Ordering::Greater {
            result.push(bookmark);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_contracts::{CreateProject, FrameRate};

    fn time(ticks: i64, timebase: u32) -> Time {
        Time {
            ticks: ticks.to_string(),
            timebase,
        }
    }
    fn project() -> ProjectDocument {
        let mut p = crate::create_project(&CreateProject {
            id: "project".into(),
            name: "P".into(),
            sequence_id: "main".into(),
            width: 320,
            height: 180,
            fps: FrameRate {
                numerator: 24,
                denominator: 1,
            },
        })
        .unwrap();
        let mut other = p.sequences[0].clone();
        other.id = "other".into();
        other.fps = FrameRate {
            numerator: 30000,
            denominator: 1001,
        };
        p.sequences.push(other);
        p
    }
    fn bookmark(id: &str, scene: &str) -> SceneBookmarkV1 {
        SceneBookmarkV1 {
            id: id.into(),
            sequence_id: scene.into(),
            time: time(1, 24),
            duration: None,
            color: Some("#ab12EF".into()),
            note: Some("注释".into()),
        }
    }
    #[test]
    fn toggle_uses_exact_scene_frame_and_preserves_other_scenes() {
        let p = project();
        let original = vec![bookmark("a", "main"), bookmark("b", "other")];
        let after = apply(
            &p,
            &original,
            &[BookmarkOperationV1::Toggle {
                sequence_id: "main".into(),
                time: time(2, 48),
                bookmark_id: "new".into(),
            }],
        )
        .unwrap();
        assert_eq!(after, vec![original[1].clone()]);
        assert_eq!(original.len(), 2);
        let after = apply(
            &p,
            &[],
            &[BookmarkOperationV1::Toggle {
                sequence_id: "main".into(),
                time: time(1, 48),
                bookmark_id: "new".into(),
            }],
        )
        .unwrap();
        assert_eq!(after[0].time, time(1, 24));
        assert_eq!(after[0].color, None);
    }
    #[test]
    fn fractional_frame_move_preserves_note_color_duration_and_identity() {
        let p = project();
        let mut b = bookmark("a", "other");
        b.duration = Some(time(3, 2));
        let moved = apply(
            &p,
            &[b.clone()],
            &[BookmarkOperationV1::Move {
                sequence_id: "other".into(),
                bookmark_id: "a".into(),
                time: time(1, 20),
            }],
        )
        .unwrap();
        let mut expected = b;
        expected.time = time(1001, 30000);
        assert_eq!(moved, vec![expected]);
    }
    #[test]
    fn updating_and_deleting_require_the_original_scene() {
        let p = project();
        let b = bookmark("a", "main");
        let mut update = b.clone();
        update.sequence_id = "other".into();
        assert!(
            apply(
                &p,
                std::slice::from_ref(&b),
                &[BookmarkOperationV1::Put { bookmark: update }]
            )
            .is_err()
        );
        assert_eq!(
            apply(
                &p,
                &[b],
                &[BookmarkOperationV1::Delete {
                    sequence_id: "other".into(),
                    bookmark_id: "a".into()
                }]
            )
            .unwrap_err()
            .code,
            "NOT_FOUND"
        );
    }
    #[test]
    fn duplicate_times_are_retained_and_toggle_removes_only_the_first() {
        let p = project();
        let before = vec![bookmark("a", "main"), bookmark("b", "main")];
        let after = apply(
            &p,
            &before,
            &[BookmarkOperationV1::Toggle {
                sequence_id: "main".into(),
                time: time(1, 24),
                bookmark_id: "unused".into(),
            }],
        )
        .unwrap();
        assert_eq!(after, vec![before[1].clone()]);
    }
    #[test]
    fn failed_later_operation_leaves_the_original_collection_intact() {
        let p = project();
        let before = vec![bookmark("a", "main")];
        let saved = before.clone();
        let mut b = before[0].clone();
        b.note = Some("changed".into());
        assert!(
            apply(
                &p,
                &before,
                &[
                    BookmarkOperationV1::Put { bookmark: b },
                    BookmarkOperationV1::Delete {
                        sequence_id: "main".into(),
                        bookmark_id: "missing".into()
                    }
                ]
            )
            .is_err()
        );
        assert_eq!(before, saved);
    }
    #[test]
    fn active_notes_use_inclusive_exact_endpoints_and_skip_blanks() {
        let mut b = bookmark("a", "main");
        b.time = time(1, 3);
        b.duration = Some(time(1, 6));
        let mut blank = b.clone();
        blank.id = "blank".into();
        blank.note = Some(" \n\t ".into());
        let notes = vec![b.clone(), blank, bookmark("other", "other")];
        assert_eq!(
            active_notes(&notes, "main", &time(2, 6)).unwrap(),
            vec![&notes[0]]
        );
        assert_eq!(
            active_notes(&notes, "main", &time(1, 2)).unwrap(),
            vec![&notes[0]]
        );
        assert!(
            active_notes(&notes, "main", &time(501, 1000))
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            active_notes(&[b.clone()], "main", &time(1, 3))
                .unwrap()
                .len(),
            1
        );
        b.duration = None;
        assert!(
            active_notes(&[b], "main", &time(334, 1000))
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn rejects_invalid_utf8_color_negative_time_overflow_and_duplicate_ids() {
        let p = project();
        let good = bookmark("a", "main");
        for color in ["red", "#１２", "#ab12ex"] {
            let mut b = good.clone();
            b.color = Some(color.into());
            assert!(validate(&p, &[b]).is_err());
        }
        let mut b = good.clone();
        b.time = time(-1, 24);
        assert!(validate(&p, std::slice::from_ref(&b)).is_err());
        b.time = time(i64::MAX, 1);
        b.duration = Some(time(1, 1));
        assert!(validate(&p, &[b]).is_err());
        b = good.clone();
        b.note = Some("汉".repeat(1366));
        assert!(validate(&p, &[b]).is_err());
        assert!(validate(&p, &[good.clone(), good]).is_err());
    }
    #[test]
    fn pending_model_rejects_unknown_wire_fields() {
        let b = bookmark("a", "main");
        let mut value = serde_json::to_value(b).unwrap();
        value["native_path"] = serde_json::json!("/private");
        assert!(serde_json::from_value::<SceneBookmarkV1>(value).is_err());
        assert!(
            serde_json::from_value::<BookmarkOperationV1>(serde_json::json!({
                "kind":"delete","sequence_id":"main","bookmark_id":"a","force":true
            }))
            .is_err()
        );
    }
}
