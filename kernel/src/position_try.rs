//! Invoker-popover flip fallbacks (LLP 1021 D2). Each comma starts a new
//! attempt against the base style; whitespace combines flips in one attempt,
//! applied in the order written (CSS Anchor Positioning §6.1: `flip-start`
//! does not commute with the other two).

/// Ordered attempts, each its flips in order; an empty list is CSS `none`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PositionTryFallbacks(Vec<Vec<&'static str>>);

const TACTICS: [&str; 3] = ["flip-block", "flip-inline", "flip-start"];

impl PositionTryFallbacks {
    /// Parse the admitted flip-block/flip-inline/flip-start grammar.
    pub fn parse(css: &str) -> Option<Self> {
        if css.trim().eq_ignore_ascii_case("none") {
            return Some(Self::default());
        }
        let mut attempts = Vec::new();
        for attempt in css.split(',') {
            let mut flips = Vec::new();
            for word in attempt.split_ascii_whitespace() {
                let tactic = TACTICS.into_iter().find(|t| word.eq_ignore_ascii_case(t))?;
                if flips.contains(&tactic) {
                    return None;
                }
                flips.push(tactic);
            }
            if flips.is_empty() {
                return None;
            }
            attempts.push(flips);
        }
        Some(Self(attempts))
    }

    /// Canonical CSS, retaining the order of attempts and of their flips.
    pub fn css(&self) -> String {
        if self.0.is_empty() {
            return "none".into();
        }
        self.0
            .iter()
            .map(|flips| flips.join(" "))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// A try tactic's permutation of physical sides, top/right/bottom/left.
/// It also exchanges the sizing axes when a vertical side becomes horizontal
/// (CSS Anchor Positioning §6.1). The host composes the ordered flips.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockTactic([u8; 4]);

impl Default for BlockTactic {
    fn default() -> Self {
        Self([0, 1, 2, 3])
    }
}

impl BlockTactic {
    /// A reflection or rotation of the four sides; other permutations are refused.
    pub fn from_sides(sides: [u8; 4]) -> Option<Self> {
        (sides.iter().all(|side| *side < 4)
            && sides[2] == (sides[0] + 2) % 4
            && sides[3] == (sides[1] + 2) % 4
            && sides[0] % 2 != sides[1] % 2)
            .then_some(Self(sides))
    }

    pub(crate) fn apply(self, style: &mut taffy::style::Style) {
        let m = style.margin;
        let margins = [m.top, m.right, m.bottom, m.left];
        [
            style.margin.top,
            style.margin.right,
            style.margin.bottom,
            style.margin.left,
        ] = self.0.map(|side| margins[usize::from(side)]);
        if self.0[0] % 2 == 1 {
            std::mem::swap(&mut style.size.width, &mut style.size.height);
            std::mem::swap(&mut style.min_size.width, &mut style.min_size.height);
            std::mem::swap(&mut style.max_size.width, &mut style.max_size.height);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordered_attempts_and_combined_flips_round_trip() {
        for (source, expected) in [
            ("none", "none"),
            (
                " FLIP-INLINE , flip-block, flip-inline flip-block ",
                "flip-inline, flip-block, flip-inline flip-block",
            ),
            ("flip-block, flip-block", "flip-block, flip-block"),
            (
                "flip-start, flip-start flip-block,flip-block  Flip-Start",
                "flip-start, flip-start flip-block, flip-block flip-start",
            ),
            (
                "flip-inline flip-start flip-block",
                "flip-inline flip-start flip-block",
            ),
        ] {
            let value = PositionTryFallbacks::parse(source).unwrap();
            assert_eq!(value.css(), expected);
            assert_eq!(PositionTryFallbacks::parse(&value.css()), Some(value));
        }
        for source in [
            "",
            "flip-block,",
            "none, flip-block",
            "flip-block flip-block",
            "flip-start flip-block flip-start",
            "flip-x",
            "--custom",
            "top",
        ] {
            assert_eq!(PositionTryFallbacks::parse(source), None, "{source}");
        }
    }
}
