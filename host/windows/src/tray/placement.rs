//! Screen-pixel geometry for a notification icon's menu.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn size(self) -> (i32, i32) {
        (self.right - self.left, self.bottom - self.top)
    }

    pub fn intersects(self, other: Self) -> bool {
        self.left < other.right
            && other.left < self.right
            && self.top < other.bottom
            && other.top < self.bottom
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Edge {
    Left,
    Top,
    Right,
    #[default]
    Bottom,
}

impl Edge {
    pub fn of_bar(bar: Rect, monitor: Rect) -> Self {
        let (width, height) = bar.size();
        if width > height {
            if (bar.top - monitor.top).abs() < (monitor.bottom - bar.bottom).abs() {
                Self::Top
            } else {
                Self::Bottom
            }
        } else if (bar.left - monitor.left).abs() < (monitor.right - bar.right).abs() {
            Self::Left
        } else {
            Self::Right
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Placement {
    icon: Rect,
    pub area: Rect,
    edge: Edge,
}

impl Placement {
    pub fn new(icon: Rect, monitor: Rect, work: Rect, taskbar: Option<(Edge, Rect)>) -> Self {
        let taskbar = taskbar.filter(|(_, bar)| bar.intersects(monitor));
        // When no app bar is reported, the icon still identifies the edge
        // and the strip that the menu must leave clear (including overflow).
        let edge = taskbar.map_or_else(
            || {
                [
                    (Edge::Left, (icon.left - monitor.left).abs()),
                    (Edge::Top, (icon.top - monitor.top).abs()),
                    (Edge::Right, (monitor.right - icon.right).abs()),
                    (Edge::Bottom, (monitor.bottom - icon.bottom).abs()),
                ]
                .into_iter()
                .min_by_key(|(_, distance)| *distance)
                .unwrap()
                .0
            },
            |(edge, _)| edge,
        );
        let bar = taskbar.map_or(icon, |(_, bar)| bar);
        let mut area = work;
        match edge {
            Edge::Left => area.left = area.left.max(bar.right).max(icon.right).min(area.right),
            Edge::Top => area.top = area.top.max(bar.bottom).max(icon.bottom).min(area.bottom),
            Edge::Right => area.right = area.right.min(bar.left).min(icon.left).max(area.left),
            Edge::Bottom => area.bottom = area.bottom.min(bar.top).min(icon.top).max(area.top),
        }
        Self { icon, area, edge }
    }

    pub fn position(self, size: (i32, i32), right_align: bool) -> (i32, i32) {
        let (width, height) = size;
        let x = match self.edge {
            Edge::Left => self.area.left,
            Edge::Right => self.area.right - width,
            _ if right_align => self.icon.right - width,
            _ => self.icon.left,
        };
        let y = match self.edge {
            Edge::Top => self.area.top,
            Edge::Bottom => self.area.bottom - height,
            _ => self.icon.top,
        };
        (
            x.clamp(
                self.area.left,
                (self.area.right - width).max(self.area.left),
            ),
            y.clamp(
                self.area.top,
                (self.area.bottom - height).max(self.area.top),
            ),
        )
    }
}
