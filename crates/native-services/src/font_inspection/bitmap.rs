//! Only the compiled monochrome EBLC/EBDT profile, never color bitmap fallback.
use super::*;
pub(super) fn validate(
    face: &Face<'_>,
    checkpoint: &mut dyn FontImportCheckpoint,
) -> Result<(), EditorError> {
    let raw = face.raw_face();
    let loc = raw
        .table(Tag::from_bytes(b"EBLC"))
        .ok_or_else(|| invalid("Missing EBLC table"))?;
    let data = raw
        .table(Tag::from_bytes(b"EBDT"))
        .ok_or_else(|| invalid("Missing EBDT table"))?;
    if u32_at(loc, 0)? != 0x20000 || u32_at(data, 0)? != 0x20000 || face.tables().ebdt.is_none() {
        return Err(invalid("Invalid monochrome bitmap table version"));
    }
    let strikes = u32_at(loc, 4)? as usize;
    if strikes == 0 || strikes > 256 {
        return Err(invalid("Bitmap strike count exceeds its envelope"));
    }
    span(loc, 8, strikes, 48)?;
    for n in 0..strikes {
        checkpoint.check()?;
        let at = 8 + n * 48;
        let array = u32_at(loc, at)? as usize;
        let table_bytes = u32_at(loc, at + 4)? as usize;
        let subtables = u32_at(loc, at + 8)? as usize;
        let first = u16_at(loc, at + 40)?;
        let last = u16_at(loc, at + 42)?;
        if first > last
            || last >= face.number_of_glyphs()
            || loc[at + 44] == 0
            || loc[at + 45] == 0
            || loc[at + 46] != 1
        {
            return Err(unsupported(
                "Only valid one-bit monochrome bitmap strikes are admitted",
            ));
        }
        if subtables == 0 {
            return Err(invalid("Bitmap strike has no index subtables"));
        }
        span(loc, array, table_bytes, 1)?;
        span(loc, array, subtables, 8)?;
        let mut previous = None;
        for k in 0..subtables {
            checkpoint.check()?;
            let entry = array + k * 8;
            let start = u16_at(loc, entry)?;
            let end = u16_at(loc, entry + 2)?;
            if start > end || start < first || end > last || previous.is_some_and(|p| p >= start) {
                return Err(invalid(
                    "Bitmap index ranges overlap or exceed their strike",
                ));
            }
            previous = Some(end);
            let offset = u32_at(loc, entry + 4)? as usize;
            if offset < subtables * 8 || offset >= table_bytes {
                return Err(invalid("Bitmap index table is outside its strike"));
            }
            let table = array + offset;
            let format = u16_at(loc, table)?;
            let image = u16_at(loc, table + 2)?;
            let base = u32_at(loc, table + 4)? as usize;
            // The admitted original Anonymous Pro strikes use dense format 3 and
            // small byte-aligned mono images. Other formats need explicit pins
            // plus a complete validator; they are not silently substituted.
            if format != 3 || image != 1 {
                return Err(unsupported(
                    "Bitmap index/image format is outside the monochrome profile",
                ));
            }
            if base < 4 {
                return Err(invalid("Bitmap image data overlaps its table header"));
            }
            let count = usize::from(end - start) + 1;
            span(loc, table + 8, count + 1, 2)?;
            if offset + 8 + (count + 1) * 2 > table_bytes {
                return Err(invalid("Bitmap offsets exceed index tables size"));
            }
            let mut prior = 0;
            for i in 0..=count {
                let next = u16_at(loc, table + 8 + i * 2)? as usize;
                if next < prior {
                    return Err(invalid("Bitmap image offsets are not monotonic"));
                }
                span(data, base, next, 1)?;
                if i != 0 && next > prior {
                    checkpoint.check()?;
                    let begin = base + prior;
                    let len = next - prior;
                    span(data, begin, 1, 5)?;
                    // Multiplication follows row alignment, not whole-image bit alignment.
                    let needed =
                        5 + usize::from(data[begin]) * usize::from(data[begin + 1]).div_ceil(8);
                    if needed > len {
                        return Err(invalid("Bitmap glyph pixels exceed their indexed image"));
                    }
                    let glyph = ttf_parser::GlyphId(start + i as u16 - 1);
                    let image = face
                        .glyph_raster_image(glyph, u16::from(loc[at + 44]))
                        .ok_or_else(|| invalid("Declared bitmap glyph could not be parsed"))?;
                    if image.format != ttf_parser::RasterImageFormat::BitmapMono {
                        return Err(unsupported("Bitmap glyph is not monochrome"));
                    }
                }
                prior = next;
            }
        }
    }
    Ok(())
}
