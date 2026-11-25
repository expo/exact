//! A canvas's children as its surface takes them (LLP 1014): where an `Each`
//! surface placed each (D5, `Surface::placement`), read for each paint and on
//! Android after each render, so that children a surface moves while the page
//! is still follow it; and a `Composite` surface's picture of them all (D2,
//! D3), painted apart (`paint/composite.rs`) and uploaded (`gpu_texture`).
use super::*;
use crate::placement::Placement;

impl Surfaces {
    /// Where each `Each` canvas's surface placed its children. `ask`: the
    /// no-device executor computes them first; a rendered canvas has them.
    pub(crate) fn placements<D: DataSource>(
        &self,
        host: &Host<D>,
        ask: bool,
    ) -> BTreeMap<u32, Placement> {
        let mut result = BTreeMap::new();
        for (&view, canvas) in &self.canvases {
            let (Some(node), Some(abi)) =
                (host.kernel().node(view), self.abis.get(&canvas.artifact))
            else {
                continue;
            };
            if unsafe { abi.symbol::<Read>(b"gpu_children_mode")(canvas.id) } != 3 {
                continue;
            }
            let children = node.children();
            for (i, id) in children.iter().enumerate() {
                let Some(child) = host.kernel().node(*id) else {
                    continue;
                };
                let f = child.frame;
                let name = child.props.str(exact_kernel::PropId::TestId).unwrap_or("");
                unsafe {
                    abi.symbol::<unsafe extern "C" fn(
                        u32,
                        u32,
                        *const u8,
                        usize,
                        f32,
                        f32,
                        f32,
                        f32,
                        u32,
                        u32,
                        *const u8,
                        usize,
                    ) -> u32>(b"gpu_child_view")(
                        canvas.id,
                        i as u32,
                        name.as_ptr(),
                        name.len(),
                        f.x - node.frame.x,
                        f.y - node.frame.y,
                        f.width,
                        f.height,
                        0,
                        0,
                        std::ptr::null(),
                        0,
                    );
                }
            }
            unsafe {
                abi.symbol::<unsafe extern "C" fn(u32, u32) -> u32>(b"gpu_children_count")(
                    canvas.id,
                    children.len() as u32,
                );
            }
            // Until this canvas has rendered at this size, ask its no-device executor
            // for placements at the committed viewport and clock.
            let size = (node.frame.width, node.frame.height);
            if ask && (!abi.device_loaded || self.drawn_sizes.get(&view) != Some(&size)) {
                abi.agent(canvas.id,&json!({"op":"state","now":host.now(),"width":node.frame.width,"height":node.frame.height}));
            }
            for (i, id) in children.iter().enumerate() {
                let mut h = [0.; 16];
                let code = unsafe {
                    abi.symbol::<unsafe extern "C" fn(u32, u32, *mut f32, usize) -> u32>(
                        b"gpu_placement",
                    )(canvas.id, i as u32, h.as_mut_ptr(), h.len())
                };
                match code {
                    1 => {
                        let opacity = unsafe {
                            abi.library
                                .get::<unsafe extern "C" fn(u32, u32) -> f32>(
                                    b"gpu_placement_opacity",
                                )
                                .map_or(1., |read| read(canvas.id, i as u32))
                        };
                        let opacity = if opacity.is_finite() {
                            opacity.clamp(0., 1.)
                        } else {
                            1.
                        };
                        result.insert(
                            *id,
                            Placement::Visible {
                                h: h[..9].try_into().unwrap(),
                                depth: h[9],
                                opacity,
                                clip_depth: [
                                    h[10..13].try_into().unwrap(),
                                    h[13..16].try_into().unwrap(),
                                ],
                                canvas: view,
                            },
                        );
                    }
                    2 => {
                        result.insert(*id, Placement::Hidden);
                    }
                    _ => {}
                }
            }
        }
        result
    }

    /// Whether a rendered `Each` canvas's placements or opacity differ from
    /// the last paint's. Android repaints only when the page changes, so it
    /// must also repaint when a surface moves or fades a child in `render`.
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    pub(crate) fn placements_moved<D: DataSource>(
        &self,
        host: &Host<D>,
        painted: &BTreeMap<u32, Placement>,
    ) -> bool {
        moved(painted, &self.placements(host, false))
    }
}

impl Surfaces {
    /// The canvases whose surfaces composite their children into a texture
    /// (`ChildrenMode::Composite`, LLP 1014 D2).
    pub(crate) fn composites(&self) -> std::collections::BTreeSet<u32> {
        self.canvases
            .iter()
            .filter(|(_, c)| {
                self.abis.get(&c.artifact).is_some_and(|abi| {
                    matches!(
                        unsafe { abi.symbol::<Read>(b"gpu_children_mode")(c.id) },
                        1 | 2
                    )
                })
            })
            .map(|(view, _)| *view)
            .collect()
    }

    /// Upload each canvas's picture of its children (`gpu_texture`), when it
    /// is not the one this canvas was last given: an upload marks the canvas
    /// dirty, and the surface renders again with it. What a canvas was given
    /// goes with it, so a canvas that replaces it under the same view (a
    /// reload) is given its first picture whatever it shows.
    pub(crate) fn children_pictures(
        &mut self,
        pictures: Vec<(u32, std::sync::Arc<tiny_skia::Pixmap>)>,
    ) {
        for (view, picture) in pictures {
            let (Some(c), true) = (self.canvases.get(&view), picture.width() > 0) else {
                continue;
            };
            let same = c.uploaded.as_ref().is_some_and(|last| {
                last.width() == picture.width()
                    && last.height() == picture.height()
                    && last.data() == picture.data()
            });
            if same {
                continue;
            }
            let abi = &self.abis[&c.artifact];
            let data = picture.data();
            // SAFETY: `gpu_texture(u32, u32, u32, *const u8, usize) -> u32` as gpu/src/native.rs declares it.
            let texture = unsafe {
                abi.symbol::<unsafe extern "C" fn(u32, u32, u32, *const u8, usize) -> u32>(
                    b"gpu_texture",
                )
            };
            if unsafe {
                texture(
                    c.id,
                    picture.width(),
                    picture.height(),
                    data.as_ptr(),
                    data.len(),
                )
            } != 0
            {
                // A headless module without a device composites nothing, as it renders nothing.
                self.error = abi.error().filter(|e| e != "no device");
                continue;
            }
            if let Some(c) = self.canvases.get_mut(&view) {
                c.uploaded = Some(picture);
            }
        }
    }
}

/// Whether two sets of placements differ in any child, bit for bit.
pub(crate) fn moved(a: &BTreeMap<u32, Placement>, b: &BTreeMap<u32, Placement>) -> bool {
    let bits = |p: &Placement| match *p {
        Placement::Hidden => None,
        Placement::Visible {
            h,
            depth,
            opacity,
            clip_depth,
            canvas,
        } => Some((
            h.map(f32::to_bits),
            depth.to_bits(),
            opacity.to_bits(),
            clip_depth.map(|r| r.map(f32::to_bits)),
            canvas,
        )),
    };
    a.len() != b.len()
        || a.iter()
            .zip(b)
            .any(|((i, p), (j, q))| i != j || bits(p) != bits(q))
}

#[cfg(test)]
mod tests {
    use super::{moved, Placement};
    use std::collections::BTreeMap;

    fn at(x: f32) -> Placement {
        Placement::Visible {
            h: [1., 0., x, 0., 1., 0., 0., 0., 1.],
            depth: 0.,
            opacity: 1.,
            clip_depth: [[0., 0., 1.]; 2],
            canvas: 7,
        }
    }

    #[test]
    fn a_child_moved_or_hidden_or_newly_placed_is_a_move() {
        let painted = BTreeMap::from([(1, at(10.)), (2, Placement::Hidden)]);
        assert!(!moved(&painted, &painted.clone()));
        assert!(moved(
            &painted,
            &BTreeMap::from([(1, at(10.5)), (2, Placement::Hidden)])
        ));
        assert!(moved(
            &painted,
            &BTreeMap::from([(1, at(10.)), (2, at(0.))])
        ));
        assert!(moved(&painted, &BTreeMap::from([(1, at(10.))])));
        // Placed before the first paint read any
        assert!(moved(&BTreeMap::new(), &painted));
    }

    #[test]
    fn placement_opacity_multiplies_paint_and_alone_requests_android_repaint() {
        use crate::presenter::{PainterChoice, Presenter};
        use crate::surfaces::{tests, Abi, Canvas};
        let (probe, compat) = tests::fixture();
        let source = r##"component App
  view
    canvas testId="canvas" width=100 height=100
      view testId="label" role="button" position="absolute" width=40 height=40 background-color="#ff0000" opacity=0.5
"##;
        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(source),
            (),
            (100., 100.),
            1.,
            probe.parent().unwrap().into(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let id = |name| {
            p.host
                .kernel()
                .node_by_key(p.host.kernel().find_by_test_id(name)[0])
                .unwrap()
                .id
        };
        let (canvas, label) = (id("canvas"), id("label"));
        p.surfaces
            .abis
            .insert(String::new(), Abi::open_path(&probe, &compat, "").unwrap());
        p.surfaces.canvases.insert(
            canvas,
            Canvas {
                performance: false,
                id: 1,
                name: "probe".into(),
                artifact: String::new(),
                owner: true,
                since: 0,
                held: Default::default(),
                restored_controls: None,
                restore_error: None,
                restore_input: false,
                restore_bytes: None,
                restore_logged: false,
                uploaded: None,
            },
        );
        let opacity = |placements: &BTreeMap<u32, Placement>| match placements[&label] {
            Placement::Visible { opacity, .. } => opacity,
            Placement::Hidden => panic!("label hidden"),
        };
        assert_eq!(opacity(&p.surfaces.placements(&p.host, false)), 1.);

        let dir = probe.parent().unwrap();
        let source = std::fs::read_to_string(dir.join("probe.c"))
            .unwrap()
            .replace(
                "if (len<16) return 0;",
                "if (outcome != 1) return outcome; if (len<16) return 0;",
            );
        let source = "static unsigned int outcome = 1;\n".to_owned()
            + &source
            + r#"
static float opacity = 1;
static uint32_t reads;
float gpu_placement_opacity(uint32_t id, uint32_t index) { reads++; return opacity; }
void test_opacity(float value) { opacity = value; }
void test_outcome(uint32_t value) { outcome = value; }
uint32_t test_opacity_reads(void) { return reads; }
"#;
        let c = dir.join("opacity.c");
        std::fs::write(&c, source).unwrap();
        let path = dir.join(format!("libopacity.{}", std::env::consts::DLL_EXTENSION));
        tests::compile_fixture(&c, &path);
        let mut compat = compat;
        compat["embedded"]["gpu"]["sha256"] =
            crate::surfaces::digest::file(&path, true).unwrap().into();
        p.surfaces
            .abis
            .insert(String::new(), Abi::open_path(&path, &compat, "").unwrap());
        let set = |p: &Presenter<()>, value| unsafe {
            p.surfaces.abis[""].symbol::<unsafe extern "C" fn(f32)>(b"test_opacity")(value);
        };
        for (value, expected) in [(1., 127), (0.5, 191), (0., 255)] {
            set(&p, value);
            if value != 1. {
                assert!(p.surfaces.placements_moved(&p.host, &p.brush.placements));
            }
            p.dirty = true;
            let frame = p.frame();
            let pixel = frame.pixel(20, 20).unwrap();
            assert_eq!(pixel.red(), 255);
            assert!(
                (i32::from(pixel.green()) - expected).abs() <= 1,
                "{pixel:?}"
            );
            let hit = p.boxes().iter().find(|b| b.id == label).unwrap();
            assert_eq!(hit.rect, (0., 0., 40., 40.));
            assert!(!p.surfaces.placements_moved(&p.host, &p.brush.placements));
        }
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            set(&p, value);
            assert_eq!(opacity(&p.surfaces.placements(&p.host, false)), 1.);
        }
        let abi = &p.surfaces.abis[""];
        unsafe {
            let reads = abi.symbol::<unsafe extern "C" fn() -> u32>(b"test_opacity_reads");
            let outcome = abi.symbol::<unsafe extern "C" fn(u32)>(b"test_outcome");
            let before = reads();
            outcome(2);
            assert!(matches!(
                p.surfaces.placements(&p.host, false)[&label],
                Placement::Hidden
            ));
            outcome(0);
            assert!(p.surfaces.placements(&p.host, false).is_empty());
            assert_eq!(reads(), before);
        }
        p.dirty = true;
        let frame = p.frame();
        assert!((i32::from(frame.pixel(20, 20).unwrap().green()) - 127).abs() <= 1);
    }

    /// A module whose surface composites its children, and which keeps what
    /// `gpu_texture` was given; beside the probe, which removes it.
    fn composite_module() -> (
        crate::surfaces::tests::Probe,
        std::path::PathBuf,
        serde_json::Value,
    ) {
        let (probe, _) = crate::surfaces::tests::fixture();
        let dir = probe.parent().unwrap();
        let source = std::fs::read_to_string(dir.join("probe.c"))
            .unwrap()
            .replace(
                "uint32_t gpu_children_mode(void) { return 3; }",
                "uint32_t gpu_children_mode(void) { return 1; }",
            );
        let source = source
            + r#"
static uint32_t textures, texture_w, texture_h, texture_at;
static unsigned char texture_pixel[4];
uint32_t gpu_texture(uint32_t id, uint32_t w, uint32_t h, const unsigned char *bytes, size_t len) {
  textures++; texture_w = w; texture_h = h;
  size_t at = ((size_t)texture_at * w + texture_at) * 4;
  if (at + 4 <= len) memcpy(texture_pixel, bytes + at, 4);
  return 0;
}
void test_texture_at(uint32_t at) { texture_at = at; }
uint32_t test_textures(void) { return textures; }
uint32_t test_texture_w(void) { return texture_w; }
uint32_t test_texture_h(void) { return texture_h; }
uint32_t test_texture_red(void) { return texture_pixel[0]; }
uint32_t test_texture_alpha(void) { return texture_pixel[3]; }
"#;
        let c = dir.join("composite.c");
        std::fs::write(&c, source).unwrap();
        let path = dir.join(format!("libcomposite.{}", std::env::consts::DLL_EXTENSION));
        crate::surfaces::tests::compile_fixture(&c, &path);
        let digest = crate::surfaces::digest::file(&path, true).unwrap();
        let compat = serde_json::json!({"id":"cohort", "inputs":{"app":"test.app"}, "embedded":{"gpu":{
            "name":path.file_name().unwrap().to_str().unwrap(),"sha256":digest,
            "app":"test.app","cohort":"cohort","trust":"production"}}});
        (probe, path, compat)
    }

    #[test]
    fn a_composite_canvas_is_given_its_children_and_they_are_not_painted_over_it() {
        use crate::presenter::Presenter;
        let (_probe, path, compat) = composite_module();
        let source = r##"component App
  view
    column
      canvas testId="glass" width=100 height=100
        view testId="label" position="absolute" left=20 top=20 width=40 height=40 background-color="#FF0000"
"##;
        let plan = contract::compile(source).unwrap();
        let (mut p, _) = Presenter::boot(
            &plan.encode(),
            (),
            (100., 200.),
            2.,
            path.parent().unwrap().into(),
        )
        .unwrap();
        let canvas = p
            .host
            .kernel()
            .node_by_key(p.host.kernel().find_by_test_id("glass")[0])
            .unwrap()
            .id;
        let abi = crate::surfaces::Abi::open_path(&path, &compat, "").unwrap();
        let call = |abi: &crate::surfaces::Abi, name: &'static [u8]| unsafe {
            abi.symbol::<unsafe extern "C" fn() -> u32>(name)()
        };
        // The label's middle, in the picture's device pixels: (20 + 20) points at scale 2.
        unsafe { abi.symbol::<unsafe extern "C" fn(u32)>(b"test_texture_at")(80) };
        p.surfaces.abis.insert(String::new(), abi);
        p.surfaces.canvases.insert(
            canvas,
            crate::surfaces::Canvas {
                performance: false,
                id: 1,
                name: "glass".into(),
                artifact: String::new(),
                owner: true,
                since: 0,
                held: Default::default(),
                restored_controls: None,
                restore_error: None,
                restore_input: false,
                restore_bytes: None,
                restore_logged: false,
                uploaded: None,
            },
        );
        p.dirty = true;
        let frame = p.frame();
        let abi = &p.surfaces.abis[""];
        assert_eq!(call(abi, b"test_textures"), 1);
        assert_eq!(
            (call(abi, b"test_texture_w"), call(abi, b"test_texture_h")),
            (200, 200)
        );
        assert_eq!(
            (
                call(abi, b"test_texture_red"),
                call(abi, b"test_texture_alpha")
            ),
            (255, 255)
        );
        // Not painted over the canvas: the page there is not the label's red.
        let at = frame.pixel(80, 80).unwrap();
        assert!(at.green() > 128 || at.alpha() < 128, "{at:?}");
        // The same picture again is not uploaded again
        p.dirty = true;
        p.frame();
        assert_eq!(call(&p.surfaces.abis[""], b"test_textures"), 1);
        // A canvas that replaces it under the same view (a reload) is given
        // the same picture: what was given went with the canvas.
        let mut replacement = p.surfaces.canvases.remove(&canvas).unwrap();
        (replacement.id, replacement.uploaded) = (2, None);
        p.surfaces.canvases.insert(canvas, replacement);
        p.dirty = true;
        p.frame();
        assert_eq!(call(&p.surfaces.abis[""], b"test_textures"), 2);
        // The label keeps its hit box at its layout frame.
        let label = p
            .host
            .kernel()
            .node_by_key(p.host.kernel().find_by_test_id("label")[0])
            .unwrap()
            .id;
        let hit = p.boxes().iter().find(|b| b.id == label).map(|b| b.rect);
        let (x, y, w, h) = hit.expect("the label's hit box");
        assert!(
            (x - 20.).abs() < 0.01
                && (y - 20.).abs() < 0.01
                && (w - 40.).abs() < 0.01
                && (h - 40.).abs() < 0.01,
            "{hit:?}"
        );
        drop(p);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
