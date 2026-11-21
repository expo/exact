use super::*;

/// A numeric control's minimum, maximum and value, or a scroll container's
/// position and travel in logical pixels along each axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NodeRange {
    /// Minimum, maximum and current value. Zeroes for a node without a range.
    Value([f32; 3]),
    /// Scrollable content. An axis with no travel has a zero maximum.
    Scroll {
        /// Current horizontal and vertical offsets.
        offset: (f32, f32),
        /// Maximum horizontal and vertical offsets.
        max: (f32, f32),
    },
}

impl From<[f32; 3]> for NodeRange {
    fn from(value: [f32; 3]) -> Self {
        Self::Value(value)
    }
}

impl Node {
    pub(super) fn reader_range(&self) -> [f32; 3] {
        match self.range {
            NodeRange::Value(value) => value,
            NodeRange::Scroll { offset, max } if self.flags & flag::SIDEWAYS != 0 => {
                [offset.0, max.0, 0.]
            }
            NodeRange::Scroll { offset, max } => [offset.1, max.1, 0.],
        }
    }
}

impl<D: DataSource> Presenter<D> {
    /// Android's forward and backward actions use the primary scroll axis.
    pub(super) fn access_scroll(s: &Scroller) -> (i32, NodeRange) {
        let sideways = s.axes.0 && !s.axes.1;
        let (at, max) = match sideways {
            true => (s.offset.0, s.max.0),
            false => (s.offset.1, s.max.1),
        };
        let mut flags = if sideways { flag::SIDEWAYS } else { 0 };
        if at < max - 0.5 {
            flags |= flag::FORWARD;
        }
        if at > 0.5 {
            flags |= flag::BACKWARD;
        }
        let max = (
            if s.axes.0 { s.max.0 } else { 0. },
            if s.axes.1 { s.max.1 } else { 0. },
        );
        (
            flags,
            NodeRange::Scroll {
                offset: s.offset,
                max,
            },
        )
    }

    /// Move the scroll container in the last accessibility tree to an absolute position.
    pub fn access_scroll_to(&mut self, id: i32, to: (f32, f32)) -> bool {
        if !to.0.is_finite() || !to.1.is_finite() {
            return false;
        }
        let Some(node) = self.access_told().iter().find(|node| node.id == id) else {
            return false;
        };
        let NodeRange::Scroll { offset, max } = node.range else {
            return false;
        };
        if node.flags & flag::DISABLED != 0 {
            return false;
        }
        let (role, view) = (node.role, node.view);
        let to = (to.0.clamp(0., max.0), to.1.clamp(0., max.1));
        if let Some((current, _)) = self
            .surface()
            .filter(|s| s.node == view)
            .and_then(|s| s.scroll)
        {
            return self.scroll_surface_by(to.1 - current);
        }
        if role == role::LIST_BOX {
            self.menu_scroll = to.1;
            self.dirty |= to.1 != offset.1;
            return to.1 != offset.1;
        }
        self.scroll_to(view, to).is_some_and(|at| at != offset)
    }
}
