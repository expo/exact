//! The commands an app gives its own window. LLP 1069.010: a quit the app
//! holds leaves full screen and fades its window first. LLP 1133.025 D4: a
//! utility that keeps running puts its window away. `startViewTransition`
//! cross-fades the whole window into the action's commit.

use crate::source::{boxed, NoData, Source};
use exact_kernel::Kernel;
use exact_plan::Value;
use exact_runner::Runner;

fn boot(src: &str) -> Runner<Source> {
    let plan = contract::compile(src).unwrap_or_else(|e| panic!("{e}"));
    Runner::boot(
        plan,
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

/// `exitFullscreen()` and `setWindowOpacity(opacity)` reach the host in
/// order; a literal opacity outside 0–1 and any argument to
/// `exitFullscreen` are refused.
#[test]
fn a_held_quit_s_window_commands() {
    let src = "component App\n  action conceal(n: number)\n    exitFullscreen()\n    setWindowOpacity(0)\n    setWindowOpacity(n)\n  view\n    button press=conceal(1)\n      text \"Quit\"\n";
    let mut r = boot(src);
    r.act("conceal", vec![Value::Number(0.5)]).unwrap();
    let commands = r.take_commands();
    let named: Vec<(&str, Vec<Value>)> = commands
        .iter()
        .map(|c| (c.name.as_str(), c.args.clone()))
        .collect();
    assert_eq!(
        named,
        [
            ("exitFullscreen", vec![]),
            ("setWindowOpacity", vec![Value::Number(0.0)]),
            ("setWindowOpacity", vec![Value::Number(0.5)]),
        ]
    );
    for (bad, id) in [
        ("exitFullscreen(1)", "type-exit-fullscreen"),
        ("setWindowOpacity(1.5)", "type-window-opacity"),
        ("setWindowOpacity(-1)", "type-window-opacity"),
        ("setWindowOpacity(\"0\")", "type-window-opacity"),
        ("setWindowOpacity()", "type-window-opacity"),
    ] {
        let error = contract::compile(&src.replace("exitFullscreen()", bad)).unwrap_err();
        assert!(format!("{error:?}").contains(id), "{bad}: {error:?}");
    }
}

#[test]
fn a_window_takes_one_rectangle_and_an_optional_animation() {
    let src = r#"component App
  action dock(x: number)
    setBounds(DOMRect(x=x, y=12, width=480, height=302), true)
    setAspectRatio(16 / 9)
  view
    button press=dock(12)
      text "Dock"
"#;
    let mut r = boot(src);
    r.act("dock", vec![Value::Number(-320.)]).unwrap();
    let commands = r.take_commands();
    assert_eq!(commands.len(), 2);
    assert_eq!(commands[0].name, "setBounds");
    assert_eq!(
        commands[0].args,
        [
            Value::Record([-320., 12., 480., 302.].map(Value::Number).to_vec().into()),
            Value::Bool(true)
        ]
    );
    assert_eq!(commands[1].name, "setAspectRatio");
    assert_eq!(commands[1].args, [Value::Number(16. / 9.)]);
    for bad in [
        "setAspectRatio(-1)",
        "setAspectRatio(true)",
        "setBounds(12)",
        "setBounds(DOMRect(x=0, y=0, width=1, height=1), 1)",
    ] {
        assert!(
            contract::compile(&src.replace("setAspectRatio(16 / 9)", bad)).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn the_window_level_mouse_events_and_opacity_have_independent_arguments() {
    let src = r#"component App
  action overlay(flag: bool)
    setWindowAlwaysOnTop(flag)
    setWindowIgnoreMouseEvents(false)
    setWindowOpacity(0.5, 180)
  view
    button press=overlay(true)
      text "Overlay"
"#;
    let mut r = boot(src);
    r.act("overlay", vec![Value::Bool(true)]).unwrap();
    let commands = r.take_commands();
    assert_eq!(commands.len(), 3);
    assert_eq!(commands[0].args, [Value::Bool(true)]);
    assert_eq!(commands[1].args, [Value::Bool(false)]);
    assert_eq!(commands[2].args, [Value::Number(0.5), Value::Number(180.)]);
    for bad in [
        "setWindowAlwaysOnTop(1)",
        "setWindowIgnoreMouseEvents()",
        "setWindowOpacity(1, -1)",
        "setWindowOpacity(1, true)",
        "setWindowOpacity(1, 2, 3)",
    ] {
        assert!(
            contract::compile(&src.replace("setWindowOpacity(0.5, 180)", bad)).is_err(),
            "{bad}"
        );
    }
}

/// LLP 1133.025 D4: `hideWindow()` reaches the host, beside the window's
/// bare `focus()` that shows it again; an argument is refused.
#[test]
fn a_utility_hides_its_window_and_shows_it_again() {
    let src = "component App\n  action done\n    hideWindow()\n  action show\n    focus()\n  view\n    button press=done\n      text \"Done\"\n";
    let mut r = boot(src);
    r.act("done", vec![]).unwrap();
    r.act("show", vec![]).unwrap();
    let commands = r.take_commands();
    let named: Vec<(&str, usize)> = commands
        .iter()
        .map(|c| (c.name.as_str(), c.args.len()))
        .collect();
    assert_eq!(named, [("hideWindow", 0), ("focus", 0)]);
    let error = contract::compile(&src.replace("hideWindow()", "hideWindow(1)")).unwrap_err();
    assert!(
        format!("{error:?}").contains("type-hide-window"),
        "{error:?}"
    );
}

/// `startViewTransition()` reaches the host with the commit it fades into;
/// any argument is refused, as the web's takes no duration (LLP 1013.003 D2).
#[test]
fn a_switch_cross_fades_the_window_into_its_commit() {
    let src = "component App\n  state game = \"a\"\n  action pick(g: string)\n    startViewTransition()\n    game = g\n  view\n    button press=pick(\"b\")\n      text game\n";
    let mut r = boot(src);
    r.act("pick", vec![Value::from("b")]).unwrap();
    let commands = r.take_commands();
    let named: Vec<(&str, &[Value])> = commands
        .iter()
        .map(|c| (c.name.as_str(), &c.args[..]))
        .collect();
    assert_eq!(named, [("startViewTransition", &[][..])]);
    for bad in ["startViewTransition(0.3)", "startViewTransition(\"slow\")"] {
        let error = contract::compile(&src.replace("startViewTransition()", bad)).unwrap_err();
        assert!(
            format!("{error:?}").contains("type-view-transition"),
            "{bad}: {error:?}"
        );
    }
}
