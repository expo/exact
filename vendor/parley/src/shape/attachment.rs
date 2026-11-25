// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::RunAdvances;
use alloc::collections::BTreeMap;
use harfrust::font::{BuiltinFontFuncs, FontFuncs};
use harfrust::{GlyphBuffer, GlyphId};

/// Add one design unit before positioning. GPOS moves each attached
/// mark by the same number of units as its base, even across other glyphs.
pub(super) struct FindBases;

impl FontFuncs for FindBases {
    fn advance_width(&mut self, builtin: &BuiltinFontFuncs<'_>, glyph: GlyphId) -> i32 {
        builtin.advance_width(glyph) + 1
    }
}

pub(super) fn place_marks(
    original: &GlyphBuffer,
    widened: &GlyphBuffer,
    measured: &mut RunAdvances,
    scale: f32,
) {
    if original.len() != widened.len()
        || !original
            .glyph_infos()
            .iter()
            .zip(widened.glyph_infos())
            .all(|(a, b)| a.glyph_id == b.glyph_id && a.cluster == b.cluster)
    {
        return;
    }
    for range in &measured.reattach {
        let original = &original.glyph_positions()[range.clone()];
        let widened = &widened.glyph_positions()[range.clone()];
        let glyphs = &mut measured.glyphs[range.clone()];
        let mut pens = BTreeMap::new();
        let (mut change, mut original_pen, mut measured_pen) = (0i64, 0i64, 0.);
        for ((o, w), g) in original.iter().zip(widened).zip(glyphs.iter()) {
            pens.insert(change, (original_pen, measured_pen));
            original_pen += i64::from(o.x_advance);
            measured_pen += g.advance;
            change += i64::from(w.x_advance) - i64::from(o.x_advance);
        }
        (change, original_pen, measured_pen) = (0, 0, 0.);
        for ((o, w), g) in original.iter().zip(widened).zip(glyphs) {
            if o.x_advance == 0 || w.x_offset != o.x_offset {
                let base = change + i64::from(w.x_offset) - i64::from(o.x_offset);
                if let Some(&(old_base, new_base)) = pens.get(&base) {
                    let anchor = (original_pen + i64::from(o.x_offset) - old_base) as f32 * scale;
                    g.x_offset = (new_base + anchor) - measured_pen;
                }
            }
            original_pen += i64::from(o.x_advance);
            measured_pen += g.advance;
            change += i64::from(w.x_advance) - i64::from(o.x_advance);
        }
    }
}
