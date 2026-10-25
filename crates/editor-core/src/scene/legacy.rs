//! Pure, all-or-nothing promotion of the frozen Rust project schemas.
//! This module neither probes resources nor reads legacy locators. A converted
//! aggregate has passed V4 validation; this is not a renderer acceptance receipt.
use opencut_contracts::{historical_visual::*, scene::*, *};
use std::collections::{BTreeMap, HashSet};

mod assets;
mod elements;
pub mod fragments;
#[cfg(test)]
mod tests;
mod visuals;

/// The outer schema, including V3 visual nodes, must be supplied explicitly.
pub enum LegacyProjectInput<'a> {
    Base(&'a ProjectDocument),
    V3(&'a HistoricalVisualDocument),
}

#[derive(Clone, Debug, Default)]
pub struct LegacyConversionInputs {
    /// Bind unversioned logical assets to already-observed immutable bytes.
    /// A binding for an already-versioned asset must exactly match its version.
    pub asset_versions: BTreeMap<String, AssetVersion>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LegacyConversionResult {
    Converted {
        project: Box<EditorDocument>,
        report: LegacyConversionReport,
    },
    /// Deliberately contains no partial project that could be committed.
    Rejected { report: LegacyConversionReport },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LegacyConversionReport {
    pub issues: Vec<LegacyConversionIssue>,
    pub generated_identities: Vec<GeneratedLegacyIdentity>,
    pub resource_bindings: Vec<LegacyResourceBinding>,
    pub normalizations: Vec<LegacyNormalization>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyConversionIssue {
    /// Stable field location in the supplied legacy document, not a host path.
    pub path: String,
    pub kind: LegacyConversionIssueKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LegacyConversionIssueKind {
    InvalidLegacyDocument { code: String, message: String },
    MissingAssetVersion { asset_id: String },
    UnknownVersionBinding { asset_id: String },
    ConflictingAssetVersion { asset_id: String },
    InvalidAssetVersion { asset_id: String, message: String },
    UnknownMetadata { field: String },
    InvalidMetadata { field: String, reason: String },
    UnsupportedClipContent { clip_id: String, reason: String },
    MissingSourceRange { clip_id: String },
    UnrepresentableRate { clip_id: String },
    UnsupportedParameter { parameter: String },
    InvalidNumericParameter { parameter: String },
    UnsupportedCaptionColor { value: String },
    InvalidV4Aggregate { code: String, message: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedLegacyIdentity {
    pub id: String,
    pub owner_id: String,
    pub purpose: GeneratedLegacyIdentityPurpose,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeneratedLegacyIdentityPurpose {
    MainTrack,
    CaptionOverlayTrack,
    AnimationChannel,
}
#[derive(Clone, Debug, PartialEq)]
pub struct LegacyResourceBinding {
    pub asset_id: String,
    pub version: AssetVersion,
    pub supplied: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LegacyNormalization {
    /// These values remain authoritative in AssetVersion, with equality checked.
    MetadataIdentityCanonicalized { asset_id: String, field: String },
    /// Standard media URLs are derived from CAS, not a second resource locator.
    MediaUrlDerived { asset_id: String },
    /// Old global cues bypassed legacy caption-track flags and rendered last.
    CaptionOverlayMadeExplicit {
        sequence_id: String,
        track_id: String,
    },
    /// V3 shapes retain their original track and order after its base clips.
    ShapeOrderMadeExplicit {
        sequence_id: String,
        shape_id: String,
        track_id: String,
    },
}

struct Context {
    report: LegacyConversionReport,
    used_ids: HashSet<String>,
    next_id: u64,
    identity_prefix: &'static str,
}
impl Context {
    fn issue(&mut self, path: impl Into<String>, kind: LegacyConversionIssueKind) {
        self.report.issues.push(LegacyConversionIssue {
            path: path.into(),
            kind,
        });
    }
    fn generated(&mut self, owner: &str, purpose: GeneratedLegacyIdentityPurpose) -> String {
        // Reserve *all* old IDs, including keys/curve nodes, before allocation.
        // Counter + membership testing is deterministic and cannot collide with
        // adversarial legacy IDs or exceed the aggregate's ID length bound.
        loop {
            let id = format!("{}_{}", self.identity_prefix, self.next_id);
            self.next_id += 1;
            if self.used_ids.insert(id.clone()) {
                self.report
                    .generated_identities
                    .push(GeneratedLegacyIdentity {
                        id: id.clone(),
                        owner_id: owner.into(),
                        purpose,
                    });
                return id;
            }
        }
    }
}

pub fn convert_legacy_project(
    input: LegacyProjectInput<'_>,
    inputs: &LegacyConversionInputs,
) -> LegacyConversionResult {
    let (base, v3) = match input {
        LegacyProjectInput::Base(base) => (base, None),
        LegacyProjectInput::V3(project) => (&project.base, Some(project)),
    };
    let mut context = Context {
        report: LegacyConversionReport::default(),
        used_ids: reserved_ids(base, v3),
        next_id: 0,
        identity_prefix: "legacy_v4",
    };
    let legacy_valid = match v3 {
        Some(value) => crate::historical_visual::validate_project_historical_visual(value),
        None => crate::validate_project(base),
    };
    if let Err(error) = legacy_valid {
        context.issue(
            "project",
            LegacyConversionIssueKind::InvalidLegacyDocument {
                code: error.code,
                message: error.message,
            },
        );
    }
    let assets = assets::convert_assets(base, inputs, &mut context);
    let mut sequences = Vec::with_capacity(base.sequences.len());
    for (sequence_index, sequence) in base.sequences.iter().enumerate() {
        let path = format!("sequences[{sequence_index}]");
        let mut tracks = sequence
            .tracks
            .iter()
            .enumerate()
            .map(|(index, track)| {
                elements::convert_track(
                    base,
                    sequence,
                    track,
                    v3,
                    &format!("{path}.tracks[{index}]"),
                    &mut context,
                )
            })
            .collect::<Vec<_>>();
        if let Some(graph) = v3.and_then(|p| p.visual.iter().find(|v| v.sequence_id == sequence.id))
        {
            visuals::apply_graph(sequence, graph, &mut tracks, &mut context);
        }
        let main_track_id = match tracks.iter().find(|t| t.kind == SceneTrackKind::Video) {
            Some(track) => track.id.clone(),
            None => {
                let id = context.generated(&sequence.id, GeneratedLegacyIdentityPurpose::MainTrack);
                // Empty bottom track does not move any preexisting relative layer.
                tracks.insert(0, empty_track(id.clone(), "Main", SceneTrackKind::Video));
                id
            }
        };
        let captions = if sequence.caption_cues.is_empty() {
            Vec::new()
        } else {
            let id = context.generated(
                &sequence.id,
                GeneratedLegacyIdentityPurpose::CaptionOverlayTrack,
            );
            tracks.push(empty_track(id.clone(), "Captions", SceneTrackKind::Caption));
            context
                .report
                .normalizations
                .push(LegacyNormalization::CaptionOverlayMadeExplicit {
                    sequence_id: sequence.id.clone(),
                    track_id: id.clone(),
                });
            sequence
                .caption_cues
                .iter()
                .enumerate()
                .filter_map(|(index, cue)| {
                    visuals::caption(
                        cue,
                        &id,
                        &format!("{path}.caption_cues[{index}]"),
                        &mut context,
                    )
                })
                .collect()
        };
        let transitions = v3
            .and_then(|p| p.visual.iter().find(|v| v.sequence_id == sequence.id))
            .map(|graph| {
                graph
                    .transitions
                    .iter()
                    .map(|value| SceneTransition {
                        id: value.id.clone(),
                        outgoing_element_id: value.outgoing_clip_id.clone(),
                        incoming_element_id: value.incoming_clip_id.clone(),
                        duration: value.duration.clone(),
                        transition: TransitionKind::LegacyCrossfadeV3,
                    })
                    .collect()
            })
            .unwrap_or_default();
        sequences.push(Scene {
            id: sequence.id.clone(),
            name: sequence.name.clone(),
            main_track_id,
            tracks,
            captions,
            transitions,
            bookmarks: Vec::new(),
        });
    }
    let project = EditorDocument {
        schema_version: 4,
        id: base.id.clone(),
        name: base.name.clone(),
        default_sequence_id: base.default_sequence_id.clone(),
        settings: SettingsProfile::LegacyPerScene {
            formats: base
                .sequences
                .iter()
                .map(|sequence| LegacySequenceFormat {
                    sequence_id: sequence.id.clone(),
                    fps: sequence.fps.clone(),
                    canvas_size: CanvasSize {
                        width: sequence.width,
                        height: sequence.height,
                    },
                    background: Background::Color {
                        color: visuals::rgba(0, 0, 0, 255),
                    },
                })
                .collect(),
        },
        assets,
        fonts: Vec::new(),
        sequences,
    };
    // Never let a successfully shaped conversion bypass the aggregate validator.
    // Unsupported fields were already exhaustively collected above; checking a
    // partial candidate adds no useful error, so validate only a complete one.
    if context.report.issues.is_empty()
        && let Err(error) = super::validate_project_scene(&project)
    {
        context.issue(
            "project",
            LegacyConversionIssueKind::InvalidV4Aggregate {
                code: error.code,
                message: error.message,
            },
        );
    }
    if context.report.issues.is_empty() {
        LegacyConversionResult::Converted {
            project: Box::new(project),
            report: context.report,
        }
    } else {
        LegacyConversionResult::Rejected {
            report: context.report,
        }
    }
}

fn empty_track(id: String, name: &str, kind: SceneTrackKind) -> SceneTrack {
    SceneTrack {
        id,
        name: name.into(),
        kind,
        enabled: true,
        muted: false,
        locked: false,
        opacity: 1.0,
        gain: Gain::LinearLegacy { value: 1.0 },
        elements: Vec::new(),
        channels: Vec::new(),
    }
}

fn reserved_ids(base: &ProjectDocument, v3: Option<&HistoricalVisualDocument>) -> HashSet<String> {
    let mut ids = HashSet::from([base.id.clone()]);
    ids.extend(base.assets.iter().map(|a| a.id.clone()));
    for sequence in &base.sequences {
        ids.insert(sequence.id.clone());
        ids.extend(sequence.caption_cues.iter().map(|c| c.id.clone()));
        for track in &sequence.tracks {
            ids.insert(track.id.clone());
            for clip in &track.clips {
                ids.insert(clip.id.clone());
                ids.extend(
                    clip.channels
                        .iter()
                        .flat_map(|c| c.keyframes.iter())
                        .map(|k| k.id.clone()),
                );
            }
        }
    }
    if let Some(project) = v3 {
        ids.extend(project.keyframe_curves.iter().map(|c| c.id.clone()));
        for graph in &project.visual {
            ids.extend(graph.shapes.iter().map(|s| s.id.clone()));
            ids.extend(graph.effects.iter().map(|s| s.id.clone()));
            ids.extend(graph.masks.iter().map(|s| s.id.clone()));
            ids.extend(graph.transitions.iter().map(|s| s.id.clone()));
        }
    }
    ids
}
