//! Margins and the layout regions per box (LLP 1123 D8, D10): what a box reads of its margins,
//! its readable content and their variants, by UIKit's rule. The kernel holds no margin value of
//! its own: the host reports each owner's from UIKit (`OwnerMargins`), and a box's margins are
//! its own, or a plain view's, with its safe area added and, where it preserves them, its
//! superview's margins projected into it.
use super::area::{self, Space};
use super::Kernel;
use crate::arena::NodeArena;
use crate::error::{KernelError, LayoutError};
use crate::style::Dimension;
use crate::ViewId;

/// The rows that hold a box's margins and their options, whose change moves its readings.
pub(crate) const ROWS: [crate::StyleId; 8] = [
    crate::StyleId::LayoutMarginTop,
    crate::StyleId::LayoutMarginRight,
    crate::StyleId::LayoutMarginBottom,
    crate::StyleId::LayoutMarginLeft,
    crate::StyleId::LayoutMarginLeading,
    crate::StyleId::LayoutMarginTrailing,
    crate::StyleId::LayoutMarginsPreserved,
    crate::StyleId::LayoutMarginsFromSafeArea,
];

/// What a host reports of an owner's margins, each as UIKit gives it, top right bottom left in
/// the owner's space, and again whenever UIKit changes it (a trait, size class, posture or text
/// size change included).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OwnerMargins {
    /// The controller root view's margins rectangle as insets from the allocation's edges
    /// (`layoutMarginsGuide`): its own margins, raised to the system minimum where it respects
    /// it, with its safe area added, less Exact's own contributions to that safe area, as the
    /// band is (D11).
    pub root: [f32; 4],
    /// A plain view's default margins (a new `UIView`'s `layoutMargins`).
    pub plain: [f32; 4],
    /// The controller's `systemMinimumLayoutMargins`, which a box whose owner reports no
    /// margins reads as unavailable.
    pub minimum: [f32; 4],
    /// The width `readableContentGuide` keeps text to at the owner's text size. One at least
    /// a box's margins' width leaves its guide the margins, as UIKit's is on the iPhone 18 Pro
    /// and the Duo at the Large text size.
    pub readable_width: f32,
}

/// What a box reads of D8's families, in `Family::MARGINS`' order and then the system minimum,
/// each top right bottom left, with the most each can be.
pub(super) struct Regions {
    pub(super) readings: [[f32; 4]; 7],
    pub(super) caps: [[f32; 4]; 7],
}

/// What `slot` reads of its owner's margins, `owner` the owner's report, `band` its space's.
pub(super) fn regions(
    arena: &NodeArena,
    slot: u32,
    space: &Space,
    band: [f32; 4],
    owner: &OwnerMargins,
) -> Regions {
    let safe = safe_area(arena, slot, space, band);
    let (own, own_cap) = own_from_safe(arena, slot, owner, safe);
    let (margins, cap) = preserved(arena, slot, space, band, owner, (own, own_cap));
    // `.readableContent()`: as wide as the box less its own margins from the safe area, centred
    // in the box and kept inside its margins, so the smaller side rises to the mean of the two
    let mean = (own[1] + own[3]) / 2.0;
    let mean_cap = (own_cap[1] + own_cap[3]) / 2.0;
    let readable = [
        margins[0],
        margins[1].max(mean),
        margins[2],
        margins[3].max(mean),
    ];
    let readable_cap = [cap[0], cap[1].max(mean_cap), cap[2], cap[3].max(mean_cap)];
    // `readableContentGuide`: the readable width centred in the margins rectangle
    let width = arena.frame(slot).width;
    let spare = ((width - margins[1] - margins[3]) - owner.readable_width).max(0.0) / 2.0;
    // UIKit keeps the guide's frame at least empty where the margins exceed the box
    // (`design/probes/margins`: `root-large preserve-bottom`, 42 where the margins are 74)
    let height = arena.frame(slot).height;
    let guide = [
        margins[0],
        (margins[1] + spare).min((width - margins[3] - spare).max(0.0)),
        margins[2].min((height - margins[0]).max(0.0)),
        margins[3] + spare,
    ];
    let guide_cap = [cap[0], cap[1] + spare, cap[2], cap[3] + spare];
    // A corner variant is its region's plain value until the host reports the display's corners
    // (LLP 1123 Q3)
    Regions {
        readings: [
            safe.0,
            margins,
            margins,
            readable,
            readable,
            guide,
            owner.minimum,
        ],
        caps: [
            safe.1,
            cap,
            cap,
            readable_cap,
            readable_cap,
            guide_cap,
            owner.minimum,
        ],
    }
}

/// `slot`'s margins and the most each can be: its own from the safe area, and where it
/// preserves its superview's, each edge raised to that superview's margin boundary projected
/// into it. An owner's root's superview is its content root, whose margins are the controller
/// root view's.
fn margins(
    arena: &NodeArena,
    slot: u32,
    space: &Space,
    band: [f32; 4],
    owner: &OwnerMargins,
) -> ([f32; 4], [f32; 4]) {
    let own = own_from_safe(arena, slot, owner, safe_area(arena, slot, space, band));
    preserved(arena, slot, space, band, owner, own)
}

/// `own`, `slot`'s own margins from the safe area and their most, raised where it preserves
/// its superview's ([`margins`]).
fn preserved(
    arena: &NodeArena,
    slot: u32,
    space: &Space,
    band: [f32; 4],
    owner: &OwnerMargins,
    (mut rect, mut cap): ([f32; 4], [f32; 4]),
) -> ([f32; 4], [f32; 4]) {
    if !preserves(arena, slot) {
        return (rect, cap);
    }
    let edges = space.edges(arena, slot);
    let boundary = match (space.owner == Some(slot), arena.parent(slot)) {
        (true, _) => Some((owner.root, owner.root, [0.0; 4])),
        (false, Some(parent)) => {
            let (m, c) = margins(arena, parent, space, band, owner);
            Some((m, c, space.edges(arena, parent)))
        }
        (false, None) => None,
    };
    if let Some((above, above_cap, from)) = boundary {
        for e in 0..4 {
            // An edge past its superview's takes the superview's margin there and no more,
            // as UIKit gives it (`design/probes/margins`: the Duo's `preserve-own-24`)
            let d = (edges[e] - from[e]).max(0.0);
            rect[e] = rect[e].max(above[e] - d);
            cap[e] = cap[e].max(above_cap[e]);
        }
    }
    (rect, cap)
}

/// `slot`'s own margins, its safe area added where it insets them from it (UIKit's default),
/// and the most each can be.
fn own_from_safe(
    arena: &NodeArena,
    slot: u32,
    owner: &OwnerMargins,
    (safe, depth): ([f32; 4], [f32; 4]),
) -> ([f32; 4], [f32; 4]) {
    let own = own(arena, slot, owner);
    if arena.style(slot).rare.layout_margins_from_safe_area != 0 {
        return (own, own);
    }
    (
        std::array::from_fn(|e| own[e] + safe[e]),
        std::array::from_fn(|e| own[e] + depth[e]),
    )
}

/// `slot`'s own margins, top right bottom left: what it writes, a leading or trailing margin on
/// the side its `direction` maps it to, and a plain view's default where it writes none.
fn own(arena: &NodeArena, slot: u32, owner: &OwnerMargins) -> [f32; 4] {
    let style = arena.style(slot);
    if !ROWS[..6].iter().any(|&id| style.mask.has(id)) {
        return owner.plain;
    }
    // A length in points needs no environment; another resolves in the box's own
    let env = std::cell::OnceCell::new();
    let length = |d: &Dimension| match d {
        Dimension::Points(v) => Some(v.max(0.0)),
        Dimension::Auto => None,
        d => match d.resolve(env.get_or_init(|| crate::kernel::owner::env_for(arena, slot, style)))
        {
            Dimension::Points(v) => Some(v.max(0.0)),
            _ => None,
        },
    };
    let rare = &style.rare;
    let mut own = [
        length(&rare.layout_margin_top),
        length(&rare.layout_margin_right),
        length(&rare.layout_margin_bottom),
        length(&rare.layout_margin_left),
    ];
    let rtl = arena
        .computed_source(slot, crate::StyleId::Direction)
        .direction
        == crate::Direction::Rtl;
    let (leading, trailing) = if rtl { (1, 3) } else { (3, 1) };
    if let Some(v) = length(&rare.layout_margin_leading) {
        own[leading] = Some(v);
    }
    if let Some(v) = length(&rare.layout_margin_trailing) {
        own[trailing] = Some(v);
    }
    std::array::from_fn(|e| own[e].unwrap_or(owner.plain[e]))
}

/// Whether `slot` preserves its superview's margins, so that its reading moves when a box
/// above it does, with no change to its own frame.
pub(super) fn preserves(arena: &NodeArena, slot: u32) -> bool {
    arena.style(slot).rare.layout_margins_preserved != 0
}

/// `slot`'s safe-area observation and the area's depth on each edge (D9).
fn safe_area(arena: &NodeArena, slot: u32, space: &Space, band: [f32; 4]) -> ([f32; 4], [f32; 4]) {
    let sides = area::area(arena, slot, space, band);
    let edges = space.edges(arena, slot);
    (
        std::array::from_fn(|e| area::depth(sides[e], edges[e])),
        std::array::from_fn(|e| (sides[e][1] - sides[e][0]).max(0.0)),
    )
}

impl Kernel {
    /// Set what the host reports of `view`'s owner's margins (LLP 1123 D8), each as UIKit gives
    /// it, or `None` where it reports none, whose boxes read their safe area in place of each
    /// region (D10). Returns whether a layout is owed: a box in the owner reads a region. A
    /// length that is not finite or is negative is refused, keeping the last.
    pub fn set_owner_margins(
        &mut self,
        view: ViewId,
        margins: Option<OwnerMargins>,
    ) -> Result<bool, KernelError> {
        let slot = self
            .arena
            .slot_of(view)
            .ok_or(LayoutError::UnknownView(view))?;
        if let Some(m) = &margins {
            let lengths = m
                .root
                .iter()
                .chain(&m.plain)
                .chain(&m.minimum)
                .chain([&m.readable_width]);
            if !lengths.into_iter().all(|v| v.is_finite() && *v >= 0.0) {
                return Err(LayoutError::Margins(
                    "a length is not finite or is negative: report each margin, the system \
                     minimum and the readable width as UIKit gives them, in points",
                )
                .into());
            }
        }
        let owners = &mut self.arena.observations.margins;
        if owners.get(&slot) == margins.as_ref() {
            return Ok(false);
        }
        match margins {
            Some(m) => owners.insert(slot, m),
            None => owners.remove(&slot),
        };
        let bits = super::observe::margins_bits();
        let observations = &mut self.arena.observations;
        Ok(observations.forget_readings(&self.arena.readers.observed, bits, |o| o == Some(slot)))
    }
}
