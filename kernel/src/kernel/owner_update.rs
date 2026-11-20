//! The owner update (@ref LLP 1123 D2): what an owner's host reports beside
//! its block, one entry per owner, every field from the same native layout
//! pass, applied whole or refused whole. Its encoding is the hosts' shared
//! wire form, which the Apple host's `exact_host_covers` and `exact_view`
//! carry.

use super::provenance::{Contribution, Operation};
use super::{Kernel, Keyboard, OwnerMargins};
use crate::error::{KernelError, LayoutError};
use crate::id::IdMap;
use crate::style::env::{Rect, SegmentGrid};
use crate::style::targets::{ChainEntry, NativeTarget, NativeTargets, Relation};
use crate::wire::codec::{Reader, Writer};
use crate::ViewId;

/// What one owner's host reports beside its block (D2).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OwnerUpdate {
    /// The owner view's raw `safeAreaInsets`, top right bottom left, with
    /// Exact's contributions in them; `None` while unavailable (D3, D9).
    pub band: Option<[f32; 4]>,
    /// Exact's contributions to the band (D11).
    pub contributions: Vec<Contribution>,
    /// The owner's native chain and relational targets (D9).
    pub targets: NativeTargets,
    /// Its margins as UIKit gives them (D8), `None` where its host reports
    /// none.
    pub margins: Option<OwnerMargins>,
    /// Its keyboard in its block's space (D7), `None` while dismissed.
    pub keyboard: Option<Keyboard>,
    /// Its segments (D16), `None` where it lies on one.
    pub segments: Option<SegmentGrid>,
}

impl Kernel {
    /// Apply the owner update of the owner whose root is `view` (LLP 1123
    /// D2), after its block: every field, or none where
    /// [`Kernel::check_owner_update`] refuses one. Returns whether a layout
    /// is owed.
    pub fn update_owner(&mut self, view: ViewId, update: OwnerUpdate) -> Result<bool, KernelError> {
        self.check_owner_update(view, &update)?;
        let mut owed = self.set_owner_targets(view, update.targets)?;
        owed |= self.set_owner_margins(view, update.margins)?;
        owed |= self.set_owner_segments(view, update.segments)?;
        owed |= self.set_owner_band(view, update.band, &update.contributions)?;
        owed |= self.set_owner_keyboard(view, update.keyboard)?;
        Ok(owed)
    }

    /// Why [`Kernel::update_owner`] would refuse `update` for `view`'s
    /// owner, as each field's setter refuses it, changing nothing. A host
    /// checks every update of a call with it before the call changes
    /// anything, since no block and no other owner's update changes what it
    /// checks. The host refuses two for one owner itself, as the first
    /// changes the generations the second is checked against.
    pub fn check_owner_update(
        &self,
        view: ViewId,
        update: &OwnerUpdate,
    ) -> Result<(), KernelError> {
        let slot = self
            .arena
            .slot_of(view)
            .ok_or(LayoutError::UnknownView(view))?;
        super::observe::check_band(update.band.as_ref(), &update.contributions)?;
        let none = IdMap::default();
        let seen = self.arena.observations.targets.seen(slot).unwrap_or(&none);
        if let Some(why) = update.targets.refusal(seen) {
            return Err(LayoutError::NativeTarget(why).into());
        }
        if let Some(m) = &update.margins {
            super::margins::check(m)?;
        }
        if let Some(k) = &update.keyboard {
            super::keyboard::check(k)?;
        }
        match update.segments.as_ref().and_then(SegmentGrid::refusal) {
            Some(why) => Err(LayoutError::OwnerSegments(why).into()),
            None => Ok(()),
        }
    }
}

impl OwnerUpdate {
    /// The update `bytes` encode ([`OwnerUpdate::encode`]); why they are
    /// refused, where they are.
    pub fn decode(bytes: &[u8]) -> Result<OwnerUpdate, &'static str> {
        let mut r = Reader::new(bytes);
        let update = read(&mut r).ok_or(
            "an owner update is truncated, holds a presence byte other than 0 or 1, or names \
             an unknown relation or operation; encode it as `OwnerUpdate::encode` does",
        )?;
        match r.is_empty() {
            true => Ok(update),
            false => Err("an owner update has bytes past its end; send one update per record"),
        }
    }

    /// The update as the hosts' wire form: each field in order, an optional
    /// one after a presence byte, a list after its `u32` count, numbers
    /// little-endian. Insets are four `f32`s, top, right, bottom, left, and
    /// a keyboard's frame and a rect four `f32`s, x, y, width, height. A
    /// contribution is its node (`u32`), its operation's index in
    /// [`Operation::ALL`] (`u8`) and its effect (insets); a target its id
    /// (`u64`), its generation (`u64`) and its optional insets; a chain entry
    /// a target and its kinds' bits (`u8`, `Kind::bit`); a relation its
    /// index in [`Relation::ALL`] (`u8`) and a target; the margins their
    /// root, plain and minimum insets and the readable width (`f32`); a
    /// keyboard its frame and a docked byte, 0 or 1; segments their columns
    /// and rows (`u8` each) and the `cols × rows` rects, row-major.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        optional(&mut w, self.band.as_ref(), |w, b| floats(w, b));
        w.u32(self.contributions.len() as u32);
        for c in &self.contributions {
            w.u32(c.node);
            w.u8(c.operation as u8);
            floats(&mut w, &c.effect);
        }
        w.u32(self.targets.chain.len() as u32);
        for e in &self.targets.chain {
            target_out(&mut w, &e.target);
            w.u8(e.kinds);
        }
        w.u32(self.targets.relations.len() as u32);
        for (relation, target) in &self.targets.relations {
            w.u8(*relation as u8);
            target_out(&mut w, target);
        }
        optional(&mut w, self.margins.as_ref(), |w, m| {
            floats(w, &m.root);
            floats(w, &m.plain);
            floats(w, &m.minimum);
            w.f32(m.readable_width);
        });
        optional(&mut w, self.keyboard.as_ref(), |w, k| {
            floats(w, &k.frame);
            w.u8(u8::from(k.docked));
        });
        optional(&mut w, self.segments.as_ref(), |w, g| {
            w.u8(g.cols);
            w.u8(g.rows);
            for r in &g.rects {
                floats(w, &[r.x, r.y, r.width, r.height]);
            }
        });
        w.into_vec()
    }
}

/// A registered target (D9, "Registration") as the hosts' wire form
/// carries it: its name's length (`u8`) and its bytes, then the target after
/// a presence byte, none where the registration is disposed of. Why `bytes`
/// are refused, where they are.
pub fn decode_registration(bytes: &[u8]) -> Result<(String, Option<NativeTarget>), &'static str> {
    let mut r = Reader::new(bytes);
    let read = |r: &mut Reader<'_>| {
        let len = usize::from(r.u8().ok()?);
        let name = (0..len).map(|_| r.u8().ok()).collect::<Option<Vec<u8>>>()?;
        let name = String::from_utf8(name).ok()?;
        let target = match present(r)? {
            true => Some(target_in(r)?),
            false => None,
        };
        Some((name, target))
    };
    match (read(&mut r), r.is_empty()) {
        (Some(registration), true) => Ok(registration),
        _ => Err("a registered target is truncated, malformed or followed by bytes; encode it as `encode_registration` does"),
    }
}

/// `name`'s registration of `target`, or its disposal (`None`), in the form
/// [`decode_registration`] reads.
pub fn encode_registration(name: &str, target: Option<&NativeTarget>) -> Vec<u8> {
    let mut w = Writer::new();
    w.u8(name.len().min(255) as u8);
    name.bytes().take(255).for_each(|b| w.u8(b));
    optional(&mut w, target, target_out);
    w.into_vec()
}

fn read(r: &mut Reader<'_>) -> Option<OwnerUpdate> {
    let band = optional_four(r)?;
    let mut contributions = Vec::new();
    for _ in 0..r.u32().ok()? {
        let node = r.u32().ok()?;
        let operation = *Operation::ALL.get(usize::from(r.u8().ok()?))?;
        contributions.push(Contribution {
            node,
            operation,
            effect: four(r)?,
        });
    }
    let mut targets = NativeTargets::default();
    for _ in 0..r.u32().ok()? {
        let target = target_in(r)?;
        let kinds = r.u8().ok()?;
        targets.chain.push(ChainEntry { target, kinds });
    }
    for _ in 0..r.u32().ok()? {
        let relation = *Relation::ALL.get(usize::from(r.u8().ok()?))?;
        targets.relations.push((relation, target_in(r)?));
    }
    let margins = match present(r)? {
        true => Some(OwnerMargins {
            root: four(r)?,
            plain: four(r)?,
            minimum: four(r)?,
            readable_width: r.f32().ok()?,
        }),
        false => None,
    };
    let keyboard = match present(r)? {
        true => Some(Keyboard {
            frame: four(r)?,
            docked: present(r)?,
        }),
        false => None,
    };
    let segments = match present(r)? {
        true => {
            let (cols, rows) = (r.u8().ok()?, r.u8().ok()?);
            let mut rects = Vec::new();
            for _ in 0..usize::from(cols) * usize::from(rows) {
                let [x, y, width, height] = four(r)?;
                rects.push(Rect {
                    x,
                    y,
                    width,
                    height,
                });
            }
            Some(SegmentGrid { cols, rows, rects })
        }
        false => None,
    };
    Some(OwnerUpdate {
        band,
        contributions,
        targets,
        margins,
        keyboard,
        segments,
    })
}

/// A presence byte: 0 or 1, else malformed.
fn present(r: &mut Reader<'_>) -> Option<bool> {
    match r.u8().ok()? {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

fn four(r: &mut Reader<'_>) -> Option<[f32; 4]> {
    Some([r.f32().ok()?, r.f32().ok()?, r.f32().ok()?, r.f32().ok()?])
}

/// Four `f32`s after a presence byte: `None` where the bytes are malformed or
/// truncated, `Some(None)` where they are absent.
fn optional_four(r: &mut Reader<'_>) -> Option<Option<[f32; 4]>> {
    Some(match present(r)? {
        true => Some(four(r)?),
        false => None,
    })
}

fn target_in(r: &mut Reader<'_>) -> Option<NativeTarget> {
    let (id, generation) = (r.u64().ok()?, r.u64().ok()?);
    let insets = optional_four(r)?;
    Some(NativeTarget {
        id,
        generation,
        insets,
    })
}

fn optional<T>(w: &mut Writer, value: Option<&T>, each: impl FnOnce(&mut Writer, &T)) {
    w.u8(u8::from(value.is_some()));
    if let Some(v) = value {
        each(w, v);
    }
}

fn floats(w: &mut Writer, values: &[f32]) {
    values.iter().for_each(|&v| w.f32(v));
}

fn target_out(w: &mut Writer, t: &NativeTarget) {
    w.u64(t.id);
    w.u64(t.generation);
    optional(w, t.insets.as_ref(), |w, i| floats(w, i));
}
