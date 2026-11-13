//! When a window's canvas renders: a late drawable may finish only the
//! display frame this canvas still owes, and a canvas at rest is not
//! rendered again because the page repainted for something else.
#[derive(Default)]
pub(super) struct Retry {
    pub frame: Option<(f32, f32, f32, f64)>,
    drawn: bool,
}

impl Retry {
    pub fn begin(&mut self) {
        self.frame = None;
        self.drawn = false;
    }

    pub fn recorded(&mut self, frame: (f32, f32, f32, f64), starved: bool) {
        self.frame = Some(frame);
        self.drawn |= !starved;
    }

    pub fn eligible(&self, buffered: bool, starved: bool, landed: bool) -> bool {
        self.frame.is_some() && !self.drawn && !buffered && starved && landed
    }
}

/// The canvas and the size and scale (points, points, pixels per point) of
/// its last render, when that render was complete and wanted no other frame.
/// It renders again only when it is dirty (`gpu_dirty`: an input, a bind, an
/// asset, a new window), when another canvas replaced it, or when
/// its box or scale changed, as LLP 1009 D4 has it and iOS's `GpuIOS.swift`
/// does (`e.wants || dirty`). A lost device also makes it due (`gpu_dirty` is
/// then 0): on Android the module drops the device and tells the surfaces
/// (`Surface::device_lost`) only from a render, which then answers 3.
#[derive(Default)]
pub(super) struct Rest(Option<(u32, f32, f32, f32)>);

impl Rest {
    pub fn due(
        &self,
        canvas: u32,
        (w, h, scale): (f32, f32, f32),
        dirty: bool,
        lost: bool,
    ) -> bool {
        dirty || lost || self.0 != Some((canvas, w, h, scale))
    }

    /// `result` is `gpu_render`'s: 0 complete and wanting no other frame; 1
    /// wanting one, or incomplete (retried); 2 failed; 3 no device.
    pub fn rendered(&mut self, canvas: u32, (w, h, scale): (f32, f32, f32), result: u32) {
        self.0 = (result == 0).then_some((canvas, w, h, scale));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_canvas_at_rest_renders_again_only_for_something_new() {
        let size = (100., 200., 2.);
        let mut rest = Rest::default();
        assert!(rest.due(7, size, false, false), "its first frame");
        rest.rendered(7, size, 0);
        assert!(
            !rest.due(7, size, false, false),
            "the page repainted for something else"
        );
        assert!(rest.due(7, size, true, false), "dirty");
        assert!(
            rest.due(7, (100., 220., 2.), false, false),
            "its box changed"
        );
        assert!(
            rest.due(7, (100., 200., 3.), false, false),
            "its scale changed"
        );
        assert!(
            rest.due(8, size, false, false),
            "another canvas on the view"
        );
        // A lost device: the render that drops it answers 3, and the canvas
        // is drawn again once a device is back.
        assert!(rest.due(7, size, false, true), "its device was lost");
        rest.rendered(7, size, 3);
        assert!(rest.due(7, size, false, false), "after the loss");
        for result in [1, 2, 3] {
            rest.rendered(7, size, 0);
            rest.rendered(7, size, result);
            assert!(rest.due(7, size, false, false), "after result {result}");
        }
    }

    #[test]
    fn staggered_windows_finish_once_and_leave_buffered_siblings_alone() {
        let frame = (100., 200., 2., 10.);
        let mut a = Retry::default();
        let mut b = Retry::default();
        let mut buffered = Retry::default();
        for canvas in [&mut a, &mut b, &mut buffered] {
            canvas.begin();
            canvas.recorded(frame, true);
        }
        assert!(!a.eligible(false, true, false));
        assert!(b.eligible(false, true, true));
        b.recorded(frame, false);
        assert!(!b.eligible(false, false, true));
        assert!(!b.eligible(false, true, true));
        assert!(a.eligible(false, true, true));
        a.recorded(frame, false);
        assert!(!a.eligible(false, true, true));
        assert!(!buffered.eligible(true, true, true));
        a.begin();
        assert!(!a.eligible(false, true, true));
        a.recorded((100., 200., 2., 20.), true);
        assert!(a.eligible(false, true, true));
        assert_eq!(a.frame.unwrap().3, 20.);
    }
}
