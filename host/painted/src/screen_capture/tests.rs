use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(std::env::temp_dir().join(format!(
            "exact-stills-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
    fn roots(&self) -> [PathBuf; 3] {
        [
            self.0.join("data"),
            self.0.join("cache"),
            self.0.join("tmp"),
        ]
    }
    fn capture(&self, displays: Vec<Display>, held: &Mutex<bool>) -> Result<Json, Failure> {
        write(
            &self.roots(),
            "app:/tmp/freeze",
            displays,
            &Encoding::default(),
            held,
            &|_| {},
        )
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn two() -> Vec<Display> {
    fixtures(&json!([
        {"left":0,"top":0,"width":64,"height":40,"devicePixelRatio":1.25,
         "clientRect":{"x":0,"y":0,"width":64,"height":40}},
        {"left":-80,"top":-10,"width":80,"height":40,"devicePixelRatio":2,
         "clientRect":{"x":-128,"y":-16,"width":128,"height":64}}
    ]))
    .unwrap()
}

#[test]
fn still_files_have_sorted_physical_pixels_and_drawable_pngs() {
    let files = Files::new();
    let answer = files.capture(two(), &Mutex::new(true)).unwrap();
    assert_eq!(answer["captures"][0]["width"], 160);
    assert_eq!(answer["captures"][0]["height"], 80);
    assert_eq!(answer["captures"][0]["screen"]["clientRect"]["x"], -128);
    assert_eq!(answer["captures"][1]["width"], 80);
    let path = files.roots()[2].join("freeze/screen-0.png");
    let bytes = std::fs::read(&path).unwrap();
    let image = tiny_skia::Pixmap::decode_png(&bytes).unwrap();
    assert_eq!((image.width(), image.height()), (160, 80));
    assert_eq!(&image.data()[..4], &[32, 64, 128, 255]);
    assert_eq!(&image.data()[(8 * 160 + 8) * 4..][..4], &[255; 4]);
    files.capture(two(), &Mutex::new(true)).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(
        std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
        2
    );
    let at_root = write(
        &files.roots(),
        "app:/tmp",
        two(),
        &Encoding::default(),
        &Mutex::new(true),
        &|_| {},
    )
    .unwrap();
    assert_eq!(at_root["captures"][0]["path"], "app:/tmp/screen-0.png");
}

#[test]
fn capture_directory_names_are_paths_not_grant_text() {
    let files = Files::new();
    for name in ["My Captures", "Jane's Captures", "截图 #1"] {
        let directory = format!("app:/tmp/{name}");
        let answer = write(
            &files.roots(),
            &directory,
            two(),
            &Encoding::default(),
            &Mutex::new(true),
            &|_| {},
        )
        .unwrap();
        for index in 0..2 {
            assert_eq!(
                answer["captures"][index]["path"],
                format!("{directory}/screen-{index}.png")
            );
            let path = files.roots()[2]
                .join(name)
                .join(format!("screen-{index}.png"));
            assert!(tiny_skia::Pixmap::load_png(path).is_ok());
        }
    }
}

#[test]
fn failure_or_cancellation_keeps_every_previous_file() {
    let files = Files::new();
    files.capture(two(), &Mutex::new(true)).unwrap();
    let paths = [0, 1].map(|n| files.roots()[2].join(format!("freeze/screen-{n}.png")));
    let before = paths.each_ref().map(|p| std::fs::read(p).unwrap());
    let held = Arc::new(Mutex::new(true));
    let mut displays = two();
    displays[1].read = Box::new(|| Err(("EncodingError", "display disconnected".into())));
    assert_eq!(
        files.capture(displays, &held).unwrap_err().0,
        "EncodingError"
    );
    let mut displays = two();
    let cancel = held.clone();
    displays[1].read = Box::new(move || {
        *cancel.lock().unwrap() = false;
        Ok(card(1, 80, 50))
    });
    assert_eq!(files.capture(displays, &held).unwrap_err().0, "AbortError");
    assert_eq!(paths.each_ref().map(|p| std::fs::read(p).unwrap()), before);
    assert_eq!(
        std::fs::read_dir(paths[0].parent().unwrap())
            .unwrap()
            .count(),
        2
    );
}

#[test]
fn capture_bounds_precede_allocation_and_encoding_uses_canvas_rules() {
    assert!(sizes([(8192, 8192); 2]).is_ok());
    for dimensions in [
        vec![(8192, 8193)],
        vec![(8192, 8192); 3],
        vec![(u32::MAX, u32::MAX)],
        vec![(0, 10)],
    ] {
        assert_eq!(sizes(dimensions).unwrap_err().0, "QuotaExceededError");
    }
    let pixels = tiny_skia::Pixmap::from_vec(
        card(0, 64, 32),
        tiny_skia::IntSize::from_wh(64, 32).unwrap(),
    )
    .unwrap();
    let (jpeg, mime) = encode(&pixels, &Encoding::new("image/jpeg", Some(0.5))).unwrap();
    assert_eq!(mime, "image/jpeg");
    assert_eq!(&jpeg[..2], &[0xff, 0xd8]);
    assert_eq!(
        encode(&pixels, &Encoding::new("image/unknown", None))
            .unwrap()
            .1,
        "image/png"
    );
}

#[cfg(unix)]
#[test]
fn a_link_in_the_destination_cannot_receive_pixels() {
    use std::os::unix::fs::symlink;
    let files = Files::new();
    let outside = files.0.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::create_dir_all(&files.roots()[2]).unwrap();
    symlink(&outside, files.roots()[2].join("freeze")).unwrap();
    assert_eq!(
        files.capture(two(), &Mutex::new(true)).unwrap_err().0,
        "NotAllowedError"
    );
    std::fs::remove_file(files.roots()[2].join("freeze")).unwrap();
    std::fs::create_dir(files.roots()[2].join("freeze")).unwrap();
    symlink(
        outside.join("still.png"),
        files.roots()[2].join("freeze/screen-1.png"),
    )
    .unwrap();
    assert_eq!(
        files.capture(two(), &Mutex::new(true)).unwrap_err().0,
        "NotAllowedError"
    );
    assert!(!files.roots()[2].join("freeze/screen-0.png").exists());
    assert_eq!(std::fs::read_dir(outside).unwrap().count(), 0);
}

#[derive(Default)]
struct StillCanvas;
impl exact_runner::DataSource for StillCanvas {
    fn query(
        &mut self,
        name: &str,
        _: &[exact_plan::Value],
    ) -> Result<exact_plan::Value, exact_runner::DataError> {
        Err(exact_runner::DataError::UnknownSource(name.into()))
    }
    fn canvas_surfaces(&self) -> Vec<(String, usize)> {
        vec![("still".into(), 0)]
    }
    fn draw_2d(
        &mut self,
        _: &str,
        _: &[exact_plan::Value],
        ctx: &exact_canvas::Context2d,
        _: &exact_canvas::Frame,
    ) -> Result<bool, exact_canvas::DrawError> {
        ctx.clear_rect(0., 0., 32., 32.);
        ctx.draw_image_with_image_handle("freeze/screen-0.png", 0., 0.)
            .unwrap();
        Ok(false)
    }
}

#[test]
fn a_replaced_still_refreshes_image_and_canvas_handles() {
    use crate::presenter::{PainterChoice, Presenter};
    let files = Files::new();
    let displays = |red| {
        vec![Display {
            screen: Json::Null,
            width: 32,
            height: 32,
            read: Box::new(move || Ok([red, 64, 128, 255].repeat(32 * 32))),
        }]
    };
    files.capture(displays(32), &Mutex::new(true)).unwrap();
    let plan = crate::test_plan("component App\n  view\n    column\n      image \"freeze/screen-0.png\" width=32 height=32\n      canvas surface=still() width=32 height=32\n");
    let (mut presenter, error) = Presenter::boot_with(
        &plan,
        StillCanvas,
        (100., 100.),
        1.,
        files.roots()[2].clone(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    presenter.wait_images(std::time::Duration::from_secs(30));
    presenter.host.sync_canvases(1., true, &presenter.assets);
    let pixels = |p: &Presenter<StillCanvas>| {
        (
            p.images().bitmaps.values().next().unwrap().pixels().data()[..4].to_vec(),
            p.host()
                .canvas_snapshots()
                .values()
                .next()
                .unwrap()
                .pixels
                .data()[..4]
                .to_vec(),
        )
    };
    assert_eq!(
        pixels(&presenter),
        (vec![32, 64, 128, 255], vec![32, 64, 128, 255])
    );
    let published = std::cell::RefCell::new(Vec::new());
    write(
        &files.roots(),
        "app:/tmp/freeze",
        displays(192),
        &Encoding::default(),
        &Mutex::new(true),
        &|path| published.borrow_mut().push(path.to_owned()),
    )
    .unwrap();
    assert_eq!(*published.borrow(), ["app:/tmp/freeze/screen-0.png"]);
    presenter.serve_packages(vec![crate::package::Entry::of::<PictureNotice>(
        "screen-capture",
    )]);
    let handler = presenter.host().runner().native_slot().handler().unwrap();
    let (sent, received) = std::sync::mpsc::channel();
    handler(
        br#"{"extension":"screen-capture","send":"published"}"#.to_vec(),
        exact_runner::Reply::new(move |_| sent.send(()).unwrap()),
    );
    presenter.run_commands(StillCanvas::default);
    received
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    presenter.run_commands(StillCanvas::default);
    presenter.wait_images(std::time::Duration::from_secs(30));
    presenter.host.sync_canvases(1., true, &presenter.assets);
    assert_eq!(
        pixels(&presenter),
        (vec![192, 64, 128, 255], vec![192, 64, 128, 255])
    );
}

struct PictureNotice(Context);
impl Package for PictureNotice {
    fn new(context: Context) -> Self {
        Self(context)
    }
    fn later(&mut self, _: &str, _: &Json, _: Option<&Json>, reply: Reply) {
        self.0.notifier().picture_changed("freeze/screen-0.png");
        reply.send(Json::Null);
    }
}
