//! Data-package adaptation reuses the original track,clip and caption conversion.
//! This adapter does not convert or replace the caller's complete editor document.
use super::*;

pub struct FragmentConverter {
    base: ProjectDocument,
    context: Context,
    existing_ids: HashSet<String>,
}
fn identity_set(value: &serde_json::Value) -> HashSet<String> {
    fn collect(value: &serde_json::Value, ids: &mut HashSet<String>) {
        match value {
            serde_json::Value::Object(fields) => {
                for (key, value) in fields {
                    if key == "id"
                        && let Some(id) = value.as_str()
                    {
                        ids.insert(id.into());
                    }
                    collect(value, ids);
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    collect(value, ids);
                }
            }
            _ => (),
        }
    }
    let mut ids = HashSet::new();
    collect(value, &mut ids);
    ids
}
impl FragmentConverter {
    pub fn new(project: &EditorDocument, source: &[Operation]) -> Result<Self, EditorError> {
        let existing_ids = super::super::validated_identities(project)?;
        let source_data = serde_json::to_value(source)
            .map_err(|_| EditorError::new("INVALID_PRESET", "Cannot inspect preset identities"))?;
        let mut ids = existing_ids.clone();
        ids.extend(identity_set(&source_data));
        let sequences = project
            .sequences
            .iter()
            .map(|s| {
                let (canvas, fps) =
                    match &project.settings {
                        SettingsProfile::ClassicGlobal { settings } => {
                            (settings.canvas_size, settings.fps.clone())
                        }
                        SettingsProfile::LegacyPerScene { formats } => {
                            let f = formats.iter().find(|f| f.sequence_id == s.id).ok_or_else(
                                || EditorError::new("INVALID_PROJECT", "Scene format is missing"),
                            )?;
                            (f.canvas_size, f.fps.clone())
                        }
                    };
                Ok(Sequence {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    width: canvas.width,
                    height: canvas.height,
                    fps,
                    tracks: vec![],
                    caption_cues: vec![],
                })
            })
            .collect::<Result<Vec<_>, EditorError>>()?;
        // Only facts consumed by the original fragment converters are supplied.
        // No native locator,projection of existing elements or new resource authority.
        let assets = project
            .assets
            .iter()
            .map(|a| Asset {
                id: a.id.clone(),
                name: a.name.clone(),
                kind: a.kind.clone(),
                uri: String::new(),
                duration: a.duration.clone(),
                version: Some(a.version.clone()),
                metadata: BTreeMap::new(),
            })
            .collect();
        Ok(Self {
            existing_ids,
            base: ProjectDocument {
                id: project.id.clone(),
                name: project.name.clone(),
                schema_version: 2,
                default_sequence_id: project.default_sequence_id.clone(),
                sequences,
                assets,
            },
            context: Context {
                report: LegacyConversionReport::default(),
                used_ids: ids,
                next_id: 0,
                identity_prefix: "preset",
            },
        })
    }
    pub fn existing_identities(&self) -> &HashSet<String> {
        &self.existing_ids
    }
    fn finish<T>(&self, value: Option<T>) -> Result<T, EditorError> {
        if !self.context.report.issues.is_empty() {
            return Err(EditorError::new("UNSUPPORTED_PRESET_CONTENT","Preset data cannot be represented without dropping declared content")
                .with_details(serde_json::json!({"fields":self.context.report.issues.iter().map(|issue|&issue.path).collect::<Vec<_>>()})));
        }
        value.ok_or_else(|| {
            EditorError::new("INVALID_PRESET", "Preset fragment could not be converted")
        })
    }
    pub fn track(&mut self, scene: &str, track: &Track) -> Result<SceneTrack, EditorError> {
        if !track.clips.is_empty() {
            return Err(EditorError::new(
                "INVALID_PRESET",
                "Preset track must be empty",
            ));
        }
        let scope = self
            .base
            .sequences
            .iter()
            .find(|s| s.id == scene)
            .ok_or_else(|| EditorError::new("NOT_FOUND", "Preset scene does not exist"))?;
        let value = elements::convert_track(
            &self.base,
            scope,
            track,
            None,
            "preset.track",
            &mut self.context,
        );
        self.finish(Some(value))
    }
    pub fn element(
        &mut self,
        scene: &str,
        track_id: &str,
        kind: TrackKind,
        clip: &Clip,
    ) -> Result<TimelineElement, EditorError> {
        let scope = self
            .base
            .sequences
            .iter()
            .find(|s| s.id == scene)
            .ok_or_else(|| EditorError::new("NOT_FOUND", "Preset scene does not exist"))?;
        let track = Track {
            id: track_id.into(),
            name: String::new(),
            kind,
            enabled: true,
            muted: false,
            locked: false,
            clips: vec![],
            params: BTreeMap::new(),
        };
        let value = elements::clip_element(
            &self.base,
            scope,
            &track,
            clip,
            None,
            "preset.clip",
            &mut self.context,
        );
        self.finish(value)
    }
    pub fn caption(&mut self, cue: &CaptionCue, track_id: &str) -> Result<Caption, EditorError> {
        let value = visuals::caption(cue, track_id, "preset.caption", &mut self.context);
        self.finish(value)
    }
}
