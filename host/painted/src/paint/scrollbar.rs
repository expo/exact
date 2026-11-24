//! Desktop scroll thumbs show the physical position within the painted content.
use super::*;

pub(crate) const WIDTH: f32 = 12.;

#[derive(Clone, Copy)]
pub(crate) struct Bar {
    pub id: ViewId,
    pub key: exact_kernel::NodeKey,
    pub owner: usize,
    /// The first box painted above the scrollbar.
    pub above: usize,
    pub vertical: bool,
    pub port: Rect4,
    pub track: Rect4,
    pub thumb: Rect4,
    pub max: f32,
    /// Physical offset from the left or top edge.
    pub offset: f32,
    pub from_right: bool,
    inverse: Transform,
}

impl Bar {
    fn local(&self, x: f32, y: f32) -> (f32, f32) {
        let mut point = tiny_skia::Point::from_xy(x, y);
        self.inverse.map_point(&mut point);
        (point.x, point.y)
    }
    pub fn coordinate(&self, x: f32, y: f32) -> f32 {
        let (x, y) = self.local(x, y);
        if self.vertical {
            y
        } else {
            x
        }
    }
    pub fn start(&self, r: Rect4) -> f32 {
        if self.vertical {
            r.1
        } else {
            r.0
        }
    }
    pub fn length(&self, r: Rect4) -> f32 {
        if self.vertical {
            r.3
        } else {
            r.2
        }
    }
    pub fn destination(&self, point: f32, grab: f32) -> f32 {
        let travel = self.length(self.track) - self.length(self.thumb);
        if travel <= 0. {
            return 0.;
        }
        ((point - grab - self.start(self.track)) / travel).clamp(0., 1.) * self.max
    }

    pub fn scroll_offset(&self, physical: f32) -> f32 {
        if self.from_right {
            self.max - physical
        } else {
            physical
        }
    }
}

pub(crate) fn contains(bar: &Bar, r: Rect4, x: f32, y: f32) -> bool {
    let (x, y) = bar.local(x, y);
    x >= r.0 && y >= r.1 && x < r.0 + r.2 && y < r.1 + r.3
}

#[derive(Default)]
pub(crate) struct State {
    pub enabled: bool,
    pub auto_hide: bool,
    /// Hovered ports by owner and axis, with the one whose track takes the pointer.
    pub hovered: Vec<(exact_kernel::NodeKey, bool, bool)>,
    pub drag: Option<(exact_kernel::NodeKey, bool, f32)>,
    pub pressed: bool,
    pub bars: Vec<Bar>,
    pub limits: BTreeMap<ViewId, f32>,
}

/// How a thumb paints.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Look {
    expanded: bool,
    /// Hovered or dragged, which a high-contrast palette highlights.
    lit: bool,
}

impl State {
    /// `None` while automatic hiding hides the thumb.
    pub fn look(&self, bar: &Bar) -> Option<Look> {
        let dragged = self
            .drag
            .is_some_and(|d| (d.0, d.1) == (bar.key, bar.vertical));
        let port = self
            .hovered
            .iter()
            .find(|&&(key, vertical, _)| (key, vertical) == (bar.key, bar.vertical));
        let hovered = port.is_some_and(|&(_, _, track)| track);
        let visible = !self.auto_hide || dragged || port.is_some();
        visible.then_some(Look {
            expanded: hovered || dragged || !self.auto_hide,
            lit: hovered || dragged,
        })
    }

    /// The policy and every thumb's look, which a partial repaint must keep.
    pub fn ink(&self) -> (bool, bool, Vec<Option<Look>>) {
        let looks = self.bars.iter().map(|b| self.look(b)).collect();
        (self.enabled, self.auto_hide, looks)
    }
}

impl Painter {
    pub(super) fn paint_scrollbars(
        &mut self,
        walk: &Walk<'_, '_>,
        node: &NodeRef<'_>,
        owner: usize,
        axes: (Overflow, Overflow),
    ) {
        let visible = |axis| matches!(axis, Overflow::Auto | Overflow::Scroll);
        if !self.scrollbars.enabled
            || self.flatten.is_some()
            || walk.reveal.is_some()
            || !walk.boxes[owner].pointer_hit
            || node.style.scrollbar_width == exact_kernel::ScrollbarWidth::None
            || !(visible(axes.0) || visible(axes.1))
        {
            return;
        }
        let mut bounds = self.scroll_bounds(
            walk.scene.kernel,
            None,
            node,
            self.scrollbars.limits.get(&node.id).copied(),
            walk.scene.presented,
            walk.scene.scaled_presented,
        );
        let field = walk.fields.get(&node.id);
        if let Some(field) = field {
            bounds.max = field.scroll_max;
        }
        self.place_scrollbars(
            walk,
            owner,
            node.key,
            bounds,
            field.is_some_and(|f| f.scrolls_from_right()),
        );
    }

    pub(super) fn place_scrollbars(
        &mut self,
        walk: &Walk<'_, '_>,
        owner: usize,
        key: exact_kernel::NodeKey,
        bounds: ScrollBounds,
        rtl: bool,
    ) {
        let axes = [bounds.axes.0, bounds.axes.1];
        let max = [bounds.max.0, bounds.max.1];
        let bar = |i: usize| matches!(axes[i], Overflow::Auto | Overflow::Scroll) && max[i] > 0.;
        if !(bar(0) || bar(1)) {
            return;
        }
        // A row with scroll travel needs to place its track under the current
        // ancestor clip and paint the thumb's current look.
        self.row_refuse();
        let b = &walk.boxes[owner];
        let Some((ts, rect)) = b.affine else {
            return;
        };
        let to_page = self.kept.to_page;
        let hit_ts = b
            .hit
            .as_ref()
            .and_then(|h| h.affine)
            .map_or(ts, |(ts, _)| ts);
        let Some(inverse) = to_page.pre_concat(hit_ts).invert() else {
            return;
        };
        let screen = to_page.pre_concat(ts);
        let viewport = (0., 0., self.viewport.0, self.viewport.1);
        let clip = self
            .row_clip(b.clip.map(|c| bbox(to_page, c)))
            .unwrap_or(viewport);
        let offset = b.scroll.unwrap_or_default();
        let port = self.split_port(b.id, rect);
        for vertical in [false, true] {
            let i = usize::from(vertical);
            if !bar(i) {
                continue;
            }
            let track = if vertical {
                (port.0 + port.2 - WIDTH, port.1 + 2., WIDTH, port.3 - 4.)
            } else {
                (
                    port.0 + 2.,
                    port.1 + port.3 - WIDTH,
                    port.2 - 4. - if bar(1) { WIDTH } else { 0. },
                    WIDTH,
                )
            };
            // Hide the bar when its track does not fit through an ancestor's clip.
            let (x, y, w, h) = bbox(screen, track);
            if x < clip.0
                || y < clip.1
                || x + w > clip.0 + clip.2 + 0.1
                || y + h > clip.1 + clip.3 + 0.1
            {
                continue;
            }
            let visible = if vertical { port.3 } else { port.2 };
            let length = if vertical { track.3 } else { track.2 };
            if length <= 0. || visible <= 0. {
                continue;
            }
            let size = (length * visible / (visible + max[i])).max(24.).min(length);
            let offset = if vertical { offset.1 } else { offset.0 };
            let from_right = !vertical && rtl;
            let offset = if from_right { max[i] - offset } else { offset };
            let start = offset.clamp(0., max[i]) / max[i] * (length - size);
            let thumb = if vertical {
                (track.0, track.1 + start, WIDTH, size)
            } else {
                (track.0 + start, track.1, size, WIDTH)
            };
            let bar = Bar {
                id: b.id,
                key,
                owner,
                above: walk.boxes.len(),
                vertical,
                port,
                track,
                thumb,
                max: max[i],
                offset,
                from_right,
                inverse,
            };
            self.paint_thumb(&bar, ts);
            self.scrollbars.bars.push(bar);
        }
    }

    fn paint_thumb(&mut self, bar: &Bar, ts: Transform) {
        let Some(Look { expanded, lit }) = self.scrollbars.look(bar) else {
            return;
        };
        let width = if expanded { 6. } else { 2. };
        let (x, y, w, h) = bar.thumb;
        let rect = if bar.vertical {
            (x + (w - width) / 2., y, width, h)
        } else {
            (x, y + (h - width) / 2., w, width)
        };
        let ink = if let Some(palette) = super::look::contrast() {
            if lit {
                palette.highlight
            } else {
                palette.button_text
            }
        } else if self.dark {
            [255, 255, 255, 139]
        } else {
            [0, 0, 0, 114]
        };
        self.backend
            .fill(&Shape::new(rect, [width / 2.; 4]), ink, ts);
    }
}
