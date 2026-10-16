//! Finite structural opt-in for an explicit actual-face/requested-style binding.
//! Approved source/proof/hash/license checks belong to application and storage.
use super::*;
use opencut_contracts::font_synthesis_v1::{
    FONT_SYNTHESIS_BINDING_ID_PREFIX_V1, FONT_SYNTHESIS_PROVIDER_V1,
};

pub fn is_font_synthesis_resource_v1(font: &FontResourceV4) -> bool {
    matches!(&font.origin, ResourceOriginV4::Generated { provider, .. }
        if provider == FONT_SYNTHESIS_PROVIDER_V1)
}

pub fn validate_font_synthesis_resource_v1(font: &FontResourceV4) -> Result<(), EditorError> {
    if !is_font_synthesis_resource_v1(font) {
        return Ok(());
    }
    let ResourceOriginV4::Generated { generation_id, .. } = &font.origin else {
        unreachable!()
    };
    let digest = generation_id.strip_prefix(FONT_SYNTHESIS_BINDING_ID_PREFIX_V1);
    if font.style != FontStyleV4::Normal
        || !matches!(font.weight, 400 | 700)
        || font.face_index != 0
        || !digest.is_some_and(|value| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
    {
        return Err(invalid(
            "Font synthesis resource requires the actual normal 400/700 face and a canonical binding identity",
        ));
    }
    Ok(())
}
