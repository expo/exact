//! A run of ordinary Latin glyphs whose font can only adjust advances has
//! its origins at the pens returned by Paint. Inspect the face once so a long
//! paragraph does not also need a PositionedGlyphs allocation.
use parley::AdvanceRequest;
use skrifa::raw::tables::gpos::{PositionSubtables, ValueFormat};
use skrifa::raw::tables::layout::CoverageTable;
use skrifa::raw::{FontRef, TableProvider};
use skrifa::{GlyphId, MetadataProvider, Tag};
use std::cell::RefCell;

struct Face {
    id: u64,
    index: u32,
    ascii: Option<[u32; 95]>,
}

thread_local! { static LAST: RefCell<Option<Face>> = const { RefCell::new(None) }; }

pub(super) fn on_pens(run: &AdvanceRequest<'_>) -> bool {
    if run.rtl
        || !run.features.is_empty()
        || run.text.len() != run.infos.len()
        || !run.text.bytes().all(|c| (b' '..=b'~').contains(&c))
    {
        return false;
    }
    LAST.with_borrow_mut(|last| {
        let id = run.font.blob.id();
        let index = run.font.index;
        if last.as_ref().is_none_or(|f| f.id != id || f.index != index) {
            let ascii = FontRef::from_index(run.font.blob.data(), index)
                .ok()
                .and_then(inspect);
            *last = Some(Face { id, index, ascii });
        }
        last.as_ref()
            .and_then(|f| f.ascii.as_ref())
            .is_some_and(|ids| {
                run.text
                    .bytes()
                    .zip(run.infos)
                    .zip(run.positions)
                    .enumerate()
                    .all(|(i, ((c, g), p))| {
                        g.cluster as usize == i
                            && ids[(c - b' ') as usize] == g.glyph_id
                            && p.x_offset == 0
                    })
            })
    })
}

fn inspect(font: FontRef<'_>) -> Option<[u32; 95]> {
    if [b"kern", b"kerx", b"trak", b"morx", b"mort", b"Silf"]
        .iter()
        .any(|tag| font.table_data(Tag::new(*tag)).is_some())
    {
        return None;
    }
    let cmap = font.charmap();
    let ids = core::array::from_fn(|i| cmap.map(32 + i as u32).map_or(u32::MAX, GlyphId::to_u32));
    if font.table_data(Tag::new(b"GPOS")).is_none() {
        return Some(ids);
    }
    let gpos = font.gpos().ok()?;
    if gpos.feature_variations().is_some() {
        return None;
    }

    // Capital spacing is opt-in. A required language feature or a contextual
    // lookup could enable it independently of the paint's feature settings.
    let scripts = gpos.script_list().ok()?;
    for rec in scripts.script_records() {
        let script = rec.script(scripts.offset_data()).ok()?;
        for lang in script.default_lang_sys().into_iter().chain(
            script
                .lang_sys_records()
                .iter()
                .map(|r| r.lang_sys(script.offset_data())),
        ) {
            if lang.ok()?.required_feature_index() != u16::MAX {
                return None;
            }
        }
    }
    let features = gpos.feature_list().ok()?;
    let lookups = gpos.lookup_list().ok()?;
    let mut active = vec![false; lookups.lookup_count() as usize];
    for rec in features.feature_records() {
        if rec.feature_tag() == Tag::new(b"cpsp") {
            continue;
        }
        for at in rec
            .feature(features.offset_data())
            .ok()?
            .lookup_list_indices()
        {
            *active.get_mut(at.get() as usize)? = true;
        }
    }
    let x = ValueFormat::X_PLACEMENT | ValueFormat::X_PLACEMENT_DEVICE;
    let touches_ascii = |coverage: CoverageTable<'_>| {
        ids.iter()
            .any(|&id| id != u32::MAX && coverage.get(GlyphId::new(id)).is_some())
    };
    for (i, lookup) in lookups.lookups().iter().enumerate() {
        if !active[i] {
            continue;
        }
        match lookup.ok()?.subtables().ok()? {
            PositionSubtables::Single(t) => {
                for s in t.iter() {
                    if s.ok()?.value_format().intersects(x) {
                        return None;
                    }
                }
            }
            PositionSubtables::Pair(t) => {
                for s in t.iter() {
                    let s = s.ok()?;
                    if (s.value_format1() | s.value_format2()).intersects(x) {
                        return None;
                    }
                }
            }
            PositionSubtables::MarkToBase(t) => {
                for s in t.iter() {
                    if touches_ascii(s.ok()?.mark_coverage().ok()?) {
                        return None;
                    }
                }
            }
            PositionSubtables::MarkToLig(t) => {
                for s in t.iter() {
                    if touches_ascii(s.ok()?.mark_coverage().ok()?) {
                        return None;
                    }
                }
            }
            PositionSubtables::MarkToMark(t) => {
                for s in t.iter() {
                    if touches_ascii(s.ok()?.mark1_coverage().ok()?) {
                        return None;
                    }
                }
            }
            _ => return None,
        }
    }
    Some(ids)
}
