//! CSS Scroll Snap 1's `scroll-snap-type` (§5.1) and `scroll-snap-align`
//! (§6.1) as an author writes them, and what each says along one axis.
//! Each row keeps the form CSSOM serializes, which the web writes as it is.
use crate::generated::{ScrollSnapAlign, ScrollSnapType};

/// Where a snap area rests along one axis: its start, centre or end at the
/// snapport's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapAlign {
    /// `start`.
    Start,
    /// `center`.
    Center,
    /// `end`.
    End,
}

impl ScrollSnapType {
    /// `none`, or an axis (`x`, `y` or `both`) and a strictness, `proximity`
    /// when unsaid. `x proximity` is the row's `x`.
    pub fn from_css(text: &str) -> Option<Self> {
        let lower = text.to_ascii_lowercase();
        match lower.split_ascii_whitespace().collect::<Vec<_>>()[..] {
            ["none"] => Some(Self::None),
            [axis] | [axis, "proximity"] if matches!(axis, "x" | "y" | "both") => {
                Self::from_name(axis)
            }
            [axis, "mandatory"] => Self::from_name(&format!("{axis} mandatory")),
            _ => None,
        }
    }

    /// Whether it snaps along x, and along y.
    pub fn axes(self) -> (bool, bool) {
        match self {
            Self::None => (false, false),
            Self::XMandatory | Self::X => (true, false),
            Self::YMandatory | Self::Y => (false, true),
            Self::BothMandatory | Self::Both => (true, true),
        }
    }

    /// Whether every rest is a snap position (`mandatory`), rather than only
    /// one near a snap position (`proximity`).
    pub fn mandatory(self) -> bool {
        matches!(
            self,
            Self::XMandatory | Self::YMandatory | Self::BothMandatory
        )
    }
}

impl ScrollSnapAlign {
    /// One keyword for both axes, or the block axis's then the inline
    /// axis's. Two equal keywords are the one.
    pub fn from_css(text: &str) -> Option<Self> {
        let lower = text.to_ascii_lowercase();
        match lower.split_ascii_whitespace().collect::<Vec<_>>()[..] {
            [one] => Self::from_name(one),
            [block, inline] if block == inline => Self::from_name(block),
            [block, inline] => Self::from_name(&format!("{block} {inline}")),
            _ => None,
        }
    }

    /// Its alignment along y (the block axis) when `vertical`, else along x
    /// (the inline axis); `None` for `none`.
    pub fn along(self, vertical: bool) -> Option<SnapAlign> {
        let name = self.name();
        let word = match name.split_once(' ') {
            Some((block, inline)) => {
                if vertical {
                    block
                } else {
                    inline
                }
            }
            None => name,
        };
        match word {
            "start" => Some(SnapAlign::Start),
            "center" => Some(SnapAlign::Center),
            "end" => Some(SnapAlign::End),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_strictness_left_unsaid_is_proximity() {
        assert_eq!(ScrollSnapType::from_css("x"), Some(ScrollSnapType::X));
        assert_eq!(
            ScrollSnapType::from_css(" Y  proximity "),
            Some(ScrollSnapType::Y)
        );
        assert_eq!(
            ScrollSnapType::from_css("both mandatory"),
            Some(ScrollSnapType::BothMandatory)
        );
        assert_eq!(ScrollSnapType::from_css("none"), Some(ScrollSnapType::None));
        for refused in ["none mandatory", "block mandatory", "x y", "mandatory", ""] {
            assert_eq!(ScrollSnapType::from_css(refused), None, "{refused}");
        }
        assert_eq!(ScrollSnapType::Both.axes(), (true, true));
        assert!(!ScrollSnapType::Y.mandatory() && ScrollSnapType::YMandatory.mandatory());
    }

    #[test]
    fn two_alignments_are_the_block_axis_then_the_inline() {
        let a = ScrollSnapAlign::from_css("start center").unwrap();
        assert_eq!(a, ScrollSnapAlign::StartCenter);
        assert_eq!(a.along(true), Some(SnapAlign::Start));
        assert_eq!(a.along(false), Some(SnapAlign::Center));
        assert_eq!(
            ScrollSnapAlign::from_css("end end"),
            Some(ScrollSnapAlign::End)
        );
        assert_eq!(ScrollSnapAlign::End.along(true), Some(SnapAlign::End));
        assert_eq!(ScrollSnapAlign::NoneStart.along(true), None);
        assert_eq!(ScrollSnapAlign::from_css("start center end"), None);
    }
}
