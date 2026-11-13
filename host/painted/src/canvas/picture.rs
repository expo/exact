//! What a reader copies a picture from: a decoded image, or a canvas's
//! rasterized drawing (Canvas 2D, LLP 1056), both premultiplied RGBA.
use super::{pending, Recorder, Shape, ANIMATED, CLIP_RRECT, IMAGE, IMAGE_DEF, RESTORE};
use crate::image::Bitmap;
use crate::paint::Rect4;
use std::sync::{Arc, Weak};
use tiny_skia::{Pixmap, Transform};

/// A picture the reader fetches by id ([`super::CanvasHost::image`]).
#[derive(Clone)]
pub enum Picture {
    /// A decoded image.
    Bitmap(Arc<Bitmap>),
    /// A canvas's pixels as last drawn.
    Canvas(Arc<Pixmap>),
}

impl Picture {
    /// Pixels across.
    pub fn width(&self) -> u32 {
        match self {
            Picture::Bitmap(b) => b.width(),
            Picture::Canvas(p) => p.width(),
        }
    }

    /// Pixels down.
    pub fn height(&self) -> u32 {
        match self {
            Picture::Bitmap(b) => b.height(),
            Picture::Canvas(p) => p.height(),
        }
    }

    /// The allocation it is known by.
    pub(super) fn key(&self) -> usize {
        match self {
            Picture::Bitmap(b) => Arc::as_ptr(b) as usize,
            Picture::Canvas(p) => Arc::as_ptr(p) as usize,
        }
    }

    pub(super) fn downgrade(&self) -> WeakPicture {
        match self {
            Picture::Bitmap(b) => WeakPicture::Bitmap(Arc::downgrade(b), b.shared()),
            Picture::Canvas(p) => WeakPicture::Canvas(Arc::downgrade(p)),
        }
    }
}

impl AsRef<[u8]> for Picture {
    fn as_ref(&self) -> &[u8] {
        match self {
            Picture::Bitmap(b) => b.as_ref().as_ref(),
            Picture::Canvas(p) => p.data(),
        }
    }
}

/// A picture the recorder announced, held weakly: dropped by the presenter,
/// it is freed on the reader too.
pub(super) enum WeakPicture {
    /// The image, and whether the reader draws from its own pixels.
    Bitmap(Weak<Bitmap>, bool),
    Canvas(Weak<Pixmap>),
}

impl WeakPicture {
    pub(super) fn alive(&self) -> bool {
        match self {
            WeakPicture::Bitmap(w, _) => w.strong_count() > 0,
            WeakPicture::Canvas(w) => w.strong_count() > 0,
        }
    }

    /// Whether the reader holds no copy of its own (it draws from the
    /// picture's GPU buffer): nothing is freed by letting go of it early.
    pub(super) fn shared(&self) -> bool {
        matches!(self, WeakPicture::Bitmap(_, true))
    }

    /// Whether this is `picture`'s allocation, still alive.
    pub(super) fn is(&self, picture: &Picture) -> bool {
        match (self, picture) {
            (WeakPicture::Bitmap(w, _), Picture::Bitmap(b)) => {
                w.upgrade().is_some_and(|live| Arc::ptr_eq(&live, b))
            }
            (WeakPicture::Canvas(w), Picture::Canvas(p)) => {
                w.upgrade().is_some_and(|live| Arc::ptr_eq(&live, p))
            }
            _ => false,
        }
    }
}

/// The next image's colour, including a fully transparent tint.
pub(super) const IMAGE_TINT: u32 = 47;

impl Recorder {
    pub(super) fn image_tint(&mut self, tint: Option<[u8; 4]>) {
        if let Some(color) = tint {
            self.ops.extend([IMAGE_TINT, Self::color(color)]);
        }
    }

    /// A picture mapped onto `dst` under `clips`.
    pub(super) fn picture(
        &mut self,
        picture: &Picture,
        dst: Rect4,
        clips: &[Shape],
        ts: Transform,
        tint: Option<[u8; 4]>,
    ) {
        let id = self.image_id(picture);
        if self.image_rrect(id, dst, clips, ts, tint) {
            return;
        }
        self.need(None);
        self.transform(ts);
        for c in clips {
            self.ops.push(CLIP_RRECT);
            self.rect_radii(c);
        }
        self.image_tint(tint);
        self.ops.extend([IMAGE, id]);
        for v in [dst.0, dst.1, dst.2, dst.3] {
            self.f(v);
        }
        for _ in clips {
            self.ops.push(RESTORE);
        }
    }

    pub(super) fn image_id(&mut self, image: &Picture) -> u32 {
        let key = image.key();
        let now = self.frames;
        match &mut self.row {
            Some(row) => row.images.push(key),
            None => self.again_image(key),
        }
        if let Some((id, weak, used)) = self.images.get_mut(&key) {
            if weak.is(image) {
                *used = now;
                return *id;
            }
        }
        let id = self.next_image;
        self.next_image += 1;
        self.images.insert(key, (id, image.downgrade(), now));
        self.ops
            .extend([IMAGE_DEF, id, image.width(), image.height()]);
        if let Picture::Bitmap(b) = image {
            if let Some(file) = b.animation_file() {
                let path = file.to_string_lossy();
                self.ops.extend([ANIMATED, id]);
                self.string(&path);
            }
        }
        pending().insert(id, image.clone());
        id
    }
}

#[cfg(test)]
mod tests {
    use super::super::{stream, Recorder, FINISHED, IMAGE, IMAGE_DEF, IMAGE_RRECT};
    use super::*;
    use crate::paint::Backend;

    /// A child placed by a `ChildrenMode::Each` surface reaches the reader as
    /// a picture over the box the warp gave it, as a Canvas 2D bitmap does.
    #[test]
    fn a_placed_canvas_child_is_a_picture_in_the_stream() {
        let mut r = Recorder::new();
        r.begin(400., 800., 2.);
        r.surface_image(Arc::new(Pixmap::new(8, 6).unwrap()), (10., 20., 4., 3.));
        r.finish().unwrap();
        let ops = FINISHED.with(|f| f.borrow_mut().take()).unwrap_or_default();
        let (mut defined, mut drawn) = (None, None);
        let mut at = 0;
        while at < ops.len() {
            match ops[at] {
                IMAGE_DEF => defined = Some((ops[at + 1], ops[at + 2], ops[at + 3])),
                // Both draw the picture over the four floats after its id.
                IMAGE | IMAGE_RRECT => {
                    let dst: Vec<f32> = ops[at + 2..at + 6]
                        .iter()
                        .map(|w| f32::from_bits(*w))
                        .collect();
                    drawn = Some((ops[at + 1], dst));
                }
                _ => {}
            }
            at = stream::end(&ops, at).unwrap();
        }
        let (id, w, h) = defined.expect("the picture is defined");
        assert_eq!((w, h), (8, 6));
        assert_eq!(drawn, Some((id, vec![10., 20., 4., 3.])));
    }
}
