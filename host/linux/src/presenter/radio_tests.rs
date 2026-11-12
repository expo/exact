use super::*;
use exact_runner::Value;

const APP: &str = r#"component App
  state choice = "one"
  action choose(value: string)
    choice = value
  view
    column direction="GROUP_DIRECTION" width=480 height=400
      row role="radiogroup" testId="group"
        input type="radio" name="choice" value="one" checked=(choice == "one") change=choose direction="CHILD_DIRECTION" testId="one"
        input type="radio" name="choice" value="disabled" disabled=true testId="disabled"
        input type="radio" name="choice" value="two" checked=(choice == "two") change=choose direction="CHILD_DIRECTION" testId="two"
        input type="radio" name="choice" value="three" checked=(choice == "three") change=choose direction="CHILD_DIRECTION" testId="three"
"#;

fn id(p: &Presenter<()>, name: &str) -> ViewId {
    let kernel = p.host.kernel();
    kernel
        .node_by_key(kernel.find_by_test_id(name)[0])
        .unwrap()
        .id
}

fn arrows(group_direction: &str, child_direction: &str, forward: &str, backward: &str) {
    let source = APP
        .replace("GROUP_DIRECTION", group_direction)
        .replace("CHILD_DIRECTION", child_direction);
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(&source),
        (),
        (480., 400.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    assert!(p.set_focus(Some(id(&p, "one")), 0.).is_none());
    for (key, next) in [
        (forward, "two"),
        (forward, "three"),
        (forward, "one"),
        (backward, "three"),
        (backward, "two"),
        (backward, "one"),
        ("ArrowDown", "two"),
        ("ArrowUp", "one"),
        ("ArrowUp", "three"),
        ("ArrowDown", "one"),
    ] {
        p.key_down(key, 0.);
        assert_eq!(p.focus, Some(id(&p, next)), "{key}: expected {next}");
        assert_eq!(p.host.runner().slot("choice"), Some(&Value::str(next)));
        for member in ["one", "disabled", "two", "three"] {
            assert_eq!(p.radio_checked(id(&p, member)), member == next, "{member}");
        }
    }
}

#[test]
fn radio_arrows_follow_an_inherited_ltr_group_with_rtl_children() {
    arrows("ltr", "rtl", "ArrowRight", "ArrowLeft");
}

#[test]
fn radio_arrows_follow_an_inherited_rtl_group_with_ltr_children() {
    arrows("rtl", "ltr", "ArrowLeft", "ArrowRight");
}
