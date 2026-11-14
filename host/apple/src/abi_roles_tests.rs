//! Each session's role overrides, its own (LLP 1121 stage 5).
use super::*;

thread_local! {
    static RUNTIMES: std::cell::RefCell<crate::abi::registry::Registry<StorageModule>> =
        std::cell::RefCell::new(crate::abi::registry::Registry::default());
}

fn plan(canvas: &str) -> Vec<u8> {
    contract::compile(&format!(
        "theme\n  CanvasText = \"{canvas}\"\n\ncomponent App\n  view\n    text \"a\"\n"
    ))
    .unwrap()
    .encode()
}

/// Boot `canvas`'s plan on runtime `rt` (a new one when `None`), through the
/// export's path; its batch.
fn boot(rt: Option<u32>, canvas: &str) -> (u32, String) {
    let rt = rt.unwrap_or_else(|| RUNTIMES.with(|r| r.borrow_mut().create()));
    let batch = crate::abi::registry::with_runtime(
        &RUNTIMES,
        rt,
        false,
        |b, _| {
            let len = b.boot(
                &plan(canvas),
                StorageModule::default(),
                Hooks::none(),
                200.,
                200.,
            );
            String::from_utf8(b.output_bytes(len as usize).to_vec()).unwrap()
        },
        |_| String::new(),
    );
    (rt, batch)
}

fn fixed(text: &str) -> Option<exact_kernel::ColorValue> {
    Some(exact_kernel::ColorValue::Fixed(
        exact_kernel::Color::parse(text).unwrap(),
    ))
}

/// What `CanvasText` resolves to in a call to `rt`, through the setters'
/// path and the exports' (each binds its session's).
fn resolved(rt: u32) -> Option<exact_kernel::ColorValue> {
    let canvas_text = exact_kernel::style::roles::role("CanvasText").unwrap();
    let mut out = None;
    crate::abi::registry::with_entry(&RUNTIMES, rt, |e| {
        assert!(!e
            .bridge
            .host
            .as_ref()
            .unwrap()
            .runner()
            .journal()
            .any(|l| l.contains("role overrides")));
        out = exact_kernel::style::roles::overridden(canvas_text);
    });
    let export = crate::abi::registry::with_runtime(
        &RUNTIMES,
        rt,
        false,
        |_, _| exact_kernel::style::roles::overridden(canvas_text),
        |_| None,
    );
    assert_eq!(out, export);
    out
}

/// Two embedded sessions on one thread, each with its own role overrides:
/// each call to a runtime resolves its session's, both ways round, and no
/// session says another's replaced its own.
#[test]
fn two_sessions_on_one_thread_resolve_their_own_role_overrides() {
    let (first, _) = boot(None, "#010101");
    let (second, _) = boot(None, "#020202");
    for _ in 0..2 {
        assert_eq!(resolved(first), fixed("#010101"));
        assert_eq!(resolved(second), fixed("#020202"));
    }
    assert_eq!(resolved(second), fixed("#020202"));
    assert_eq!(resolved(first), fixed("#010101"));
}

/// A session that reboots with other overrides, inside the call that binds
/// its old ones, presents the new ones: its first batch's colours and role
/// table are the new plan's, and the other session keeps its own.
#[test]
fn a_reboot_with_other_overrides_re_presents_them() {
    let (first, batch) = boot(None, "#010101");
    assert!(batch.contains(r#""text_color":[1,1,1,255]"#), "{batch}");
    let (second, _) = boot(None, "#020202");
    let (_, batch) = boot(Some(first), "#030303");
    assert!(batch.contains(r#""text_color":[3,3,3,255]"#), "{batch}");
    assert!(
        batch.contains(r#"{"op":"roles","roles":{"CanvasText":[3,3,3,255]}}"#),
        "{batch}"
    );
    assert_eq!(resolved(first), fixed("#030303"));
    assert_eq!(resolved(second), fixed("#020202"));
}

/// Boot a plan whose theme sets `AccentColor` and whose one view paints
/// `image`; its batch.
fn boot_gradient(accent: &str, image: &str) -> String {
    let plan = contract::compile(&format!(
        "theme\n  AccentColor = \"{accent}\"\n\ncomponent App\n  view\n    view width=100 height=20 background-image=\"{image}\"\n"
    ))
    .unwrap()
    .encode();
    let rt = RUNTIMES.with(|r| r.borrow_mut().create());
    crate::abi::registry::with_runtime(
        &RUNTIMES,
        rt,
        false,
        |b, _| {
            let len = b.boot(&plan, StorageModule::default(), Hooks::none(), 200., 200.);
            String::from_utf8(b.output_bytes(len as usize).to_vec()).unwrap()
        },
        |_| String::new(),
    )
}

/// The string a batch carries in `field` (the first).
fn batch_string<'a>(batch: &'a str, field: &str) -> &'a str {
    let key = format!(r#""{field}":""#);
    let at = batch
        .find(&key)
        .unwrap_or_else(|| panic!("{field}: {batch}"))
        + key.len();
    &batch[at..at + batch[at..].find('"').unwrap()]
}

/// What the presenter resolves for `css` in a 100 × 20 box under `tint`.
#[allow(unsafe_code)] // The presenter's resolve, over buffers this test owns.
fn resolve_gradient(css: &str, tint: Option<[f32; 4]>) -> String {
    let mut out = vec![0u8; 16384];
    // SAFETY: the buffers are the lengths given; the tint is four floats.
    let n = unsafe {
        crate::gradient::resolve(
            css.as_ptr(),
            css.len(),
            [100., 20.],
            false,
            tint.as_ref().map_or(std::ptr::null(), |t| t.as_ptr()),
            out.as_mut_ptr(),
            out.len(),
        )
    };
    String::from_utf8(out[..n].to_vec()).unwrap()
}

/// A gradient the presenter resolves later, once it knows the box, carries
/// its session's overrides as their colours (a tint role's as the theme's
/// accent), and resolves with none of another session's.
#[test]
fn a_boxed_gradient_carries_its_sessions_accent() {
    let boot = |accent: &str, stop: &str| {
        boot_gradient(
            accent,
            &format!("linear-gradient(90deg, {stop} 0px, transparent 100px)"),
        )
    };
    let resolve = |batch: &str| resolve_gradient(batch_string(batch, "css"), None);
    let red = r#""stops":[0,255,0,0,255"#;
    let first = boot("#ff0000", "AccentColor");
    let link = boot("#ff0000", "LinkText");
    let second = boot("#0000ff", "AccentColor");
    assert!(second.contains("#0000ffff 0px"), "{second}");
    for batch in [&first, &link] {
        let resolved = resolve(batch);
        assert!(resolved.contains(red), "{batch}\n{resolved}");
    }
    // A role the session leaves alone resolves as the platform's, though
    // the session installed last (the second) replaces it.
    let css = "linear-gradient(90deg, AccentColor 0px, transparent 100px)";
    let resolved = resolve(&format!(r#""css":"{css}""#));
    assert!(
        !resolved.contains(r#""stops":[0,0,0,255,255"#),
        "{resolved}"
    );
}

/// The view's tint replaces a tint stop unclipped, in the interpolation the
/// session's colours chose: a Display P3 theme accent, undimmed, paints as
/// the CSS its batch carries beside the reference does.
#[test]
fn a_wide_theme_accent_keeps_its_colour_and_interpolation_through_the_views_tint() {
    let p3 = |text: &str| -> [f32; 4] {
        let Some(exact_color::Parsed::Wide(w)) = exact_color::parse(text) else {
            panic!("{text}")
        };
        let [r, g, b] = w.linear_srgb().map(exact_color::linear_to_srgb);
        [r as f32, g as f32, b as f32, w.alpha as f32]
    };
    let stops = |json: &str| -> Vec<f64> {
        let v: serde_json::Value = serde_json::from_str(json).unwrap();
        let list = v["stops"].as_array().unwrap_or_else(|| panic!("{json}"));
        list.iter().map(|n| n.as_f64().unwrap()).collect()
    };
    let same = |a: &[f64], b: &[f64], why: &str| {
        assert_eq!(a.len(), b.len(), "{why}: {a:?}\n{b:?}");
        for (x, y) in a.iter().zip(b) {
            assert!((x - y).abs() < 2e-3, "{why}: {a:?}\n{b:?}");
        }
    };
    let red = p3("color(display-p3 1 0 0)");
    assert!(red[0] > 1.0 && red[1] < 0.0, "outside sRGB: {red:?}");
    for image in [
        "linear-gradient(90deg, AccentColor 0px, AccentColor 100px)",
        "linear-gradient(90deg, AccentColor 0px, #00ff00 100px)",
    ] {
        let batch = boot_gradient("color(display-p3 1 0 0)", image);
        let session = stops(&resolve_gradient(batch_string(&batch, "css"), None));
        let tinted = stops(&resolve_gradient(batch_string(&batch, "tint"), Some(red)));
        assert!(
            session[1] > 1.0,
            "the session's ramp is unclipped: {session:?}"
        );
        same(&tinted, &session, image);
    }
    // A legacy accent keeps the legacy ramp; a wider tint than its theme's
    // (an embedder's) is unclipped in it.
    let image = "linear-gradient(90deg, AccentColor 0px, #00ff00 100px)";
    let batch = boot_gradient("#ff0000", image);
    let session = resolve_gradient(batch_string(&batch, "css"), None);
    let legacy = resolve_gradient(batch_string(&batch, "tint"), Some([1., 0., 0., 1.]));
    same(&stops(&legacy), &stops(&session), "legacy");
    assert!(!legacy.contains("srgb-linear"), "{legacy}");
    let wide = stops(&resolve_gradient(batch_string(&batch, "tint"), Some(red)));
    assert!(wide[1] > 1.0, "{wide:?}");
    // Each scheme's choice, where they differ.
    let batch = boot_gradient("light-dark(color(display-p3 1 0 0), #0000ff)", image);
    assert!(
        batch_string(&batch, "tint").contains(" in oklab,"),
        "{batch}"
    );
    assert!(
        !batch_string(&batch, "tintDark").contains(" in "),
        "{batch}"
    );
}

/// The binding core through the export's path (LLP 1121 D12, stage 5): each
/// runtime answers a token from its own design, a name its plan lacks is
/// `null`, and `inputs` is the same until something it read moves.
#[test]
fn the_binding_core_answers_each_runtime_from_its_own_design() {
    let (first, _) = boot(None, "#010101");
    let (second, _) = boot(None, "#020202");
    let ask = |rt: u32| {
        crate::abi::registry::with_runtime(
            &RUNTIMES,
            rt,
            false,
            |b, _| {
                let len = b.input_write(br#"{"view": 0, "names": ["CanvasText", "nonesuch"]}"#);
                let n = b.tamber(len) as usize;
                String::from_utf8(b.output_bytes(n).to_vec()).unwrap()
            },
            |_| String::new(),
        )
    };
    let (a, b) = (ask(first), ask(second));
    assert!(a.contains(r#""CanvasText":[1,1,1,255]"#), "{a}");
    assert!(b.contains(r#""CanvasText":[2,2,2,255]"#), "{b}");
    assert!(a.contains(r#""nonesuch":null"#), "{a}");
    assert_eq!(ask(first), a, "nothing moved");
    let refused = crate::abi::registry::with_runtime(
        &RUNTIMES,
        first,
        false,
        |b, _| {
            let len = b.input_write(br#"{"view": 4294967297, "names": ["CanvasText"]}"#);
            let n = b.tamber(len) as usize;
            String::from_utf8(b.output_bytes(n).to_vec()).unwrap()
        },
        |_| String::new(),
    );
    assert!(refused.contains("no live view id"), "{refused}");
}
