use super::*;
use crate::presenter::top::SurfacePainter;
use exact_runner::Value as StateValue;

struct Windows;
impl SurfacePainter for Windows {
    fn record(
        &mut self,
        _: (f32, f32),
        _: f32,
        _: &mut dyn FnMut(&mut dyn crate::paint::Backend),
    ) -> Vec<u32> {
        Vec::new()
    }
}

const APP: &str = r#"component App
  state opened = 0
  action open
    opened = opened + 1
  view
    view width=100 height=360
      canvas testId="world" position="absolute" inset=0
      view testId="target" position="absolute" top=10 left=10 width=80 height=40 contextmenu=open
        view testId="child" width=80 height=40
"#;

fn input_count(p: &Presenter<()>) -> u32 {
    unsafe { p.surfaces.abis[""].symbol::<unsafe extern "C" fn() -> u32>(b"test_input_count")() }
}

#[test]
fn android_agent_contextmenu_matches_a_long_press_on_a_handler_above_a_canvas() {
    let (mut p, probe) = boot(APP, &["world"]);
    p.serve_windows(Box::new(Windows));
    p.set_pointer_kind(crate::presenter::PointerKind::Touch);
    let target = find(&p, "target");
    let (x, y, w, h) = p.rect_of(target).unwrap();
    let point = (x + w / 2., y + h / 2.);
    p.reader_touch(0, point.0, point.1, 0., 0.).unwrap();
    p.reader_touch(4, point.0, point.1, 500., 500.).unwrap();
    p.reader_touch(1, point.0, point.1, 600., 600.).unwrap();
    assert_eq!(
        p.host.runner().slot("opened"),
        Some(&StateValue::Number(1.))
    );
    assert_eq!(p.long_presses(), 1);
    assert!(p.surface().is_none());

    for name in ["target", "child"] {
        let target = find(&p, name);
        let reply: Value = serde_json::from_str(&crate::agent::answer(
            &mut p,
            &format!(r#"{{"op":"tap","id":{target},"contextmenu":true}}"#),
        ))
        .unwrap();
        assert_eq!(reply["tapped"], target, "{reply}");
    }
    assert_eq!(
        p.host.runner().slot("opened"),
        Some(&StateValue::Number(3.))
    );
    assert_eq!(p.long_presses(), 3);
    assert!(p.surface().is_none());
    assert!(p.contact_position().is_none());
    assert_eq!(input_count(&p), 0, "the canvas receives no mouse input");
    done(p, probe);
}

#[test]
fn android_contextmenu_refuses_unavailable_targets_without_mouse_input() {
    for (from, to, at) in [
        ("contextmenu=open", "", None),
        ("contextmenu=open", "contextmenu=open disabled=true", None),
        ("contextmenu=open", "contextmenu=open visibility=\"hidden\"", None),
        ("left=10", "left=200", None),
        ("contextmenu=open", "contextmenu=open", Some((80., 20.))),
        ("contextmenu=open", "contextmenu=open", Some((f32::NAN, 20.))),
        (
            "        view testId=\"child\" width=80 height=40",
            "        view testId=\"child\" width=80 height=40\n      view position=\"absolute\" inset=0",
            None,
        ),
    ] {
        let (mut p, probe) = boot(&APP.replace(from, to), &["world"]);
        p.serve_windows(Box::new(Windows));
        let target = find(&p, "target");
        assert!(p.contextmenu(target, at).is_err(), "{from} -> {to}");
        assert_eq!(p.host.runner().slot("opened"), Some(&StateValue::Number(0.)));
        assert_eq!(p.long_presses(), 0);
        assert_eq!(input_count(&p), 0);
        done(p, probe);
    }
}

#[test]
fn desktop_agent_contextmenu_delivers_secondary_buttons_before_the_menu_handler() {
    let app = APP
        .replace(
            "  action open",
            "  state buttons = \"\"\n  action note(e: PointerEvent)\n    buttons = `${buttons}${e.button}:${e.buttons},`\n  action open",
        )
        .replace("contextmenu=open", "contextmenu=open pointerdown=note pointerup=note")
        .replace("opened = opened + 1", "opened = opened + 1\n    buttons = `${buttons}menu,`");
    for named in [false, true] {
        let source = if named {
            app.replace("contextmenu=open", "contextmenu=open contextPopover=\"menu\"")
                + "      column id=\"menu\" popover=\"auto\" role=\"menu\"\n        button \"Item\" press=open\n"
        } else {
            app.clone()
        };
        let (mut p, probe) = boot(&source, &["world"]);
        p.serve_overlays();
        let target = find(&p, "target");
        let reply: Value = serde_json::from_str(&crate::agent::answer(
            &mut p,
            &format!(r#"{{"op":"tap","id":{target},"contextmenu":true}}"#),
        ))
        .unwrap();
        assert_eq!(reply["tapped"], target, "{reply}");
        assert_eq!(
            p.host.runner().slot("opened"),
            Some(&StateValue::Number(1.))
        );
        assert_eq!(
            p.host.runner().slot("buttons"),
            Some(&StateValue::str("2:2,2:0,menu,"))
        );
        assert_eq!(p.surface().is_some(), named);
        assert!(p.contact_position().is_none());
        assert_eq!(input_count(&p), 0);
        done(p, probe);
    }
}
