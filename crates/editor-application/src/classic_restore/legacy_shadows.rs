//! Original reviewed Classic shadow-field input reader.
use super::*;
use serde_json::json;
const MAX_CLASSIC_BYTES: usize = 1024 * 1024;
pub(super) fn normalize(input: &AnalyzeClassic) -> (AnalyzeClassic, Vec<CompatibilityFinding>) {
    if input.content.len() > MAX_CLASSIC_BYTES {
        return (input.clone(), Vec::new());
    }
    let Ok(mut raw) = serde_json::from_str::<Value>(&input.content) else {
        return (input.clone(), Vec::new());
    };
    let mut losses = Vec::new();
    if raw["version"] != json!(31) {
        return (input.clone(), losses);
    }
    if let Some(root) = raw.as_object_mut() {
        let metadata = root.get("metadata").and_then(Value::as_object).cloned();
        if let Some(metadata) = metadata {
            for name in ["id", "name", "thumbnail", "createdAt", "updatedAt"] {
                // A contradictory keyPath identity is an error, not a shadow.
                if name == "id" && root.get(name) != metadata.get(name) {
                    continue;
                }
                if metadata.contains_key(name) {
                    remove_shadow(root, name, "$", &mut losses);
                }
            }
        }
        if root.get("settings").is_some_and(Value::is_object) {
            for name in [
                "fps",
                "canvasSize",
                "background",
                "backgroundType",
                "backgroundColor",
                "blurIntensity",
            ] {
                remove_shadow(root, name, "$", &mut losses);
            }
        }
        // Root bookmarks were moved to individual scenes by Classic v1→v2.
        if root
            .get("scenes")
            .and_then(Value::as_array)
            .is_some_and(|scenes| scenes.iter().all(|scene| scene.get("bookmarks").is_some()))
        {
            remove_shadow(root, "bookmarks", "$", &mut losses);
        }
    }
    if let Some(scenes) = raw.get_mut("scenes").and_then(Value::as_array_mut) {
        for (scene_index, scene) in scenes.iter_mut().enumerate() {
            if let Some(tracks) = scene.get_mut("tracks").and_then(Value::as_object_mut) {
                for (category, value) in tracks {
                    if category == "main" {
                        recovery_track(
                            value,
                            &format!("$.scenes[{scene_index}].tracks.main"),
                            &mut losses,
                        );
                    } else if let Some(items) = value.as_array_mut() {
                        for (index, track) in items.iter_mut().enumerate() {
                            recovery_track(
                                track,
                                &format!("$.scenes[{scene_index}].tracks.{category}[{index}]"),
                                &mut losses,
                            );
                        }
                    }
                }
            }
        }
    }
    let mut source = input.clone();
    source.content = raw.to_string();
    (source, losses)
}

fn remove_shadow(
    object: &mut Map<String, Value>,
    name: &str,
    path: &str,
    losses: &mut Vec<CompatibilityFinding>,
) {
    if object.remove(name).is_some() {
        losses.push(CompatibilityFinding {path: format!("{path}.{name}"), code: "LEGACY_SHADOW_FIELD".into(),
            detail: "Classic v31 uses its canonical metadata/settings/params; this obsolete serialized shadow is excluded from the new document. The source snapshot is unchanged.".into()});
    }
}

fn recovery_track(track: &mut Value, path: &str, losses: &mut Vec<CompatibilityFinding>) {
    let Some(elements) = track.get_mut("elements").and_then(Value::as_array_mut) else {
        return;
    };
    for (index, element) in elements.iter_mut().enumerate() {
        let Some(item) = element.as_object_mut() else {
            continue;
        };
        let Some(params) = item.get("params").and_then(Value::as_object).cloned() else {
            continue;
        };
        let path = format!("{path}.elements[{index}]");
        for name in [
            "opacity",
            "blendMode",
            "volume",
            "muted",
            "content",
            "fontSize",
            "fontFamily",
            "color",
            "textAlign",
            "fontWeight",
            "fontStyle",
            "textDecoration",
            "letterSpacing",
            "lineHeight",
        ] {
            if params.contains_key(name) {
                remove_shadow(item, name, &path, losses);
            }
        }
        if let Some(transform) = item.get("transform").and_then(Value::as_object) {
            let represented = transform.iter().all(|(key, value)| match key.as_str() {
                "position" => value.as_object().is_some_and(|position| {
                    position
                        .iter()
                        .all(|(key, value)| matches!(key.as_str(), "x" | "y") && value.is_number())
                        && params.contains_key("transform.positionX")
                        && params.contains_key("transform.positionY")
                }),
                "scale" => {
                    value.is_number()
                        && params.contains_key("transform.scaleX")
                        && params.contains_key("transform.scaleY")
                }
                "scaleX" => value.is_number() && params.contains_key("transform.scaleX"),
                "scaleY" => value.is_number() && params.contains_key("transform.scaleY"),
                "rotate" => value.is_number() && params.contains_key("transform.rotate"),
                _ => false,
            });
            if represented {
                remove_shadow(item, "transform", &path, losses);
            }
        }
    }
}
