//! An editable width shares longer hard lines and packs short ones together. The
//! paragraph's visual lines keep global ranges; a painter reads one slice.
use super::{LayoutGlyph, LayoutLine, Lines};
use std::collections::HashSet;
use std::mem::{size_of, take};
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, OnceLock};

/// Positioned glyphs in visual order. A field width shares its hard lines.
#[derive(Clone, Debug, Default)]
pub struct Glyphs {
    flat: Vec<LayoutGlyph>,
    field: Option<Box<FieldLines>>,
}

impl From<Vec<LayoutGlyph>> for Glyphs {
    fn from(flat: Vec<LayoutGlyph>) -> Self {
        Self { flat, field: None }
    }
}

impl Glyphs {
    pub fn len(&self) -> usize {
        self.field.as_ref().map_or(self.flat.len(), |f| f.len)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(super) fn line(&self, line: &LayoutLine) -> &[LayoutGlyph] {
        let (a, b) = (line.glyphs.0 as usize, line.glyphs.1 as usize);
        self.slice(line.hard as usize, a, b)
    }

    fn slice(&self, hard: usize, a: usize, b: usize) -> &[LayoutGlyph] {
        if a == b {
            return &[];
        }
        if let Some(field) = &self.field {
            let (start, block) = field.block(hard, a);
            &block.glyphs[a - start..b - start]
        } else {
            &self.flat[a..b]
        }
    }

    pub(super) fn bytes(&self) -> usize {
        self.flat.capacity() * size_of::<LayoutGlyph>()
            + self.field.as_ref().map_or(0, |f| {
                f.bytes
                    + f.blocks.capacity() * size_of::<(usize, Arc<Block>)>()
                    + size_of::<FieldLines>()
                    + f.flat
                        .get()
                        .map_or(0, |g| g.capacity() * size_of::<LayoutGlyph>())
            })
    }

    pub(in crate::text) fn previously_counted_bytes(
        &self,
        seen: &mut HashSet<*const Block>,
    ) -> usize {
        self.field.as_ref().map_or(0, |f| {
            f.blocks
                .iter()
                .filter(|(_, b)| Arc::strong_count(b) > 1 && !seen.insert(Arc::as_ptr(b)))
                .map(|(_, b)| b.bytes())
                .sum()
        })
    }
}

// Callers asking for the whole contiguous vector get it once. Paint, hit
// testing and field layout read the slices directly.
impl Deref for Glyphs {
    type Target = Vec<LayoutGlyph>;
    fn deref(&self) -> &Self::Target {
        match &self.field {
            Some(field) => field.flat.get_or_init(|| field.contiguous()),
            None => &self.flat,
        }
    }
}

impl DerefMut for Glyphs {
    fn deref_mut(&mut self) -> &mut Self::Target {
        if let Some(mut field) = self.field.take() {
            self.flat = field.flat.take().unwrap_or_else(|| field.contiguous());
        }
        &mut self.flat
    }
}

impl<'a> IntoIterator for &'a Glyphs {
    type Item = &'a LayoutGlyph;
    type IntoIter = std::slice::Iter<'a, LayoutGlyph>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut Glyphs {
    type Item = &'a mut LayoutGlyph;
    type IntoIter = std::slice::IterMut<'a, LayoutGlyph>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

#[derive(Debug)]
pub(in crate::text) struct Block {
    glyphs: Vec<LayoutGlyph>,
    faces: Vec<u16>,
}

impl Block {
    fn new(glyphs: Vec<LayoutGlyph>) -> Self {
        let mut faces = Vec::new();
        let mut last = None;
        for glyph in &glyphs {
            if last != Some(glyph.face) && !faces.contains(&glyph.face) {
                faces.push(glyph.face);
            }
            last = Some(glyph.face);
        }
        Self { glyphs, faces }
    }

    fn bytes(&self) -> usize {
        size_of::<Self>()
            + self.glyphs.capacity() * size_of::<LayoutGlyph>()
            + self.faces.capacity() * size_of::<u16>()
    }
}

#[derive(Clone, Debug, Default)]
pub(in crate::text) struct FieldLines {
    blocks: Vec<(usize, Arc<Block>)>,
    short: Vec<LayoutGlyph>,
    len: usize,
    bytes: usize,
    flat: OnceLock<Vec<LayoutGlyph>>,
}

// Copying a handful of glyphs costs less space than a separate shared block.
const SHORT_LINE: usize = 8;

impl FieldLines {
    fn block(&self, hard: usize, at: usize) -> &(usize, Arc<Block>) {
        if let Some(entry) = self
            .blocks
            .get(hard)
            .filter(|(start, block)| *start <= at && at - start < block.glyphs.len())
        {
            return entry;
        }
        &self.blocks[self.blocks.partition_point(|(start, _)| *start <= at) - 1]
    }

    fn flush_short(&mut self) {
        if !self.short.is_empty() {
            let start = self.len - self.short.len();
            self.short.shrink_to_fit();
            let block = Arc::new(Block::new(take(&mut self.short)));
            self.bytes += block.bytes();
            self.blocks.push((start, block));
        }
    }

    fn contiguous(&self) -> Vec<LayoutGlyph> {
        let mut all = Vec::with_capacity(self.len);
        for (_, block) in &self.blocks {
            all.extend_from_slice(&block.glyphs);
        }
        all
    }

    fn push(&mut self, block: Arc<Block>) {
        self.flush_short();
        let start = self.len;
        self.len += block.glyphs.len();
        self.bytes += block.bytes();
        self.blocks.push((start, block));
    }

    pub(in crate::text) fn extracted(&mut self, out: &mut Lines, first_visual: usize) {
        for line in &mut out.lines[first_visual..] {
            line.glyphs.0 += self.len as u32;
            line.glyphs.1 += self.len as u32;
        }
        if out.glyphs.flat.len() <= SHORT_LINE {
            self.len += out.glyphs.flat.len();
            self.short.append(&mut out.glyphs.flat);
        } else {
            self.push(Arc::new(Block::new(take(&mut out.glyphs.flat))));
        }
    }

    pub(in crate::text) fn reuse(
        &mut self,
        out: &mut Lines,
        old: &Lines,
        from: usize,
        to: usize,
        cursor: &mut usize,
        faces: &mut Vec<Option<u16>>,
    ) {
        if old
            .lines
            .get(*cursor)
            .is_none_or(|l| l.hard as usize != from)
        {
            *cursor = old.lines.partition_point(|l| (l.hard as usize) < from);
        }
        let first = *cursor;
        while old
            .lines
            .get(*cursor)
            .is_some_and(|l| l.hard as usize == from)
        {
            *cursor += 1;
        }
        let lines = &old.lines[first..*cursor];
        let start = lines[0].glyphs.0;
        for line in lines {
            let (a, b) = line.glyphs;
            out.lines.push(LayoutLine {
                hard: to as u32,
                glyphs: (self.len as u32 + (a - start), self.len as u32 + (b - start)),
                ..*line
            });
        }
        if let Some(metrics) = old.field_metrics.get(first..*cursor) {
            out.field_metrics
                .resize(out.lines.len() - metrics.len(), None);
            out.field_metrics.extend_from_slice(metrics);
        }
        let (a, b) = (start as usize, lines.last().unwrap().glyphs.1 as usize);
        faces.resize(old.faces.len(), None);
        let mut face = |id: u16| {
            let i = id as usize;
            *faces[i].get_or_insert_with(|| out.face(old.faces[i].clone()))
        };
        if b - a <= SHORT_LINE {
            self.short
                .extend(old.glyphs.slice(from, a, b).iter().map(|g| LayoutGlyph {
                    face: face(g.face),
                    ..*g
                }));
            self.len += b - a;
            return;
        }
        let block = match &old.glyphs.field {
            Some(field) => field.block(from, a).1.clone(),
            None => Arc::new(Block::new(old.glyphs.flat[a..b].to_vec())),
        };
        let mut remap = false;
        for &id in &block.faces {
            remap |= face(id) != id;
        }
        if remap {
            let mut glyphs = block.glyphs.clone();
            for g in &mut glyphs {
                g.face = faces[g.face as usize].unwrap();
            }
            self.push(Arc::new(Block::new(glyphs)));
        } else {
            self.push(block);
        }
    }

    pub(in crate::text) fn finish(mut self) -> Glyphs {
        if self.blocks.is_empty() {
            self.short.shrink_to_fit();
            return self.short.into();
        }
        self.flush_short();
        Glyphs {
            flat: Vec::new(),
            field: Some(Box::new(self)),
        }
    }
}
