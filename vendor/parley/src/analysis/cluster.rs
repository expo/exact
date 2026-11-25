// Copyright 2025 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::vec::Vec;
use icu_normalizer::properties::Decomposed;

use crate::analysis::AnalysisDataSources;

#[derive(Debug, Default)]
pub(crate) struct CharCluster {
    pub chars: Vec<Char>,
    pub emoji_presentation: Option<bool>,
    pub map_len: usize,
    pub start: u32,
    pub end: u32,
    pub force_normalize: bool,
    comp: Form,
    decomp: Form,
    best_ratio: f64,
}

impl CharCluster {
    pub(crate) fn range(&self) -> SourceRange {
        SourceRange {
            start: self.start,
            end: self.end,
        }
    }
}

/// Source range of a cluster in code units.
#[derive(Copy, Clone)]
pub(crate) struct SourceRange {
    pub start: u32,
    pub end: u32,
}

#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct Char {
    /// The character.
    pub ch: char,
    /// Whether the character is a control.
    pub is_control_character: bool,
    /// True if the character should be considered when mapping glyphs.
    pub contributes_to_shaping: bool,
    /// Indexes into the list of styles for the containing text run, to find the style applicable
    /// to this character.
    pub style_index: u16,
}

/// Whitespace content of a cluster.
#[derive(Copy, Clone, PartialOrd, Ord, PartialEq, Eq, Debug)]
#[repr(u8)]
pub(crate) enum Whitespace {
    /// Not a space.
    None = 0,
    /// Standard space.
    Space = 1,
    /// Non-breaking space (U+00A0).
    NoBreakSpace = 2,
    /// Horizontal tab.
    Tab = 3,
    /// Newline (CR, LF, CRLF, LS, or PS).
    Newline = 4,
}

impl Whitespace {
    /// Returns true for space or no break space.
    pub(crate) fn is_space_or_nbsp(self) -> bool {
        matches!(self, Self::Space | Self::NoBreakSpace)
    }
}

/// Iterative status of mapping a character cluster to nominal glyph identifiers.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum Status {
    /// Mapping should be skipped.
    Discard,
    /// The best mapping so far.
    Keep,
    /// Complete mapping.
    Complete,
}

impl CharCluster {
    pub(crate) fn clear(&mut self) {
        self.chars.clear();
        self.emoji_presentation = None;
        self.map_len = 0;
        self.start = 0;
        self.end = 0;
        self.force_normalize = false;
        self.comp.clear();
        self.decomp.clear();
        self.best_ratio = 0.;
    }

    /// Returns the primary style index for the cluster.
    #[inline(always)]
    pub(crate) fn style_index(&self) -> u16 {
        self.chars[0].style_index
    }

    #[inline(always)]
    fn contributes_to_shaping(ch: char, analysis_data_sources: &AnalysisDataSources) -> bool {
        let props = analysis_data_sources.properties(ch);
        crate::analysis::contributes_to_shaping(props.general_category(), props.script())
    }

    fn decomposed(&mut self, analysis_data_sources: &AnalysisDataSources) -> Option<&[Char]> {
        match self.decomp.state {
            FormState::Invalid => None,
            FormState::None => {
                self.decomp.state = FormState::Invalid;

                // Only attempt pairwise normalization (1 <-> 2 characters)
                if self.chars.len() != 1 {
                    return None;
                }

                let decomposer = analysis_data_sources.decomposing_normalizer();
                let decomp = decomposer.decompose(self.chars[0].ch);
                match decomp {
                    Decomposed::Default | Decomposed::Singleton(_) => {
                        return None;
                    }
                    Decomposed::Expansion(a, b) => {
                        let mut copy = self.chars[0];
                        copy.ch = a;
                        copy.contributes_to_shaping =
                            Self::contributes_to_shaping(a, analysis_data_sources);
                        self.decomp.chars[0] = copy;

                        copy.ch = b;
                        copy.contributes_to_shaping =
                            Self::contributes_to_shaping(b, analysis_data_sources);
                        self.decomp.chars[1] = copy;

                        self.decomp.len = 2;
                    }
                }

                self.decomp.state = FormState::Valid;
                self.decomp.setup();
                Some(self.decomp.chars())
            }
            FormState::Valid => Some(self.decomp.chars()),
        }
    }

    fn composed(&mut self, analysis_data_sources: &AnalysisDataSources) -> Option<&[Char]> {
        match self.comp.state {
            FormState::Invalid => None,
            FormState::None => {
                self.comp.state = FormState::Invalid;

                // Only attempt pairwise normalization (1 <-> 2 characters)
                if self.chars.len() != 2 {
                    return None;
                }

                let composer = analysis_data_sources.composing_normalizer();
                let ch = composer.compose(self.chars[0].ch, self.chars[1].ch)?;
                let mut copy = self.chars[0];
                copy.ch = ch;
                copy.contributes_to_shaping =
                    Self::contributes_to_shaping(ch, analysis_data_sources);
                self.comp.chars[0] = copy;
                self.comp.len = 1;

                self.comp.state = FormState::Valid;
                self.comp.setup();
                Some(self.comp.chars())
            }
            FormState::Valid => Some(self.comp.chars()),
        }
    }

    pub(crate) fn map(
        &mut self,
        has_glyph: impl Fn(char) -> bool,
        analysis_data_sources: &AnalysisDataSources,
    ) -> Status {
        let mut ratio = if self.force_normalize && self.composed(analysis_data_sources).is_some() {
            self.comp.coverage(&has_glyph)
        } else {
            0.
        };
        if ratio < 1. {
            ratio = ratio.max(coverage(&self.chars, self.map_len, &has_glyph));
        }
        if ratio < 1. && self.decomposed(analysis_data_sources).is_some() {
            ratio = ratio.max(self.decomp.coverage(&has_glyph));
            if ratio < 1. && !self.force_normalize && self.composed(analysis_data_sources).is_some()
            {
                ratio = ratio.max(self.comp.coverage(&has_glyph));
            }
        }
        if ratio >= 1. {
            Status::Complete
        } else if ratio > self.best_ratio {
            self.best_ratio = ratio;
            Status::Keep
        } else {
            Status::Discard
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum FormState {
    None,
    Valid,
    Invalid,
}

#[derive(Clone, Debug)]
pub(crate) struct Form {
    chars: [Char; 2],
    len: u8,
    map_len: u8,
    has_contributing: bool,
    state: FormState,
}

impl Default for Form {
    fn default() -> Self {
        Self::new()
    }
}

impl Form {
    fn new() -> Self {
        Self {
            chars: [Char::default(), Char::default()],
            len: 0,
            map_len: 0,
            has_contributing: false,
            state: FormState::None,
        }
    }

    fn clear(&mut self) {
        self.chars = [Char::default(), Char::default()];
        self.len = 0;
        self.map_len = 0;
        self.has_contributing = false;
        self.state = FormState::None;
    }

    #[inline(always)]
    fn chars(&self) -> &[Char] {
        &self.chars[..self.len as usize]
    }

    #[inline(always)]
    fn setup(&mut self) {
        self.map_len = (self
            .chars()
            .iter()
            .filter(|c| !c.is_control_character)
            .count() as u8)
            .max(1);
        self.has_contributing = self.chars().iter().any(|c| c.contributes_to_shaping);
    }

    fn coverage(&self, has_glyph: &impl Fn(char) -> bool) -> f64 {
        let required = if self.has_contributing {
            self.map_len as usize
        } else {
            0
        };
        coverage(self.chars(), required, has_glyph)
    }
}

// The shaper looks up glyph IDs after choosing the face. Font fallback needs only coverage.
fn coverage(chars: &[Char], required: usize, has_glyph: &impl Fn(char) -> bool) -> f64 {
    if required == 0 {
        return 1.;
    }
    let covered = chars
        .iter()
        .filter(|c| c.contributes_to_shaping && has_glyph(c.ch))
        .count();
    covered as f64 / required as f64
}
