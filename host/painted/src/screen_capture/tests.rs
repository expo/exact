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
    fn local(&self, capture: &Json) -> PathBuf {
        self.roots()[2].join(
            capture["path"]
                .as_str()
                .unwrap()
                .strip_prefix("app:/tmp/")
                .unwrap(),
        )
    }
    // Canvas cache tests replace their own named image after a capture.
    fn replace_image(&self, displays: Vec<Display>) {
        let answer = self.capture(displays, &Mutex::new(true)).unwrap();
        std::fs::copy(
            self.local(&answer["captures"][0]),
            self.roots()[2].join("freeze/screen-0.png"),
        )
        .unwrap();
    }
    fn capture(&self, displays: Vec<Display>, held: &Mutex<bool>) -> Result<Json, Failure> {
        write(
            &self.roots(),
            "app:/tmp/freeze",
            displays,
            &Encoding::default(),
            held,
            &|_, _| {},
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
    let path = files.local(&answer["captures"][0]);
    let bytes = std::fs::read(&path).unwrap();
    let image = tiny_skia::Pixmap::decode_png(&bytes).unwrap();
    assert_eq!((image.width(), image.height()), (160, 80));
    assert_eq!(&image.data()[..4], &[32, 64, 128, 255]);
    assert_eq!(&image.data()[(8 * 160 + 8) * 4..][..4], &[255; 4]);
    // Keep an earlier display open with sharing denied while sixty more captures publish.
    let mut reader = std::fs::OpenOptions::new();
    reader.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        reader.share_mode(0);
    }
    let mut held = reader.open(&path).unwrap();
    let mut paths =
        std::collections::HashSet::from([path.clone(), files.local(&answer["captures"][1])]);
    for _ in 0..60 {
        let next = files.capture(two(), &Mutex::new(true)).unwrap();
        for image in next["captures"].as_array().unwrap() {
            assert!(paths.insert(files.local(image)));
        }
    }
    let mut still = Vec::new();
    std::io::Read::read_to_end(&mut held, &mut still).unwrap();
    assert_eq!(still, bytes);
    assert_eq!(
        std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
        paths.len()
    );
    let at_root = write(
        &files.roots(),
        "app:/tmp",
        two(),
        &Encoding::default(),
        &Mutex::new(true),
        &|_, _| {},
    )
    .unwrap();
    assert_eq!(
        files.local(&at_root["captures"][0]).parent(),
        Some(files.roots()[2].as_path())
    );
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
            &|_, _| {},
        )
        .unwrap();
        for index in 0..2 {
            let path = files.local(&answer["captures"][index]);
            assert_eq!(path.parent().unwrap(), files.roots()[2].join(name));
            assert!(path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .ends_with(&format!("-{index}.png")));
            assert!(tiny_skia::Pixmap::load_png(path).is_ok());
        }
    }
}

#[test]
fn failure_or_cancellation_keeps_every_previous_file() {
    let files = Files::new();
    let first = files.capture(two(), &Mutex::new(true)).unwrap();
    let paths = [0, 1].map(|n| files.local(&first["captures"][n]));
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
    let answer = files.capture(two(), &Mutex::new(true)).unwrap();
    assert!(files.local(&answer["captures"][1]).is_file());
    assert!(files.roots()[2].join("freeze/screen-1.png").is_symlink());
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
        vec![
            ("still".into(), 0),
            ("blank".into(), 0),
            ("pattern".into(), 0),
            ("saved".into(), 0),
            ("selected".into(), 1),
        ]
    }
    fn draw_2d(
        &mut self,
        surface: &str,
        args: &[exact_plan::Value],
        ctx: &exact_canvas::Context2d,
        _: &exact_canvas::Frame,
    ) -> Result<bool, exact_canvas::DrawError> {
        ctx.clear_rect(0., 0., 32., 32.);
        if surface == "selected" {
            if let Some(path) = args
                .first()
                .and_then(|v| v.as_str())
                .filter(|p| !p.is_empty())
            {
                ctx.draw_image_with_image_handle(path, 0., 0.)?;
            }
            return Ok(false);
        }
        if surface == "blank" {
            return Ok(false);
        }
        // A pattern made once and then held between draws: by `fillStyle`,
        // or by a saved state under a colour.
        let saved = surface == "saved";
        if surface == "pattern" || saved {
            if saved {
                ctx.restore();
            }
            if !matches!(ctx.fill_style(), exact_canvas::Style::Pattern(_)) {
                let pattern = ctx.create_pattern_with_image_handle("freeze/screen-0.png", "repeat");
                if let Some(pattern) = pattern.unwrap() {
                    ctx.set_fill_style_canvas_pattern(&pattern);
                }
            }
            ctx.fill_rect(0., 0., 32., 32.);
            if saved {
                ctx.save();
                ctx.set_fill_style_str("black");
            }
            return Ok(true);
        }
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
            frame: None,
            screen: Json::Null,
            width: 32,
            height: 32,
            read: Box::new(move || Ok([red, 64, 128, 255].repeat(32 * 32))),
        }]
    };
    files.replace_image(displays(32));
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
    presenter.sync_canvases(true);
    let pixels = |p: &Presenter<StillCanvas>| {
        let bitmap = p.images().bitmaps.values().next().unwrap();
        let handle = p.host().cached_canvas_image("freeze/screen-0.png").unwrap();
        assert_eq!(
            bitmap.pixels().data().as_ptr(),
            handle.data().as_ptr(),
            "both consumers share the decoded allocation"
        );
        let canvases = p.host().canvas_snapshots().borrow();
        (
            bitmap.pixels().data()[..4].to_vec(),
            canvases.values().next().unwrap().pixels.data()[..4].to_vec(),
        )
    };
    assert_eq!(
        pixels(&presenter),
        (vec![32, 64, 128, 255], vec![32, 64, 128, 255])
    );
    files.replace_image(displays(192));
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
    presenter.sync_canvases(true);
    assert_eq!(
        pixels(&presenter),
        (vec![192, 64, 128, 255], vec![192, 64, 128, 255])
    );
    #[cfg(any(windows, target_os = "macos"))]
    {
        let encoding = Encoding::new("image/jpeg", Some(0.6));
        let answer = write(
            &files.roots(),
            "app:/tmp/freeze",
            displays(96),
            &encoding,
            &Mutex::new(true),
            &|_, _| {},
        )
        .unwrap();
        let path = files.local(&answer["captures"][0]);
        std::fs::copy(path, files.roots()[2].join("freeze/screen-0.png")).unwrap();
        presenter.picture_changed("freeze/screen-0.png", false);
        presenter.wait_images(std::time::Duration::from_secs(30));
        let (image, canvas) = pixels(&presenter);
        assert_eq!(image, canvas);
        assert!(image[0].abs_diff(96) < 5, "{image:?}");
    }
}

#[test]
fn a_canvas_whose_still_was_missing_draws_it_once_captured() {
    use crate::presenter::{PainterChoice, Presenter};
    let files = Files::new();
    std::fs::create_dir_all(&files.roots()[2]).unwrap();
    let plan =
        crate::test_plan("component App\n  view\n    canvas surface=still() width=32 height=32\n");
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
    let pixel = |p: &mut Presenter<StillCanvas>| {
        p.sync_canvases(true);
        p.wait_images(std::time::Duration::from_secs(30));
        let canvases = p.host().canvas_snapshots().borrow();
        canvases.values().next().unwrap().pixels.data()[..4].to_vec()
    };
    assert_eq!(pixel(&mut presenter), [0; 4]);
    let red = vec![Display {
        frame: None,
        screen: Json::Null,
        width: 32,
        height: 32,
        read: Box::new(|| Ok([192, 64, 128, 255].repeat(32 * 32))),
    }];
    files.replace_image(red);
    presenter.picture_changed("freeze/screen-0.png", false);
    assert_eq!(pixel(&mut presenter), [192, 64, 128, 255]);
}

/// A changed still is decoded again only for a mounted canvas whose latest
/// draw asked for it, and a canvas mounted after the change draws the new
/// file, whether or not the earlier one decoded.
#[test]
fn a_still_reloads_only_for_mounted_canvases_and_a_remount_draws_it() {
    use crate::presenter::{PainterChoice, Presenter};
    let files = Files::new();
    std::fs::create_dir_all(&files.roots()[2]).unwrap();
    let plan = crate::test_plan(
        "component App\n  state shown = true\n  state other = false\n  action toggle\n    shown = not shown\n  action add\n    other = true\n  view\n    column\n      button press=toggle testId=\"toggle\" width=10 height=10\n      button press=add testId=\"add\" width=10 height=10\n      if other\n        canvas surface=blank() testId=\"blank\" width=32 height=32\n      if shown\n        canvas surface=still() testId=\"still\" width=32 height=32\n",
    );
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
    let still = "freeze/screen-0.png";
    let named = |p: &Presenter<StillCanvas>, id: &str| {
        let k = p.host().kernel();
        k.node_by_key(k.find_by_test_id(id)[0]).unwrap().id
    };
    let pixel = |p: &mut Presenter<StillCanvas>| {
        p.sync_canvases(true);
        p.wait_images(std::time::Duration::from_secs(30));
        let canvases = p.host().canvas_snapshots().borrow();
        canvases[&named(p, "still")].pixels.data()[..4].to_vec()
    };
    let tap = |p: &mut Presenter<StillCanvas>, id: &str| {
        p.tap(named(p, id)).unwrap();
    };
    let toggle = |p: &mut Presenter<StillCanvas>| tap(p, "toggle");
    let capture = |p: &mut Presenter<StillCanvas>, red: u8| {
        let display = Display {
            frame: None,
            screen: Json::Null,
            width: 32,
            height: 32,
            read: Box::new(move || Ok([red, 64, 128, 255].repeat(32 * 32))),
        };
        files.replace_image(vec![display]);
        p.picture_changed(still, false);
    };
    // No canvas is left while the file appears.
    assert_eq!(pixel(&mut presenter), [0; 4]);
    toggle(&mut presenter);
    capture(&mut presenter, 192);
    toggle(&mut presenter);
    assert_eq!(pixel(&mut presenter), [192, 64, 128, 255]);
    // Another canvas stays while the still's goes.
    tap(&mut presenter, "add");
    drop(presenter.frame());
    let id = named(&presenter, "still");
    let blank = named(&presenter, "blank");
    let bitmap = Arc::downgrade(&presenter.host().canvas_snapshots().borrow()[&id].pixels);
    let kept = Arc::downgrade(&presenter.host().canvas_snapshots().borrow()[&blank].pixels);
    let image = Arc::downgrade(presenter.host().cached_canvas_image(still).unwrap());
    presenter.timer_visibility(true);
    toggle(&mut presenter);
    assert!(
        bitmap.upgrade().is_none(),
        "the hidden commit must release the unmounted bitmap"
    );
    assert!(
        kept.upgrade().is_some(),
        "the other canvas is still mounted"
    );
    assert!(
        image.upgrade().is_none(),
        "the last canvas using the still is gone"
    );
    capture(&mut presenter, 32);
    assert!(presenter.host().cached_canvas_image(still).is_none());
    toggle(&mut presenter);
    assert_eq!(pixel(&mut presenter), [32, 64, 128, 255]);
}

/// A pattern's image stays while a canvas's retained style holds it, though
/// later draws only fill with it.
#[test]
fn a_retained_pattern_keeps_its_image_across_draws() {
    use crate::presenter::{PainterChoice, Presenter};
    let files = Files::new();
    let display = Display {
        frame: None,
        screen: Json::Null,
        width: 32,
        height: 32,
        read: Box::new(|| Ok([192, 64, 128, 255].repeat(32 * 32))),
    };
    files.replace_image(vec![display]);
    // One canvas at a time: another drawing the same image would keep it.
    for surface in ["pattern", "saved"] {
        let plan = crate::test_plan(&format!(
            "component App\n  view\n    canvas surface={surface}() width=32 height=32\n"
        ));
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
        for frame in 0..4 {
            presenter.host.advance(16. * frame as f64);
            presenter.sync_canvases(true);
            presenter.wait_images(std::time::Duration::from_secs(30));
            let canvases = presenter.host().canvas_snapshots().borrow();
            let pixel = &canvases.values().next().unwrap().pixels.data()[..4];
            assert_eq!(pixel, [192, 64, 128, 255], "{surface}, frame {frame}");
        }
    }
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

/// Large enough that a 16-point thumbnail decodes a smaller picture.
const STILL: u32 = 512;

struct SyntheticScreen(Context, Captures);
impl Package for SyntheticScreen {
    fn new(context: Context) -> Self {
        Self(context, Captures::default())
    }
    fn later(&mut self, _: &str, args: &Json, _: Option<&Json>, reply: Reply) {
        // Every pixel differs from its neighbours, so a lossy or shifted
        // round trip cannot match.
        let pixels: Vec<u8> = (0..STILL * STILL)
            .flat_map(|i| [i as u8, (i >> 4) as u8, (i * 7) as u8, 255])
            .collect();
        let display = Display {
            frame: None,
            screen: json!({"allocation": pixels.as_ptr() as usize}),
            width: STILL,
            height: STILL,
            read: Box::new(move || Ok(pixels)),
        };
        self.1.start(&self.0, args, vec![display], reply);
    }
}

#[test]
fn image_and_canvas_share_the_capture_allocation_only_while_the_file_matches() {
    use crate::presenter::{PainterChoice, Presenter};
    let plan = crate::test_plan(
        r#"component App
  state path = ""
  state late = false
  action selectFile(value: string)
    path = value
  action showLate(value: string)
    late = true
  view
    column
      input testId="path" value=path input=selectFile width=96 height=20
      input testId="late" input=showLate width=96 height=20
      image path width=512 height=512
      image path width=16 height=16
      canvas surface=selected(path) width=32 height=32
      if late
        image path testId="later" width=512 height=512
"#,
    );
    let directory = format!("app:/tmp/exact-publish-test-{}", std::process::id());
    let roots = crate::picker::roots();
    let _files = Files(roots[2].join(directory.strip_prefix("app:/tmp/").unwrap()));
    let named = |p: &Presenter<StillCanvas>, id: &str| {
        let k = p.host().kernel();
        k.node_by_key(k.find_by_test_id(id)[0]).unwrap().id
    };
    // Both readers have their answer: pixels, or a failure.
    let settle = |p: &mut Presenter<StillCanvas>, pictures: usize| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        p.wait_images(std::time::Duration::from_secs(30));
        while (p.images().pending() || p.images().bitmaps.len() < pictures)
            && std::time::Instant::now() < deadline
        {
            p.wait_images(std::time::Duration::from_millis(50));
        }
        assert!(!p.images().pending(), "the images settle");
    };
    for file in ["original", "replaced", "corrupted", "deleted"] {
        let (mut p, error) = Presenter::boot_with(
            &plan,
            StillCanvas,
            (600., 700.),
            1.,
            roots[2].clone(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.serve_packages(vec![crate::package::Entry::of::<SyntheticScreen>(
            "screen-capture",
        )]);
        let handler = p.host().runner().native_slot().handler().unwrap();
        let (sent, received) = std::sync::mpsc::channel();
        handler(json!({"extension":"screen-capture", "send":"captureScreens", "args":{"directory":directory}}).to_string().into_bytes(),
            exact_runner::Reply::new(move |outcome| sent.send(outcome).unwrap()));
        p.run_commands(StillCanvas::default);
        let exact_runner::Outcome::Response(response) = received
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap()
        else {
            panic!("no capture reply")
        };
        let answer: Json = serde_json::from_slice(&response.body).unwrap();
        let still = &answer["captures"][0];
        let path = still["path"].as_str().unwrap();
        let original = still["screen"]["allocation"].as_u64().unwrap() as usize;
        let disk = roots[2].join(path.strip_prefix("app:/tmp/").unwrap());
        match file {
            "replaced" => {
                let replacement = [128, 64, 32, 255].repeat((STILL * STILL) as usize);
                std::fs::write(
                    &disk,
                    crate::canvas2d::capture::png(STILL, STILL, &replacement).unwrap(),
                )
                .unwrap();
            }
            // The same length: its new modification time tells it apart.
            "corrupted" => {
                let mut bytes = std::fs::read(&disk).unwrap();
                let data = bytes.windows(4).position(|w| w == b"IDAT").unwrap() + 64;
                bytes[data] ^= 0xff;
                std::fs::write(&disk, bytes).unwrap();
            }
            "deleted" => std::fs::remove_file(&disk).unwrap(),
            _ => {}
        }
        p.run_commands(StillCanvas::default);
        p.type_text(named(&p, "path"), path).unwrap();
        let readable = matches!(file, "original" | "replaced");
        settle(&mut p, if readable { 2 } else { 0 });
        let cached = p.host().cached_canvas_image(path).cloned();
        let bitmaps = &p.images().bitmaps;
        if !readable {
            assert!(
                cached.is_none() && bitmaps.is_empty(),
                "both readers see the file as it is"
            );
            continue;
        }
        let canvas = cached.unwrap();
        let image = bitmaps.values().find(|b| b.width() == STILL).unwrap();
        let small = bitmaps.values().find(|b| b.width() < STILL).unwrap();
        assert_ne!(
            small.pixels().data().as_ptr(),
            image.pixels().data().as_ptr(),
            "a thumbnail decodes its own pixels rather than pin the whole still"
        );
        let ptr = image.pixels().data().as_ptr() as usize;
        assert_eq!(canvas.data().as_ptr() as usize, ptr);
        if file == "replaced" {
            assert_ne!(ptr, original);
            assert_eq!(&canvas.data()[..4], [128, 64, 32, 255]);
            continue;
        }
        assert_eq!(
            ptr, original,
            "the actual capture allocation reaches both readers"
        );
        let decoded = tiny_skia::Pixmap::load_png(&disk).unwrap();
        assert_eq!(
            canvas.data(),
            decoded.data(),
            "readers see what the file decodes to"
        );
        // A reader that comes after the file is gone does not get its pixels.
        std::fs::remove_file(&disk).unwrap();
        p.type_text(named(&p, "late"), "x").unwrap();
        settle(&mut p, 0);
        let later = named(&p, "later");
        assert!(!p.images().bitmaps.contains_key(&later));
    }
}

#[test]
fn a_captures_pixels_reach_its_readers_in_the_turn_that_asks() {
    use crate::presenter::{PainterChoice, Presenter};
    let plan = crate::test_plan(
        r#"component App
  state path = ""
  action selectFile(value: string)
    path = value
  view
    column
      input testId="path" value=path input=selectFile width=96 height=20
      image path width=512 height=512
      canvas surface=selected(path) width=32 height=32
"#,
    );
    let directory = format!("app:/tmp/exact-asked-test-{}", std::process::id());
    let roots = crate::picker::roots();
    let _files = Files(roots[2].join(directory.strip_prefix("app:/tmp/").unwrap()));
    let (mut p, error) = Presenter::boot_with(
        &plan,
        StillCanvas,
        (600., 600.),
        1.,
        roots[2].clone(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.serve_packages(vec![crate::package::Entry::of::<SyntheticScreen>(
        "screen-capture",
    )]);
    let handler = p.host().runner().native_slot().handler().unwrap();
    let (sent, received) = std::sync::mpsc::channel();
    handler(json!({"extension":"screen-capture", "send":"captureScreens", "args":{"directory":directory}}).to_string().into_bytes(),
        exact_runner::Reply::new(move |outcome| sent.send(outcome).unwrap()));
    p.run_commands(StillCanvas::default);
    let exact_runner::Outcome::Response(response) = received
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap()
    else {
        panic!("no capture reply")
    };
    let answer: Json = serde_json::from_slice(&response.body).unwrap();
    let path = answer["captures"][0]["path"].as_str().unwrap().to_owned();
    let original = answer["captures"][0]["screen"]["allocation"]
        .as_u64()
        .unwrap() as usize;
    p.run_commands(StillCanvas::default);
    let kernel = p.host().kernel();
    let field = kernel
        .node_by_key(kernel.find_by_test_id("path")[0])
        .unwrap()
        .id;
    p.type_text(field, &path).unwrap();
    // No wait: nothing has read the file's header.
    p.settle_pictures(std::time::Duration::ZERO);
    let canvas = p
        .host()
        .cached_canvas_image(&path)
        .expect("the canvas has the still");
    assert_eq!(canvas.data().as_ptr() as usize, original);
    let image = p
        .images()
        .bitmaps
        .values()
        .next()
        .expect("the image has it");
    assert_eq!(image.pixels().data().as_ptr() as usize, original);
}

/// The reporter's resident freeze (expo/exact#1315): a capture drawn by a
/// canvas and an image, the page hidden, both unmounted and the files
/// deleted. Nothing can ask for those pixels again, so the session keeps
/// none of them: not the capture's handed pixels, and not a canvas's own
/// decode of a file the app wrote.
#[test]
fn a_still_no_view_shows_leaves_the_raster_cache() {
    use crate::presenter::{PainterChoice, Presenter};
    let plan = crate::test_plan(
        r#"component App
  state path = ""
  state shown = false
  state image = false
  action selectFile(value: string)
    path = value
  action show(value: string)
    shown = true
  action unmount(value: string)
    shown = false
  action withImage(value: string)
    image = true
  view
    column
      input testId="path" value=path input=selectFile width=96 height=20
      input testId="show" input=show width=96 height=20
      input testId="unmount" input=unmount width=96 height=20
      input testId="image" input=withImage width=96 height=20
      if shown
        canvas surface=selected(path) width=32 height=32
        if image
          image path width=512 height=512
"#,
    );
    let directory = format!("app:/tmp/exact-released-test-{}", std::process::id());
    let roots = crate::picker::roots();
    let _files = Files(roots[2].join(directory.strip_prefix("app:/tmp/").unwrap()));
    let disk = |path: &str| roots[2].join(path.strip_prefix("app:/tmp/").unwrap());
    let named = |p: &Presenter<StillCanvas>, id: &str| {
        let k = p.host().kernel();
        k.node_by_key(k.find_by_test_id(id)[0]).unwrap().id
    };
    // The capture's pixels are handed to both readers; a file the app
    // wrote is decoded on a worker, for the canvas alone.
    for written in [false, true] {
        let (mut p, error) = Presenter::boot_with(
            &plan,
            StillCanvas,
            (600., 700.),
            1.,
            roots[2].clone(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.serve_packages(vec![crate::package::Entry::of::<SyntheticScreen>(
            "screen-capture",
        )]);
        for press in 0..3 {
            let path = if written {
                let path = format!("{directory}/written-{press}.png");
                let pixels = [32, 64, 128, 255].repeat((STILL * STILL) as usize);
                let png = crate::canvas2d::capture::png(STILL, STILL, &pixels).unwrap();
                std::fs::create_dir_all(disk(&directory)).unwrap();
                std::fs::write(disk(&path), png).unwrap();
                path
            } else {
                let handler = p.host().runner().native_slot().handler().unwrap();
                let (sent, received) = std::sync::mpsc::channel();
                handler(json!({"extension":"screen-capture", "send":"captureScreens", "args":{"directory":directory}}).to_string().into_bytes(),
                    exact_runner::Reply::new(move |outcome| sent.send(outcome).unwrap()));
                p.run_commands(StillCanvas::default);
                let exact_runner::Outcome::Response(response) = received
                    .recv_timeout(std::time::Duration::from_secs(10))
                    .unwrap()
                else {
                    panic!("no capture reply")
                };
                let answer: Json = serde_json::from_slice(&response.body).unwrap();
                answer["captures"][0]["path"].as_str().unwrap().to_owned()
            };
            p.run_commands(StillCanvas::default);
            let mut page = p.host().runner().page();
            page.hidden = false;
            p.set_page(page);
            p.type_text(named(&p, "path"), &path).unwrap();
            if !written {
                p.type_text(named(&p, "image"), "y").unwrap();
            }
            p.type_text(named(&p, "show"), "y").unwrap();
            let images = usize::from(!written);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
            while (p.host().cached_canvas_image(&path).is_none()
                || p.images().bitmaps.len() < images)
                && std::time::Instant::now() < deadline
            {
                p.wait_images(std::time::Duration::from_millis(50));
            }
            assert!(
                p.host().cached_canvas_image(&path).is_some() && p.images().bitmaps.len() == images,
                "written {written}, press {press}: {:?}",
                p.images().stats()
            );
            // Hidden, then unmounted and deleted, as the reporter's app does.
            let mut page = p.host().runner().page();
            page.hidden = true;
            p.set_page(page);
            p.type_text(named(&p, "unmount"), "y").unwrap();
            p.wait_images(std::time::Duration::from_millis(100));
            std::fs::remove_file(disk(&path)).unwrap();
            p.wait_images(std::time::Duration::from_millis(100));
            assert!(p.host().cached_canvas_image(&path).is_none());
            let stats = p.images().stats();
            assert_eq!(
                (stats.resident_bytes, stats.reserved_bytes),
                (0, 0),
                "written {written}, press {press}: {stats:?}"
            );
        }
    }
}

#[test]
fn targeted_fixture_selects_before_quota_and_keeps_local_units() {
    let screens = fixture_screens(&json!([
        {"left":-240,"top":0,"width":240,"height":100,"devicePixelRatio":1.25},
        {"left":0,"top":0,"width":100000,"height":100000,"devicePixelRatio":2},
    ]));
    let args = json!({"screen":screens[0],"rect":{"x":100,"y":20,"width":80,"height":48}});
    let image = fixture_targets(&screens, &args, None)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!((image.width, image.height), (100, 60));
    assert!(image.frame.is_none());
    let shot = fixture_targets(&screens, &json!({"window":"held"}), Some("held"))
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(shot.frame.unwrap()["x"], -240);
    assert_eq!(
        fixture_targets(&screens, &json!({"window":"held"}), None)
            .err()
            .unwrap()
            .0,
        "NotFoundError"
    );
}

#[test]
fn the_painted_substitute_captures_the_looked_up_window_for_one_session() {
    let context = Context::new(
        "screen-capture",
        true,
        true,
        Default::default(),
        Default::default(),
        Default::default(),
        Arc::new(|_| {}),
    );
    let folder = format!("exact-guide-test-{}", std::process::id());
    let _files = Files(context.temporary().join(&folder));
    let mut package = FixtureCapture::new(context);
    let fixture =
        json!({"screens":[{"left":-30,"top":0,"width":30,"height":20,"devicePixelRatio":2}]});
    let call = |package: &mut FixtureCapture, name: &str, args| {
        let (tx, rx) = std::sync::mpsc::channel();
        package.later(
            name,
            &args,
            Some(&fixture),
            Reply::new(exact_runner::Reply::new(move |v| tx.send(v).unwrap())),
        );
        let exact_runner::Outcome::Response(response) =
            rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap()
        else {
            panic!("no fixture reply")
        };
        serde_json::from_slice::<Json>(&response.body).unwrap()
    };
    let window = call(&mut package, "getWindowAtPoint", json!({"x":-12,"y":8}));
    assert_eq!(window, call(&mut package, "getFrontmostWindow", json!({})));
    let args = json!({"directory":format!("app:/tmp/{folder}"), "window":window["handle"], "cursor":"always"});
    let answer = call(&mut package, "captureScreens", args.clone());
    let still = &answer["captures"][0];
    assert_eq!(still["frame"], window["frame"]);
    assert_eq!(
        (still["width"].as_u64(), still["height"].as_u64()),
        (Some(60), Some(40))
    );
    package.destroy();
    assert_eq!(
        call(&mut package, "captureScreens", args)["error"],
        "NotFoundError"
    );
}
