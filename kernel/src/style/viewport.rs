//! CSS viewport lengths. Native surfaces have no retractable browser chrome.
use super::{Dimension, Env};

/// The viewport dimension a CSS length uses (CSS Values 4 §6.1.2), or the
/// container's (CSS Conditional Rules 5, "Container Relative Lengths").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ViewportUnit {
    /// Large viewport width (`vw`).
    Vw,
    /// Large viewport height (`vh`).
    Vh,
    /// The smaller large viewport dimension.
    Vmin,
    /// The larger large viewport dimension.
    Vmax,
    /// Small viewport width.
    Svw,
    /// Small viewport height.
    Svh,
    /// Large viewport width.
    Lvw,
    /// Large viewport height.
    Lvh,
    /// Dynamic viewport width.
    Dvw,
    /// Dynamic viewport height.
    Dvh,
    /// The query container's width (`cqw`), its content root's for an owner
    /// (LLP 1123 D16).
    Cqw,
    /// The query container's height (`cqh`).
    Cqh,
    /// The query container's inline size (`cqi`), its width in horizontal
    /// writing.
    Cqi,
    /// The query container's block size (`cqb`), its height in horizontal
    /// writing.
    Cqb,
    /// The smaller of `cqi` and `cqb`.
    Cqmin,
    /// The larger of `cqi` and `cqb`.
    Cqmax,
}

impl Dimension {
    /// Whether it reads a container unit (LLP 1123 D16), alone or in a
    /// calculation, which resolves against the box's query containers.
    pub fn reads_container(&self) -> bool {
        match self {
            Dimension::Viewport(unit, _) => unit.is_container(),
            Dimension::Compare(c) => c.reads_container(),
            _ => false,
        }
    }
}

impl ViewportUnit {
    /// Every supported unit, in wire order: dimension kinds 14–23 for the
    /// viewport's, 30–35 for the container's ([`Self::kind`]).
    pub const ALL: [Self; 16] = [
        Self::Vw,
        Self::Vh,
        Self::Vmin,
        Self::Vmax,
        Self::Svw,
        Self::Svh,
        Self::Lvw,
        Self::Lvh,
        Self::Dvw,
        Self::Dvh,
        Self::Cqw,
        Self::Cqh,
        Self::Cqi,
        Self::Cqb,
        Self::Cqmin,
        Self::Cqmax,
    ];

    /// The CSS suffix.
    pub fn name(self) -> &'static str {
        [
            "vw", "vh", "vmin", "vmax", "svw", "svh", "lvw", "lvh", "dvw", "dvh", "cqw", "cqh",
            "cqi", "cqb", "cqmin", "cqmax",
        ][self as usize]
    }

    /// Whether it is a container unit, which a box resolves against its own
    /// query container rather than the viewport.
    pub fn is_container(self) -> bool {
        self as u8 >= Self::Cqw as u8
    }

    /// Its dimension kind on the wire: 14–23 for a viewport unit, 30–35 for
    /// a container unit, past the size keywords.
    pub fn kind(self) -> u8 {
        match self.is_container() {
            true => 30 + (self as u8 - Self::Cqw as u8),
            false => 14 + self as u8,
        }
    }

    /// The unit a dimension kind names, if any.
    pub fn from_kind(kind: u8) -> Option<Self> {
        let index = match kind {
            14..=23 => kind - 14,
            30..=35 => kind - 30 + Self::Cqw as u8,
            _ => return None,
        };
        Self::ALL.get(usize::from(index)).copied()
    }

    /// The viewport's length in points. Native windows have no browser chrome.
    pub fn basis(self, env: &Env) -> f32 {
        // LLP 1075.003 §9.11: the window's, when the host lays the page out
        // in a sheet's smaller viewport.
        let (w, h) = env
            .screen
            .unwrap_or((env.viewport_width, env.viewport_height));
        // A box's query container on each axis, the small viewport where it
        // has none (LLP 1123 D16)
        let (cw, ch) = env.container.unwrap_or((w, h));
        match self {
            Self::Vw | Self::Svw | Self::Lvw | Self::Dvw => w,
            Self::Vh | Self::Svh | Self::Lvh | Self::Dvh => h,
            Self::Vmin => w.min(h),
            Self::Vmax => w.max(h),
            Self::Cqw | Self::Cqi => cw,
            Self::Cqh | Self::Cqb => ch,
            Self::Cqmin => cw.min(ch),
            Self::Cqmax => cw.max(ch),
        }
    }
}

pub(super) fn parse(text: &str) -> Option<Dimension> {
    let text = text
        .trim_matches(['\t', '\n', '\u{c}', '\r', ' '])
        .to_ascii_lowercase();
    ViewportUnit::ALL.into_iter().find_map(|unit| {
        text.strip_suffix(unit.name())
            .and_then(|n| super::parse_pixel_length(&format!("{n}px")))
            .filter(|n| n.is_finite())
            .map(|n| Dimension::Viewport(unit, n))
    })
}
