//! Encode strokes without the Apple module or a GPU.
#[allow(dead_code)]
#[path = "../src/paint.rs"]
mod paint;
#[allow(dead_code)]
#[path = "../src/replay.rs"]
mod replay;

use exact_canvas::Context2d;
use replay::{Frame, Host, Replayer};
use vello::kurbo::BezPath;
use vello::peniko::ImageData;
use vello::Scene;

struct NoHost;

impl Host for NoHost {
    fn text(&mut self, _: &[f64], _: &[u32], _: bool) -> Option<BezPath> {
        None
    }
    fn image(&mut self, _: &str) -> Option<ImageData> {
        None
    }
}

fn stroke(dash: &[f64], offset: f64, rectangle: bool) -> Scene {
    let ctx = Context2d::new();
    ctx.set_line_width(4.0);
    ctx.set_line_dash(dash).unwrap();
    ctx.set_line_dash_offset(offset);
    if rectangle {
        ctx.stroke_rect(10.0, 10.0, 180.0, 80.0);
    } else {
        ctx.begin_path();
        ctx.move_to(10.0, 50.0);
        ctx.line_to(190.0, 50.0);
        ctx.stroke();
    }
    let mut replayer = Replayer::new(200, 100, 1.0);
    let mut frame = Frame {
        scene: Scene::new(),
        keep: true,
        shadows: Vec::new(),
        masks: Vec::new(),
    };
    replayer.begin(&mut frame.scene);
    for list in ctx.take_lists() {
        replayer.apply(&list, &mut frame, &mut NoHost).unwrap();
    }
    replayer.end(&mut frame.scene);
    frame.scene
}

#[test]
fn zero_dash_lists_stroke_solid_paths_and_rectangles() {
    for rectangle in [false, true] {
        let solid = stroke(&[], 0.0, rectangle);
        let expected = solid.encoding();
        assert_eq!(expected.n_paths, 1);
        assert!(!expected.path_data.is_empty());
        for dash in [&[0.0][..], &[0.0, 0.0], &[0.0, -0.0, 0.0]] {
            for offset in [0.0, 3.0, -3.0] {
                let scene = stroke(dash, offset, rectangle);
                let actual = scene.encoding();
                assert_eq!(
                    actual.n_paths, expected.n_paths,
                    "rectangle={rectangle}, dash={dash:?}, offset={offset}"
                );
                assert!(actual.path_tags == expected.path_tags);
                assert_eq!(actual.path_data, expected.path_data);
            }
        }
        let dashed = stroke(&[4.0, 4.0], 0.0, rectangle);
        assert!(dashed.encoding().n_paths > 0);
        assert_ne!(dashed.encoding().path_data, expected.path_data);
    }
}
