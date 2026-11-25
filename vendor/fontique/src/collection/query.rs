// Copyright 2024 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Query support.

use crate::{Charmap, CharmapIndex};

use super::super::{Collection, SourceCache};

use alloc::vec::Vec;
use parlance::Script;
use smallvec::SmallVec;

use super::{
    super::{Attributes, Blob, FallbackKey, FamilyId, FamilyInfo, GenericFamily, Synthesis},
    Inner,
};

#[derive(Clone, Default)]
pub(super) struct QueryState {
    families: Vec<CachedFamily>,
    fallback_families: Vec<CachedFamily>,
}

impl QueryState {
    fn clear(&mut self) {
        self.families.clear();
        self.fallback_families.clear();
    }
}

/// State for font selection.
///
/// Instances of this can be obtained from [`Collection::query`].
pub struct Query<'a> {
    symbol_fallback: Option<&'a super::SymbolFallback>,
    collection: &'a mut Inner,
    state: &'a mut QueryState,
    source_cache: &'a mut SourceCache,
    attributes: Attributes,
    fallbacks: Option<FallbackKey>,
}

impl<'a> Query<'a> {
    pub(super) fn new(collection: &'a mut Collection, source_cache: &'a mut SourceCache) -> Self {
        collection.query_state.clear();
        Self {
            symbol_fallback: collection.symbol_fallback.as_deref(),
            collection: &mut collection.inner,
            state: &mut collection.query_state,
            source_cache,
            attributes: Attributes::default(),
            fallbacks: None,
        }
    }

    /// Sets the ordered sequence of families to match against.
    pub fn set_families<'f, I>(&mut self, families: I)
    where
        I: IntoIterator,
        I::Item: Into<QueryFamily<'f>>,
    {
        self.state.families.clear();
        for family in families {
            let family = family.into();
            match family {
                QueryFamily::Named(name) => {
                    if let Some(id) = self.collection.family_id(name) {
                        self.state.families.push(CachedFamily::new(id));
                    }
                }
                QueryFamily::Id(id) => {
                    self.state.families.push(CachedFamily::new(id));
                }
                QueryFamily::Generic(generic) => {
                    for id in self.collection.generic_families(generic) {
                        self.state.families.push(CachedFamily::new(id));
                    }
                }
            }
        }
    }

    /// Sets the primary attributes to match against.
    pub fn set_attributes(&mut self, attributes: Attributes) {
        if self.attributes != attributes {
            for family in &mut self.state.families {
                family.clear_fonts();
            }
            for family in &mut self.state.fallback_families {
                family.clear_fonts();
            }
            self.attributes = attributes;
        }
    }

    /// Sets the script and locale for fallback fonts.
    pub fn set_fallbacks(&mut self, key: impl Into<FallbackKey>) {
        let key = key.into();
        if self.fallbacks != Some(key) {
            self.state.fallback_families.clear();
            self.state.fallback_families.extend(
                self.collection
                    .fallback_families(key)
                    .map(CachedFamily::new),
            );
            // HACK: always add a Han font to the fallback list to capture
            // punctuation in the common script
            // See <https://github.com/linebender/parley/issues/597>
            self.state.fallback_families.extend(
                self.collection
                    .fallback_families(FallbackKey::new(Script::from_bytes(*b"Hani"), None))
                    .map(CachedFamily::new),
            );
            self.fallbacks = Some(key);
        }
    }

    /// Invokes the given callback with all fonts that match the current
    /// settings.
    ///
    /// Return [`QueryStatus::Stop`] to end iterating over the matching
    /// fonts or [`QueryStatus::Continue`] to continue iterating.
    pub fn matches_with(&mut self, mut f: impl FnMut(&QueryFont) -> QueryStatus) {
        self.matches_for_symbol(None, |font, _| f(font));
    }

    /// Tries the requested families, the platform's face for a bare symbol,
    /// then the script fallbacks. The flag identifies a platform presentation.
    pub fn matches_for_symbol(
        &mut self,
        symbol: Option<(char, GenericFamily)>,
        mut f: impl FnMut(&QueryFont, bool) -> QueryStatus,
    ) {
        let select = self.symbol_fallback.zip(symbol);
        if matches_in(
            self.collection,
            self.source_cache,
            self.attributes,
            &mut self.state.families,
            |font| f(font, select.is_some()),
        ) {
            return;
        }
        if let Some(((_, index), family)) = select
            .and_then(|(select, (ch, base))| select(ch, self.attributes, base))
            .and_then(|face| self.collection.family(face.0).map(|family| (face, family)))
        {
            if load_font(
                &family,
                self.attributes,
                &mut Entry::Vacant,
                index,
                self.source_cache,
            )
            .is_some_and(|font| f(font, true) == QueryStatus::Stop)
            {
                return;
            }
        }
        matches_in(
            self.collection,
            self.source_cache,
            self.attributes,
            &mut self.state.fallback_families,
            |font| f(font, false),
        );
    }
}

fn matches_in(
    collection: &mut Inner,
    cache: &mut SourceCache,
    attrs: Attributes,
    families: &mut [CachedFamily],
    mut f: impl FnMut(&QueryFont) -> QueryStatus,
) -> bool {
    for family in families {
        match &mut family.family {
            Entry::Error => continue,
            Entry::Ok(..) => {}
            status @ Entry::Vacant => {
                if let Some(info) = collection.family(family.id) {
                    *status = Entry::Ok(info);
                } else {
                    *status = Entry::Error;
                    continue;
                }
            }
        }
        let Entry::Ok(family_info) = &family.family else {
            continue;
        };
        let default = family_info.default_font_index();
        let best = family.best.get_or_insert_with(|| {
            (
                family_info
                    .match_index(attrs.width, attrs.style, attrs.weight, true)
                    .unwrap_or(default),
                Entry::Vacant,
            )
        });
        if load_font(family_info, attrs, &mut best.1, best.0, cache)
            .is_some_and(|font| f(font) == QueryStatus::Stop)
        {
            return true;
        }
        // Android splits Noto Sans Symbols across files with the same family
        // and style. Look for those subsets only after the best face misses.
        let others = family.others.get_or_insert_with(|| {
            let mut fonts = SmallVec::<[(usize, Entry<QueryFont>); 1]>::new();
            if let Some(face) = family_info.fonts().get(best.0) {
                for (i, sibling) in family_info.fonts().iter().enumerate() {
                    if i != best.0
                        && sibling.style() == face.style()
                        && sibling.matching_width(attrs.width) == face.matching_width(attrs.width)
                        && sibling.matching_weight(attrs.weight)
                            == face.matching_weight(attrs.weight)
                    {
                        fonts.push((i, Entry::Vacant));
                    }
                }
            }
            if default != best.0 && !fonts.iter().any(|(i, _)| *i == default) {
                fonts.push((default, Entry::Vacant));
            }
            fonts
        });
        for (index, entry) in others {
            if let Some(font) = load_font(family_info, attrs, entry, *index, cache) {
                if f(font) == QueryStatus::Stop {
                    return true;
                }
            }
        }
    }
    false
}

impl Drop for Query<'_> {
    fn drop(&mut self) {
        self.state.clear();
    }
}

/// Determines whether a font query operation will continue.
///
/// See [`Query::matches_with`].
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum QueryStatus {
    /// Query should continue with the next font.
    Continue,
    /// Query should stop.
    Stop,
}

/// Family descriptor for a font query.
///
/// This allows [`Query::set_families`] to
/// take a variety of family types.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum QueryFamily<'a> {
    /// A named font family.
    Named(&'a str),
    /// An identifier for a font family.
    Id(FamilyId),
    /// A generic font family.
    Generic(GenericFamily),
}

impl<'a> From<&'a str> for QueryFamily<'a> {
    fn from(value: &'a str) -> Self {
        Self::Named(value)
    }
}

impl From<FamilyId> for QueryFamily<'static> {
    fn from(value: FamilyId) -> Self {
        Self::Id(value)
    }
}

impl From<GenericFamily> for QueryFamily<'static> {
    fn from(value: GenericFamily) -> Self {
        Self::Generic(value)
    }
}

/// Candidate font generated by a [`Query`].
#[derive(Clone, Debug)]
pub struct QueryFont {
    /// Family identifier and index of the font in the family font list.
    pub family: (FamilyId, usize),
    /// Blob containing the font data.
    pub blob: Blob<u8>,
    /// Index of a font in a font collection (`ttc`) file.
    pub index: u32,
    /// Whether the face contains colour glyph tables.
    pub has_color: bool,
    /// Synthesis suggestions for this font based on the requested attributes.
    pub synthesis: Synthesis,
    /// Data used for constructing a character map for this font.
    pub charmap_index: CharmapIndex,
}

impl QueryFont {
    /// Attempts to construct a [Charmap] for this font.
    pub fn charmap(&self) -> Option<Charmap<'_>> {
        self.charmap_index.charmap(self.blob.as_ref())
    }
}

fn load_font<'a>(
    family: &FamilyInfo,
    attributes: Attributes,
    font: &'a mut Entry<QueryFont>,
    family_index: usize,
    source_cache: &mut SourceCache,
) -> Option<&'a QueryFont> {
    match font {
        Entry::Error => None,
        Entry::Ok(font) => Some(font),
        status @ Entry::Vacant => {
            // Set to error in case we fail. This simplifies
            // the following code.
            *status = Entry::Error;
            let font_info = family.fonts().get(family_index)?;
            let blob = font_info.load(Some(source_cache))?;
            let blob_index = font_info.index();
            let synthesis =
                font_info.synthesis(attributes.width, attributes.style, attributes.weight);
            *status = Entry::Ok(QueryFont {
                family: (family.id(), family_index),
                blob: blob.clone(),
                index: blob_index,
                has_color: read_fonts::FontRef::from_index(blob.as_ref(), blob_index).is_ok_and(
                    |face| {
                        face.table_directory().table_records().iter().any(|record| {
                            matches!(
                                &record.tag().to_be_bytes(),
                                b"COLR" | b"CBDT" | b"sbix" | b"SVG "
                            )
                        })
                    },
                ),
                synthesis,
                charmap_index: font_info.charmap_index(),
            });
            if let Entry::Ok(font) = status {
                Some(font)
            } else {
                None
            }
        }
    }
}

#[derive(Clone)]
struct CachedFamily {
    id: FamilyId,
    family: Entry<FamilyInfo>,
    best: Option<(usize, Entry<QueryFont>)>,
    others: Option<SmallVec<[(usize, Entry<QueryFont>); 1]>>,
}

impl CachedFamily {
    fn new(id: FamilyId) -> Self {
        Self {
            id,
            family: Entry::Vacant,
            best: None,
            others: None,
        }
    }

    fn clear_fonts(&mut self) {
        self.best = None;
        self.others = None;
    }
}

#[derive(Clone)]
enum Entry<T> {
    Ok(T),
    Vacant,
    Error,
}
