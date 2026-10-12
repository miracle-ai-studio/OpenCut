//! Collection and tagged-value semantics belong to their document version.
use serde_json::{Map, Value};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum MergeProfile {
    Legacy,
    V4,
}

pub(super) fn collection_key(profile: MergeProfile, path: &str) -> Option<&'static str> {
    let field = path.rsplit('/').next()?;
    if profile == MergeProfile::V4 {
        return match field {
            "assets" | "fonts" | "sequences" | "tracks" | "elements" | "captions" | "bookmarks"
            | "transitions" | "effects" | "masks" | "channels" | "keys" | "points" => Some("id"),
            "formats" => Some("sequence_id"),
            _ => None,
        };
    }
    match field {
        "assets" | "sequences" | "tracks" | "clips" | "caption_cues" | "keyframes" | "shapes"
        | "effects" | "masks" | "transitions" | "keyframe_curves" => Some("id"),
        "visual" => Some("sequence_id"),
        "channels" => Some("parameter"),
        _ => None,
    }
}

/// A changed variant is an atomic value. Property inversion must never combine
/// fields from different content kinds, color spaces, units or clock profiles.
pub(super) fn same_variant(
    profile: MergeProfile,
    from: &Map<String, Value>,
    to: &Map<String, Value>,
    head: &Map<String, Value>,
) -> bool {
    if profile == MergeProfile::Legacy {
        return true;
    }
    let tags_match = ["kind", "profile", "unit", "status", "encoding"]
        .iter()
        .all(|tag| from.get(*tag) == to.get(*tag) && from.get(*tag) == head.get(*tag));
    // A channel's target and binding define the meaning and units of every
    // key and tangent. Never invert key edits into a subsequently rebound
    // channel, even if both bindings use the same serde variant.
    let channel_identity_matches = !from.contains_key("keys")
        || ["target", "binding", "parameter"].iter().all(|field| {
            from.get(*field) == to.get(*field) && from.get(*field) == head.get(*field)
        });
    // Exact time/rate components form one semantic value. Property inversion
    // cannot combine an earlier numerator/tick with a later denominator/base.
    let rational_identity_matches = [["ticks", "timebase"], ["numerator", "denominator"]]
        .iter()
        .all(|fields| {
            !fields.iter().all(|field| from.contains_key(*field))
                || fields.iter().all(|field| {
                    from.get(*field) == to.get(*field) && from.get(*field) == head.get(*field)
                })
        });
    tags_match && channel_identity_matches && rational_identity_matches
}
