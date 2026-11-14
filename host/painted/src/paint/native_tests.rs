use super::*;
use crate::image::Bitmap;
use crate::presenter::{PainterChoice, Presenter};
use crate::text::{Paragraph, RunPaint, TextEngine};
use std::cell::RefCell;
use tiny_skia::Pixmap;

/// A backend that keeps the shape each platform element is shown in.
struct Natives(Rc<RefCell<Vec<Shape>>>);

impl Backend for Natives {
    crate::paints_nothing!();
    fn name(&self) -> &'static str {
        "natives"
    }
    fn begin(&mut self, _: f32, _: f32, _: f32) {}
    fn fill(&mut self, _: &Shape, _: [u8; 4], _: Transform) {}
    fn fill_gradient(&mut self, _: &Shape, _: &GradientPaint, _: Transform) {}
    fn fill_border(&mut self, _: &border::BorderFill, _: Transform) {}
    fn image(
        &mut self,
        _: &Arc<Bitmap>,
        _: Rect4,
        _: &[Shape],
        _: Transform,
        _: Option<[u8; 4]>,
        _: exact_kernel::ImageRendering,
    ) {
    }
    fn native(&mut self, _: ViewId, _: NativeKind, shape: &Shape, _: Transform, _: &str, _: bool) {
        self.0.borrow_mut().push(*shape);
    }
    fn text(
        &mut self,
        _: &mut TextEngine,
        _: &Paragraph,
        _: &[RunPaint],
        _: (f32, f32),
        _: Transform,
    ) {
    }
    fn push_clip(&mut self, _: &Shape, _: Transform) {}
    fn pop_clip(&mut self) {}
    fn push_opacity(&mut self, _: f32) {}
    fn pop_opacity(&mut self) {}
    fn pointer(&mut self, _: f32, _: f32) {}
    fn finish(&mut self) -> Result<Pixmap, String> {
        Pixmap::new(1, 1).ok_or_else(|| "no pixmap".into())
    }
}

/// A bordered, rounded video is shown in its content box on the content
/// edge's curve: each radius less the border beside it (CSS Backgrounds 3
/// §5.3), so a 6-point border under radius 8 leaves radius 2.
#[test]
fn a_bordered_video_is_clipped_on_its_content_edge() {
    let source = concat!(
        "component App\n  view\n    column\n",
        "      video \"assets/t.mp4\" width=60 height=60 box-sizing=\"border-box\" ",
        "border-width=6 border-style=\"solid\" border-color=\"#000000\" border-radius=8\n",
    );
    let plan = contract::compile(source).unwrap();
    let (p, error) = Presenter::boot_with(
        &plan.encode(),
        (),
        (100.0, 100.0),
        1.0,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let kernel = p.host().kernel();
    let scene = Scene {
        kernel,
        roots: &kernel.roots(),
        hidden: &|_| false,
        presented: &|_| Presented::IDENTITY,
        paths: &|_| None,
        scroll: &BTreeMap::new(),
        page: (0.0, 0.0),
        images: &BTreeMap::new(),
        focus: None,
        selection: None,
        caret_upstream: false,
        pointer: None,
        controls: &BTreeMap::new(),
        chosen: &BTreeMap::new(),
        menu: None,
    };
    let shown = Rc::new(RefCell::new(Vec::new()));
    let mut painter = Painter::new(p.text().clone(), 1.0, Box::new(Natives(shown.clone())));
    painter.paint(&scene, (100.0, 100.0)).unwrap();
    let shown = shown.borrow();
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert_eq!(shown[0].rect, (6.0, 6.0, 48.0, 48.0));
    assert_eq!(shown[0].radii, [(2.0, 2.0); 4]);
}

/// A radius that reads the safe area where the box is (LLP 1123 D9) is its
/// route's, not the document's 0.
#[test]
fn a_radius_reads_its_routes_inset() {
    let source = concat!(
        "component App\n  view\n    column navigationKey=\"home\" testId=\"route\"\n",
        "      video \"assets/t.mp4\" width=60 height=60 ",
        "border-radius=\"var(--exact-safe-area-inset-top)\"\n",
    );
    let plan = contract::compile(source).unwrap();
    let (mut p, error) = Presenter::boot_with(
        &plan.encode(),
        (),
        (100.0, 100.0),
        1.0,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let kernel = p.host.runner_mut().kernel_mut();
    let route = kernel.find_by_test_id("route")[0];
    let route = kernel.node_by_key(route).unwrap().id;
    let local = Some([12.0, 0.0, 0.0, 0.0]);
    kernel.set_local_insets(route, local).unwrap();
    let kernel = p.host().kernel();
    let scene = Scene {
        kernel,
        roots: &kernel.roots(),
        hidden: &|_| false,
        presented: &|_| Presented::IDENTITY,
        paths: &|_| None,
        scroll: &BTreeMap::new(),
        page: (0.0, 0.0),
        images: &BTreeMap::new(),
        focus: None,
        selection: None,
        caret_upstream: false,
        pointer: None,
        controls: &BTreeMap::new(),
        chosen: &BTreeMap::new(),
        menu: None,
    };
    let shown = Rc::new(RefCell::new(Vec::new()));
    let mut painter = Painter::new(p.text().clone(), 1.0, Box::new(Natives(shown.clone())));
    painter.paint(&scene, (100.0, 100.0)).unwrap();
    let shown = shown.borrow();
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert_eq!(shown[0].radii, [(12.0, 12.0); 4]);
}
