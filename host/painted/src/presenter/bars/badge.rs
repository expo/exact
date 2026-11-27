//! Constrained badge face for top app bar buttons (LLP 1075.003 §9.6, #1945):
//! lowers the same markup Apple's `BadgeFace` projects (a header `<button>`
//! whose single displayed child is a filled box holding a picture, initial text
//! or symbol) into Android's painted Material 3 top app bar.
use crate::image::Bitmap;
use crate::paint::{border, object_fit, object_fit_of, rgba, Backend, Rect4, Shape};
use crate::presenter::top::look::{directed_paragraph, label_x, symbol, Type};
use crate::text::{Paragraph, RunPaint, TextEngine};
use exact_kernel::{Direction, ImageRendering, Kernel, ObjectFit, StyleId, SymbolSet, ViewId};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;
use tiny_skia::Transform;

/// Fixed badge face diameter inside the 48 dp top-bar icon target (matches
/// Apple's `BadgeFace.size`, fits within the 40 dp ripple and 64 dp bar).
pub const BADGE_SIZE: f32 = 36.0;

/// A decoded badge picture and its CSS placement inputs.
#[derive(Clone)]
pub struct BadgePicture {
    /// Decoded RGBA bitmap for the badge face.
    pub bitmap: Arc<Bitmap>,
    /// Authored CSS `object-fit` mode for placing the bitmap inside the badge box.
    pub fit: ObjectFit,
    /// Authored CSS `object-position` anchor inside the badge box.
    pub position: exact_kernel::object::Position,
    /// Authored CSS `image-rendering` sampling mode.
    pub rendering: ImageRendering,
    /// Resolved CSS environment of the picture node for percentage/length positions.
    pub env: exact_kernel::Env,
}

impl std::fmt::Debug for BadgePicture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BadgePicture")
            .field("ptr", &(Arc::as_ptr(&self.bitmap) as usize))
            .field("natural", &self.bitmap.natural())
            .field("fit", &self.fit)
            .field("rendering", &self.rendering)
            .finish()
    }
}

impl PartialEq for BadgePicture {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.bitmap, &other.bitmap)
            && self.fit == other.fit
            && self.position == other.position
            && self.rendering == other.rendering
            && self.env == other.env
    }
}

/// Authored badge face resolved from a header button's single filled box child.
#[derive(Debug, Clone, PartialEq)]
pub struct Badge {
    /// Square face side in dp (`BADGE_SIZE`).
    pub size: f32,
    /// Authored fill colour resolved in the box's colour scheme.
    pub fill: [u8; 4],
    /// Authored ink colour for fallback text or symbol.
    pub ink: [u8; 4],
    /// Per-corner elliptical radii `(rx, ry)` resolved against `size`.
    pub corners: [(f32, f32); 4],
    /// Decoded picture once loaded; `None` while loading, failed or absent.
    pub picture: Option<BadgePicture>,
    /// Fallback symbol role when no picture is loaded.
    pub symbol: Option<String>,
    /// Fallback initial text when neither picture nor symbol is shown.
    pub text: String,
    /// Fallback text direction.
    pub text_direction: Direction,
    /// Fallback text font size in dp before font-scale clamping.
    pub text_size: f32,
    /// Fallback text font weight.
    pub text_weight: u16,
    /// Authored node id for text runs (the button or text node).
    pub text_node: ViewId,
}

impl Badge {
    /// Read a constrained badge face for header button `button`, if it matches
    /// [`Kernel::header_badge`], using any decoded bitmap already in `bitmaps`.
    pub fn read(
        kernel: &Kernel,
        button: ViewId,
        dark_at: impl Fn(ViewId) -> bool,
        bitmaps: &BTreeMap<ViewId, Arc<Bitmap>>,
    ) -> Option<Self> {
        let found = kernel.header_badge(button)?;
        let box_node = kernel.node(found.box_node)?;
        let box_dark = dark_at(found.box_node);
        let _accent = crate::paint::accent_scope(&box_node);
        let fill = rgba(box_node.style.background_color?.resolve(box_dark));
        if fill[3] == 0 {
            return None;
        }
        let env = kernel.env_of(found.box_node);
        let radii = [
            box_node.style.border_radius_top_left.clone(),
            box_node.style.border_radius_top_right.clone(),
            box_node.style.border_radius_bottom_right.clone(),
            box_node.style.border_radius_bottom_left.clone(),
        ]
        .map(|d| d.resolve(&env));
        let corners = border::resolve_radii(radii, &env, BADGE_SIZE, BADGE_SIZE);

        let ink_ref = found.ink_node.and_then(|id| kernel.node(id));
        let ink_dark = found.ink_node.map_or(box_dark, &dark_at);
        let ink = ink_ref.as_ref().map_or_else(
            || {
                if ink_dark {
                    [255, 255, 255, 255]
                } else {
                    [0, 0, 0, 255]
                }
            },
            |n| {
                let _a = crate::paint::accent_scope(n);
                rgba(n.text_color().resolve(ink_dark))
            },
        );
        let (text_direction, text_size, text_weight) = ink_ref
            .as_ref()
            .filter(|n| n.node_type == exact_kernel::NodeType::Text)
            .map_or((Direction::Ltr, BADGE_SIZE * 0.42, 500), |n| {
                let dir = n.computed_row(StyleId::Direction, |s| s.direction);
                let size = if n.style.written.has(StyleId::FontSize) {
                    n.computed_row(StyleId::FontSize, |s| s.font_size).max(1.0)
                } else {
                    BADGE_SIZE * 0.42
                };
                let weight = if n.style.written.has(StyleId::FontWeight) {
                    n.computed_row(StyleId::FontWeight, |s| s.font_weight)
                } else {
                    500
                };
                (dir, size, weight)
            });

        let picture = found.picture.and_then(|pic_id| {
            let bitmap = bitmaps.get(&pic_id)?.clone();
            let pic_node = kernel.node(pic_id)?;
            Some(BadgePicture {
                bitmap,
                fit: object_fit_of(&pic_node),
                position: pic_node.style.object_position.clone(),
                rendering: pic_node.computed_row(StyleId::ImageRendering, |s| s.image_rendering),
                env: kernel.env_of(pic_id),
            })
        });

        Some(Self {
            size: BADGE_SIZE,
            fill,
            ink,
            corners,
            picture,
            symbol: found.symbol,
            text: found.text,
            text_direction,
            text_size,
            text_weight,
            text_node: found.ink_node.unwrap_or(button),
        })
    }
}

/// Measured top-bar badge item ready to mirror for RTL and paint.
pub(super) struct DrawnBadge {
    shape: Shape,
    fill: [u8; 4],
    ink: [u8; 4],
    enabled: bool,
    picture: Option<(Arc<Bitmap>, Rect4, ImageRendering)>,
    symbol: Option<(String, Rect4)>,
    text: Option<(Rc<Paragraph>, (f32, f32), ViewId)>,
}

impl DrawnBadge {
    pub(super) fn layout(
        engine: &mut TextEngine,
        badge: &Badge,
        rect: Rect4,
        enabled: bool,
        font_scale: f32,
    ) -> Self {
        let shape = Shape::elliptical(rect, badge.corners);
        let picture = badge.picture.as_ref().and_then(|pic| {
            let dst = object_fit(pic.bitmap.natural(), pic.fit, &pic.position, rect, &pic.env)?;
            Some((pic.bitmap.clone(), dst, pic.rendering))
        });
        let symbol = (picture.is_none())
            .then(|| {
                let role = badge.symbol.as_ref()?;
                let s = (badge.size * 0.48).round();
                let r = (
                    rect.0 + (rect.2 - s) / 2.0,
                    rect.1 + (rect.3 - s) / 2.0,
                    s,
                    s,
                );
                Some((role.clone(), r))
            })
            .flatten();
        let text = (picture.is_none() && symbol.is_none() && !badge.text.is_empty()).then(|| {
            // Gently scale fallback initials while keeping them bounded inside the
            // fixed 36 dp avatar circle even at 2x accessibility font scale.
            let size = (badge.text_size * font_scale.clamp(1.0, 1.35)).min(badge.size * 0.62);
            let style = Type(size, size * 1.2, badge.text_weight, 0.0);
            let p = directed_paragraph(engine, &badge.text, style, 1.0, None, badge.text_direction);
            let at = (
                label_x(&p, rect.0 + rect.2 / 2.0, 0.5),
                rect.1 + (rect.3 - p.height) / 2.0,
            );
            (p, at, badge.text_node)
        });
        Self {
            shape,
            fill: badge.fill,
            ink: badge.ink,
            enabled,
            picture,
            symbol,
            text,
        }
    }

    pub(super) fn mirror(&mut self, width: f32) {
        // Move the badge into its RTL bar slot without flipping physical CSS
        // corners or internal object-position alignment.
        let new_x = width - self.shape.rect.0 - self.shape.rect.2;
        let dx = new_x - self.shape.rect.0;
        self.shape.rect.0 = new_x;
        if let Some((_, dst, _)) = &mut self.picture {
            dst.0 += dx;
        }
        if let Some((_, r)) = &mut self.symbol {
            r.0 += dx;
        }
        if let Some((_, at, _)) = &mut self.text {
            at.0 += dx;
        }
    }

    pub(super) fn paint(&self, backend: &mut dyn Backend, engine: &mut TextEngine, ts: Transform) {
        if !self.enabled {
            backend.push_opacity(0.38);
        }
        backend.fill(&self.shape, self.fill, ts);
        if let Some((bitmap, dst, rendering)) = &self.picture {
            backend.image(
                bitmap,
                bitmap.placed(*dst),
                &[self.shape],
                ts,
                None,
                *rendering,
            );
        } else if let Some((role, rect)) = &self.symbol {
            backend.push_clip(&self.shape, ts);
            symbol(
                backend,
                role,
                *rect,
                self.ink,
                SymbolSet::Material,
                400,
                false,
            );
            backend.pop_clip();
        } else if let Some((p, at, source)) = &self.text {
            backend.push_clip(&self.shape, ts);
            let paint = [RunPaint {
                color: self.ink,
                source: *source,
            }];
            backend.text(engine, p, &paint, *at, ts);
            backend.pop_clip();
        }
        if !self.enabled {
            backend.pop_opacity();
        }
    }
}

#[cfg(test)]
#[path = "badge_tests.rs"]
mod tests;
