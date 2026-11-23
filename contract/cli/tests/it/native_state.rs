use crate::source::{boxed, NoData};
use exact_kernel::{Kernel, NodeType, PropId, StyleId};
use exact_runner::{Event, Runner};

#[test]
fn retired_press_paints_name_the_active_property() {
    for (retired, property) in [
        ("-exact-press-background-color", "background-color"),
        ("-exact-press-color", "color"),
    ] {
        for source in [
            format!("component App\n  view\n    button {retired}=\"#ff0000\"\n      text \"Go\"\n"),
            format!("style Feedback\n  {retired}=\"#ff0000\"\ncomponent App\n  view\n    button class=Feedback\n      text \"Go\"\n"),
        ] {
            let error = contract::compile(&source).unwrap_err();
            assert_eq!(error.id, "lower-unknown-attr", "{source}: {error}");
            assert!(error.message.contains(":active"), "{error}");
            assert!(error.message.contains(property), "{error}");
            assert!(error.message.contains("contract migrate"), "{error}");
            let migrated = contract_syntax::migrate::migrate(&source).unwrap();
            contract::compile(&migrated).unwrap_or_else(|error| panic!("{migrated}: {error}"));
        }
    }
}

#[test]
fn active_properties_keep_native_buttons_and_leave_their_base_face_native() {
    let mut failures = Vec::new();
    for appearance in ["", "appearance=\"auto\""] {
        for (name, target, id) in [
            (
                "background-color",
                "\"#1265df\"",
                StyleId::PressBackgroundColor,
            ),
            ("color", "\"#ffffff\"", StyleId::PressColor),
            ("opacity", "0.4", StyleId::PressOpacityTo),
            ("-exact-press-scale", "0.94", StyleId::PressScale),
        ] {
            let source = format!(
                "component App\n  state on = false\n  action toggle\n    on = not on\n  view\n    button {appearance} press=toggle testId=\"button\"\n      :active\n        {name} = (on ? {target} : none)\n      text \"Go\"\n"
            );
            let plan = match contract::compile(&source) {
                Ok(plan) => plan,
                Err(e) => {
                    failures.push(format!("{appearance} {name}: {e}"));
                    continue;
                }
            };
            let mut r = Runner::boot(
                plan,
                boxed(NoData),
                Kernel::with_monospace(),
                Default::default(),
                "/",
            )
            .unwrap();
            let key = r.kernel().find_by_test_id("button")[0];
            let node = r.kernel().node_by_key(key).unwrap();
            if node.node_type != NodeType::Control || node.props.str(PropId::Type) != Some("button")
            {
                failures.push(format!(
                    "{appearance} {name}: active styling replaced the native button"
                ));
                continue;
            }
            let view = node.id;
            let face = r.kernel().press_face(view).unwrap();
            assert!(
                !node.style.mask.has(id),
                "the conditional override starts absent"
            );
            r.dispatch(view, Event::Press).unwrap();
            let node = r.kernel().node_by_key(key).unwrap();
            assert_eq!(node.node_type, NodeType::Control);
            assert!(
                node.style.mask.has(id),
                "{name}: the target updates while the control stays native"
            );
            assert_eq!(
                r.kernel().press_face(view).unwrap(),
                face,
                "active styling leaves the native base face intact"
            );
            r.dispatch(view, Event::Press).unwrap();
            assert!(!r.kernel().node_by_key(key).unwrap().style.mask.has(id));
            assert_eq!(r.kernel().press_face(view).unwrap(), face);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
