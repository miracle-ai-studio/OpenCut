//! Classic effects/types.ts and effects/definitions/index.ts. The original
//! registry contains blur only; unknown effects never become no-op layers.
use super::*;
pub(super) fn kind(
    r: &mut Review,
    name: &str,
    params: &Map<String, Value>,
    p: &str,
) -> Option<EffectKind> {
    if name != "blur" {
        r.block(p, "UNSUPPORTED_DEPENDENCY", "Unknown Classic effect type");
        return None;
    }
    r.keys(params, p, &["intensity"]);
    if !params.contains_key("intensity") {
        r.block(
            &format!("{p}.intensity"),
            "MISSING_EFFECT_PARAMETER",
            "Persisted effect parameters must be complete",
        );
    }
    Some(EffectKind::ClassicBlur {
        intensity: r.num(params, "intensity", p, 15.),
    })
}
pub(super) fn parse(
    r: &mut Review,
    value: Option<&Value>,
    p: &str,
    target: &str,
) -> Vec<SceneEffect> {
    let Some(value) = value else { return vec![] };
    let Some(values) = value.as_array() else {
        r.block(p, "INVALID_DEPENDENCY", "Expected effects array");
        return vec![];
    };
    if !values.is_empty() && !matches!(target, "video" | "image" | "text" | "graphic") {
        r.block(
            p,
            "UNSUPPORTED_EFFECT_TARGET",
            "Classic nested effects require visual content",
        );
        return vec![];
    }
    values
        .iter()
        .enumerate()
        .filter_map(|(i, v)| {
            let p = format!("{p}[{i}]");
            let o = r.object(v, &p)?;
            r.keys(o, &p, &["id", "type", "enabled", "params"]);
            for key in ["id", "type", "enabled", "params"] {
                if !o.contains_key(key) {
                    r.block(
                        &format!("{p}.{key}"),
                        "MISSING_EFFECT_PARAMETER",
                        "Persisted effects require complete fields",
                    );
                }
            }
            let name = r.string(o, "type", &p, None);
            if name != "blur" {
                r.block(
                    &format!("{p}.type"),
                    "UNSUPPORTED_DEPENDENCY",
                    "Unknown Classic effect type",
                );
                return None;
            }
            let pp = format!("{p}.params");
            let params = r.object(&v["params"], &pp)?;
            Some(SceneEffect {
                id: r.string(o, "id", &p, None),
                enabled: r.boolean(o, "enabled", &p, true),
                effect: kind(r, &name, params, &pp)?,
            })
        })
        .collect()
}
