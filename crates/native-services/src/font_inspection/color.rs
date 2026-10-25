//! Strict referenced COLR graph validation. ttf-parser's v1 painting path ignores
//! some nested parse errors, so a populated `FaceTables::colr` alone is not proof.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

fn relative(data: &[u8], base: usize, at: usize, width: usize) -> Result<usize, EditorError> {
    span(data, at, 1, width)?;
    let value = if width == 3 {
        usize::from(data[at]) << 16 | usize::from(data[at + 1]) << 8 | usize::from(data[at + 2])
    } else {
        u32_at(data, at)? as usize
    };
    if value == 0 {
        return Err(invalid("Required color table offset is zero"));
    }
    let offset = base
        .checked_add(value)
        .ok_or_else(|| invalid("Color table offset overflow"))?;
    span(data, offset, 1, 1)?;
    Ok(offset)
}
fn palette(data: &[u8]) -> Result<u16, EditorError> {
    let version = u16_at(data, 0)?;
    let entries = u16_at(data, 2)?;
    let palettes = u16_at(data, 4)?;
    let colors = u16_at(data, 6)?;
    if version > 1 || entries == 0 || palettes == 0 || colors == 0 {
        return Err(invalid("Invalid CPAL palette envelope"));
    }
    span(data, 12, palettes as usize, 2)?;
    span(data, u32_at(data, 8)? as usize, colors as usize, 4)?;
    for i in 0..palettes as usize {
        if u32::from(u16_at(data, 12 + i * 2)?) + u32::from(entries) > u32::from(colors) {
            return Err(invalid("CPAL palette exceeds color records"));
        }
    }
    if version == 1 {
        let start = 12 + palettes as usize * 2;
        span(data, start, 3, 4)?;
        for (offset, count, width) in [
            (start, palettes as usize, 4),
            (start + 4, palettes as usize, 2),
            (start + 8, entries as usize, 2),
        ] {
            let offset = u32_at(data, offset)? as usize;
            if offset != 0 {
                span(data, offset, count, width)?;
            }
        }
    }
    if ttf_parser::cpal::Table::parse(data).is_none() {
        return Err(invalid("Declared CPAL table could not be parsed"));
    }
    Ok(entries)
}
fn color_index(data: &[u8], at: usize, entries: u16) -> Result<(), EditorError> {
    let index = u16_at(data, at)?;
    if index != u16::MAX && index >= entries {
        return Err(invalid("COLR references an absent palette color"));
    }
    Ok(())
}
fn alpha(data: &[u8], at: usize) -> Result<(), EditorError> {
    if u16_at(data, at)? > 0x4000 {
        return Err(invalid("COLR alpha is outside zero to one"));
    }
    Ok(())
}
pub(super) fn validate(
    face: &Face<'_>,
    profile: FontRenderProfile,
    checkpoint: &mut dyn FontImportCheckpoint,
) -> Result<(), EditorError> {
    let data = face
        .raw_face()
        .table(Tag::from_bytes(b"COLR"))
        .ok_or_else(|| invalid("Approved color source has no COLR table"))?;
    let cpal = face
        .raw_face()
        .table(Tag::from_bytes(b"CPAL"))
        .ok_or_else(|| invalid("Approved color source has no CPAL table"))?;
    let entries = palette(cpal)?;
    let version = u16_at(data, 0)?;
    let expected = if matches!(profile, FontRenderProfile::ColrV0) {
        0
    } else {
        1
    };
    if version != expected || face.tables().colr.is_none() {
        return Err(invalid("COLR version does not match its approved profile"));
    }
    let base_count = u16_at(data, 2)? as usize;
    let bases = u32_at(data, 4)? as usize;
    let layers = u32_at(data, 8)? as usize;
    let layer_count = u16_at(data, 12)? as usize;
    span(data, bases, base_count, 6)?;
    span(data, layers, layer_count, 4)?;
    let glyphs = face.number_of_glyphs();
    let mut prior = None;
    let mut v0 = BTreeSet::new();
    for n in 0..base_count {
        checkpoint.check()?;
        let at = bases + n * 6;
        let glyph = u16_at(data, at)?;
        if glyph >= glyphs
            || prior.is_some_and(|p| p >= glyph)
            || usize::from(u16_at(data, at + 2)?) + usize::from(u16_at(data, at + 4)?) > layer_count
        {
            return Err(invalid("Invalid COLRv0 base glyph or layer range"));
        }
        prior = Some(glyph);
        v0.insert(glyph);
    }
    for n in 0..layer_count {
        checkpoint.check()?;
        let at = layers + n * 4;
        if u16_at(data, at)? >= glyphs {
            return Err(invalid("COLR layer glyph is outside the face"));
        }
        color_index(data, at + 2, entries)?;
    }
    if version == 0 {
        if base_count == 0 {
            return Err(invalid("COLRv0 has no color glyphs"));
        }
        return Ok(());
    }
    span(data, 0, 1, 34)?;
    if u32_at(data, 26)? != 0 || u32_at(data, 30)? != 0 {
        return Err(unsupported(
            "Variable color paint data requires a fully static derived source",
        ));
    }
    let root = relative(data, 0, 14, 4)?;
    let count = u32_at(data, root)? as usize;
    span(data, root + 4, count, 6)?;
    if count == 0 && base_count == 0 {
        return Err(invalid("COLRv1 has no color glyphs"));
    }
    let mut roots = BTreeMap::new();
    let mut prior = None;
    for n in 0..count {
        checkpoint.check()?;
        let at = root + 4 + n * 6;
        let glyph = u16_at(data, at)?;
        if glyph >= glyphs || prior.is_some_and(|p| p >= glyph) {
            return Err(invalid("COLRv1 base glyphs are not unique and sorted"));
        }
        roots.insert(glyph, relative(data, root, at + 2, 4)?);
        prior = Some(glyph);
    }
    let mut layer_paints = Vec::new();
    let layer_list = u32_at(data, 18)? as usize;
    if layer_list != 0 {
        let count = u32_at(data, layer_list)? as usize;
        span(data, layer_list + 4, count, 4)?;
        for n in 0..count {
            layer_paints.push(relative(data, layer_list, layer_list + 4 + n * 4, 4)?);
        }
    }
    let clips = u32_at(data, 22)? as usize;
    if clips != 0 {
        span(data, clips, 1, 5)?;
        if data[clips] != 1 {
            return Err(invalid("Unsupported COLRv1 clip list format"));
        }
        let count = u32_at(data, clips + 1)? as usize;
        span(data, clips + 5, count, 7)?;
        let mut end = None;
        for n in 0..count {
            checkpoint.check()?;
            let at = clips + 5 + n * 7;
            let first = u16_at(data, at)?;
            let last = u16_at(data, at + 2)?;
            if first > last || last >= glyphs || end.is_some_and(|end| end >= first) {
                return Err(invalid("Invalid COLRv1 clip glyph range"));
            }
            end = Some(last);
            let bounds = relative(data, clips, at + 4, 3)?;
            span(data, bounds, 1, 9)?;
            if data[bounds] != 1 {
                return Err(unsupported("Variable or unknown COLRv1 clip box"));
            }
            if (u16_at(data, bounds + 1)? as i16) > (u16_at(data, bounds + 5)? as i16)
                || (u16_at(data, bounds + 3)? as i16) > (u16_at(data, bounds + 7)? as i16)
            {
                return Err(invalid("Inverted COLRv1 clip box"));
            }
        }
    }
    let mut graph = Graph {
        data,
        entries,
        glyphs,
        roots: &roots,
        v0: &v0,
        layers: &layer_paints,
        visiting: BTreeSet::new(),
        done: BTreeSet::new(),
        checkpoint,
    };
    for offset in roots.values().chain(layer_paints.iter()) {
        graph.paint(*offset, 0)?;
    }
    Ok(())
}
struct Graph<'a, 'p> {
    data: &'a [u8],
    entries: u16,
    glyphs: u16,
    roots: &'a BTreeMap<u16, usize>,
    v0: &'a BTreeSet<u16>,
    layers: &'a [usize],
    visiting: BTreeSet<usize>,
    done: BTreeSet<usize>,
    checkpoint: &'p mut dyn FontImportCheckpoint,
}
impl Graph<'_, '_> {
    fn paint(&mut self, at: usize, depth: usize) -> Result<(), EditorError> {
        self.checkpoint.check()?;
        if self.done.contains(&at) {
            return Ok(());
        }
        if depth >= 64
            || self.done.len() + self.visiting.len() >= 1_000_000
            || !self.visiting.insert(at)
        {
            return Err(invalid(
                "COLR paint graph is cyclic or exceeds its bounded envelope",
            ));
        }
        let data = self.data;
        span(data, at, 1, 1)?;
        let child = |field| relative(data, at, at + field, 3);
        match data[at] {
            1 => {
                span(data, at, 1, 6)?;
                let start = u32_at(data, at + 2)? as usize;
                let count = data[at + 1] as usize;
                let end = start
                    .checked_add(count)
                    .ok_or_else(|| invalid("COLR layer range overflow"))?;
                let nodes = self
                    .layers
                    .get(start..end)
                    .ok_or_else(|| invalid("COLR paint references missing layers"))?
                    .to_vec();
                for node in nodes {
                    self.paint(node, depth + 1)?;
                }
            }
            2 => {
                color_index(data, at + 1, self.entries)?;
                alpha(data, at + 3)?;
            }
            4 | 6 | 8 => {
                span(data, at, 1, if data[at] == 8 { 12 } else { 16 })?;
                let line = child(1)?;
                span(data, line, 1, 3)?;
                if data[line] > 2 {
                    return Err(invalid("Unknown gradient extension"));
                }
                let count = u16_at(data, line + 1)? as usize;
                if count == 0 {
                    return Err(invalid("Color gradient has no stops"));
                }
                span(data, line + 3, count, 6)?;
                for n in 0..count {
                    self.checkpoint.check()?;
                    let stop = line + 3 + n * 6;
                    color_index(data, stop + 2, self.entries)?;
                    alpha(data, stop + 4)?;
                }
            }
            10 => {
                span(data, at, 1, 6)?;
                if u16_at(data, at + 4)? >= self.glyphs {
                    return Err(invalid("COLRv1 paint glyph is outside the face"));
                }
                self.paint(child(1)?, depth + 1)?;
            }
            11 => {
                let glyph = u16_at(data, at + 1)?;
                if let Some(offset) = self.roots.get(&glyph) {
                    self.paint(*offset, depth + 1)?;
                } else if !self.v0.contains(&glyph) {
                    return Err(invalid("COLRv1 paint references absent color glyph"));
                }
            }
            12 => {
                span(data, at, 1, 7)?;
                let matrix = child(4)?;
                span(data, matrix, 1, 24)?;
                self.paint(child(1)?, depth + 1)?;
            }
            14 | 16 | 18 | 20 | 22 | 24 | 26 | 28 | 30 => {
                let size = match data[at] {
                    14 | 16 | 28 => 8,
                    18 | 30 => 12,
                    20 | 24 => 6,
                    22 | 26 => 10,
                    _ => unreachable!(),
                };
                span(data, at, 1, size)?;
                self.paint(child(1)?, depth + 1)?;
            }
            32 => {
                span(data, at, 1, 8)?;
                if data[at + 4] > 27 {
                    return Err(invalid("Unknown COLRv1 composite mode"));
                }
                self.paint(child(1)?, depth + 1)?;
                self.paint(child(5)?, depth + 1)?;
            }
            _ => {
                return Err(unsupported(
                    "Variable or unknown COLRv1 paint format is not admitted",
                ));
            }
        }
        self.visiting.remove(&at);
        self.done.insert(at);
        Ok(())
    }
}
pub(super) fn validate_svg_envelope(
    data: &[u8],
    glyphs: u16,
    checkpoint: &mut dyn FontImportCheckpoint,
) -> Result<(), EditorError> {
    if u16_at(data, 0)? != 0 || u32_at(data, 6)? != 0 {
        return Err(invalid("Invalid declared SVG table header"));
    }
    let start = u32_at(data, 2)? as usize;
    let count = u16_at(data, start)? as usize;
    span(data, start + 2, count, 12)?;
    let mut end = None;
    for n in 0..count {
        checkpoint.check()?;
        let at = start + 2 + n * 12;
        let first = u16_at(data, at)?;
        let last = u16_at(data, at + 2)?;
        if first > last || last >= glyphs || end.is_some_and(|end| end >= first) {
            return Err(invalid("Invalid SVG glyph range"));
        }
        end = Some(last);
        let offset = start
            .checked_add(u32_at(data, at + 4)? as usize)
            .ok_or_else(|| invalid("SVG offset overflow"))?;
        let len = u32_at(data, at + 8)? as usize;
        if len == 0 {
            return Err(invalid("Empty declared SVG document"));
        }
        span(data, offset, len, 1)?;
    }
    Ok(())
}
