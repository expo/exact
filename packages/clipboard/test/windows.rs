use super::*;

#[test]
fn item_refusals_and_text_preserve_the_clipboard_contract() {
    let no_file = |_: &str| Err(refused("NotFoundError", "missing"));
    for (name, args, error) in [
        ("write", json!({"data": {}}), "TypeError"),
        ("write", json!({"data": [{}]}), "TypeError"),
        ("write", json!({"data": [null]}), "TypeError"),
        (
            "write",
            json!({"data": [{"text/plain":"a"}, {"text/plain":"b"}]}),
            "NotAllowedError",
        ),
        (
            "write",
            json!({"data": [{"text/html":"a"}]}),
            "NotAllowedError",
        ),
        ("write", json!({"data": [{"text/plain":3}]}), "TypeError"),
        ("writeText", json!({"data": 3}), "TypeError"),
        ("write", json!({"data": [{"image/png":3}]}), "TypeError"),
        (
            "writeText",
            json!({"data": "x".repeat(1024 * 1024 + 1)}),
            "DataError",
        ),
        (
            "write",
            json!({"data": [{"image/png":{"base64":"AAAA"}}]}),
            "NotSupportedError",
        ),
        (
            "write",
            json!({"data": [{"image/png":"file:/other"}]}),
            "TypeError",
        ),
        (
            "write",
            json!({"data": [{"image/png":"app:/tmp/missing"}]}),
            "NotFoundError",
        ),
    ] {
        assert_eq!(
            prepare(name, &args, no_file).err().unwrap().0,
            error,
            "{name} {args}"
        );
    }
    let text = prepare("writeText", &json!({"data":"café 🦀"}), no_file).unwrap();
    assert_eq!(text.text.as_deref(), Some("café 🦀"));
    exact_windows::clipboard::write(text, true).unwrap();
    let empty = prepare("write", &json!({"data":[]}), no_file).unwrap();
    exact_windows::clipboard::write(empty, true).unwrap();
    crate::clipboard::PRIVATE.with(|b| assert_eq!(b.borrow().text.as_deref(), Some("café 🦀")));
}

#[test]
fn path_image_and_text_land_together_and_failed_preparation_leaves_them() {
    let dir = std::env::temp_dir().join(format!("exact-clipboard-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("image.png");
    {
        let file = std::fs::File::create(&path).unwrap();
        let mut encoder = png::Encoder::new(file, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[200, 100, 50, 128])
            .unwrap();
    }
    let open =
        |_: &str| std::fs::File::open(&path).map_err(|e| ("NotReadableError", e.to_string()));
    let args = json!({"data": [{"image/png":"app:/tmp/image.png", "text/plain":"image URL"}]});
    let both = prepare("write", &args, open).unwrap();
    assert_eq!(both.image.as_ref().unwrap().dimensions(), (1, 1));
    exact_windows::clipboard::write(both, true).unwrap();
    std::fs::write(&path, b"not a PNG").unwrap();
    assert_eq!(prepare("write", &args, open).err().unwrap().0, "DataError");
    crate::clipboard::PRIVATE.with(|b| {
        let b = b.borrow();
        assert_eq!(b.text.as_deref(), Some("image URL"));
        assert_eq!(b.image.as_ref().unwrap().dimensions(), (1, 1));
    });
    // A sparse file exercises the path bound without decoding or allocating it.
    std::fs::File::create(&path)
        .unwrap()
        .set_len(MOST_FILE + 1)
        .unwrap();
    assert_eq!(prepare("write", &args, open).err().unwrap().0, "DataError");
    // Path input is not held to the binding's 2 MiB in-memory limit.
    {
        let file = std::fs::File::create(&path).unwrap();
        let mut encoder = png::Encoder::new(file, 1024, 1024);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_compression(png::Compression::NoCompression);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&vec![255; 1024 * 1024 * 4])
            .unwrap();
    }
    assert!(std::fs::metadata(&path).unwrap().len() > 2 * 1024 * 1024);
    let large = prepare("write", &args, open).unwrap();
    assert_eq!(large.image.as_ref().unwrap().dimensions(), (1024, 1024));
    let shown = large.description(true);
    assert_eq!(shown["types"], json!(["text/plain", "image/png"]));
    assert_eq!(
        shown["platformTypes"],
        json!(["PNG", "CF_DIBV5", "CF_UNICODETEXT"])
    );
    assert_eq!(shown["image"]["sha256"].as_str().unwrap().len(), 64);
    assert_eq!(shown["pasteboard"], "memory");
    let directory =
        |_: &str| std::fs::File::open(&dir).map_err(|e| ("NotReadableError", e.to_string()));
    assert_eq!(
        prepare("write", &args, directory).err().unwrap().0,
        "NotReadableError"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn composed_package_dispatches_calls_and_refuses_non_app_origins() {
    use exact_painted::{package::Entry, presenter::PainterChoice, Presenter};
    let plan = contract::compile("component App\n  view\n    text \"Clipboard\"\n")
        .unwrap()
        .encode();
    let (mut p, error) = Presenter::boot_with(
        &plan,
        (),
        (100., 100.),
        1.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.serve_packages(vec![Entry::of::<Clipboard>("clipboard")]);
    for (body, expected) in [
        (
            json!({"extension":"clipboard", "send":"writeText", "origin":"tool", "args":{"data":"must not copy"}}),
            json!({"error":"NotAllowedError", "message":"person-required"}),
        ),
        (
            json!({"extension":"clipboard", "send":"writeText", "origin":"wake", "args":{"data":"must not copy"}}),
            json!({"error":"NotAllowedError", "message":"person-required"}),
        ),
        (
            json!({"extension":"clipboard", "send":"write", "args":{"data":[]}}),
            json!({}),
        ),
    ] {
        let (tx, rx) = std::sync::mpsc::channel();
        let reply = exact_runner::Reply::new(move |outcome| {
            if let exact_runner::Outcome::Response(r) = outcome {
                tx.send(serde_json::from_slice::<Json>(&r.body).unwrap())
                    .unwrap();
            }
        });
        p.host().runner().native_slot().handler().unwrap()(body.to_string().into_bytes(), reply);
        p.run_commands(|| ());
        assert_eq!(rx.try_recv().unwrap(), expected);
    }
    p.end_packages();
}

#[test]
fn headless_startup_installs_the_copy_text_adapter() {
    const CHILD: &str = "EXACT_CLIPBOARD_STARTUP_TEST";
    if std::env::var_os(CHILD).is_none() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "clipboard_package::tests::headless_startup_installs_the_copy_text_adapter",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("EXACT_AGENT", "1")
            .env("EXACT_PAINTER", "cpu")
            .env("EXACT_GPU_RENDER", "0")
            .env_remove("EXACT_DEVICE")
            .env_remove("EXACT_AGENT_BIND")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(
            b"{\"op\":\"tree\"}\n{\"op\":\"clock\",\"to\":250}\n{\"op\":\"clock\",\"to\":500}\n{\"op\":\"tree\"}\n{\"op\":\"quit\"}\n"
        ).unwrap();
        let out = child.wait_with_output().unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success(),
            "{stdout} {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(stdout.contains("\"ready\":true"), "{stdout}");
        assert!(stdout.contains("private-image"), "{stdout}");
        assert!(stdout.contains("replacement"), "{stdout}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!stderr.contains("copyText unsupported"), "{stderr}");
        return;
    }
    #[derive(Default)]
    struct Board;
    impl exact_runner::DataSource for Board {
        fn query(
            &mut self,
            _: &str,
            _: &[exact_runner::Value],
        ) -> Result<exact_runner::Value, exact_runner::DataError> {
            Ok(crate::clipboard::PRIVATE.with(|board| {
                let board = board.borrow();
                exact_runner::Value::record(vec![exact_runner::Value::str(
                    if board.image.is_some() {
                        "private-image"
                    } else {
                        board.text.as_deref().unwrap_or_default()
                    },
                )])
            }))
        }
    }
    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, 1, 1);
    encoder.set_color(png::ColorType::Rgba);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&[255, 0, 0, 128])
        .unwrap();
    exact_windows::clipboard::write(
        Item {
            image: Some(Image::decode(&png).unwrap()),
            text: None,
        },
        true,
    )
    .unwrap();
    let plan = contract::compile(
        r#"shape Board
  text: string
component App
  state tick = 0
  resource copied = board(tick) as shape Board
  action copy
    copyText("replacement")
    tick = tick + 1
  task timer mount
    every(250, copy)
  view
    text copied.text
"#,
    )
    .unwrap()
    .encode();
    let mut config = exact_painted::app::Config::from_env(&plan, "{}");
    assert!(config.agent);
    assert!(!config.windowed());
    assert_eq!(
        crate::clipboard::run_headless::<Board>(&mut config, std::time::Instant::now()),
        0
    );
    crate::clipboard::PRIVATE.with(|board| {
        assert!(board.borrow().image.is_none());
        assert!(board.borrow().text.is_none());
    });
}
