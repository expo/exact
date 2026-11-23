//! A row's place in its list (`aria-posinset`, `aria-setsize`) and its kept recording.
use super::*;

/// A list row's wrapper given only a new place in the list (a row inserted
/// above it) draws as it did: its row is not recorded again. Another change
/// to the wrapper is.
#[test]
fn a_rows_place_in_the_list_keeps_its_recording() {
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &contract::compile(
            "component App\n  view\n    list virtualized=true estimated-item-height=50 width=200 height=400\n      for x in [0, 1, 2, 3] key=x\n        box height=50 width=200 testId=`r${x}`\n",
        )
        .unwrap()
        .encode(),
        (),
        (400., 500.),
        1.,
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/caltrain")),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let heard = Rc::new(RefCell::new(Heard::default()));
    p.brush.replace_backend(Box::new(Keeps {
        heard: heard.clone(),
        row: 0,
    }));
    let _ = p.frame();
    assert!(!heard.borrow().begun.is_empty(), "the rows are kept");
    let k = p.host.kernel();
    let wrapper = (k.node_by_key(k.find_by_test_id("r1")[0]).unwrap().parent).unwrap();
    let mut set = |prop, value| {
        heard.borrow_mut().begun.clear();
        let receipt = (p.host.runner_mut().kernel_mut())
            .apply(
                0,
                0,
                &[exact_kernel::Op::SetProp {
                    id: wrapper,
                    prop,
                    value,
                }],
            )
            .unwrap();
        p.host.row_dirty.commit(&receipt, 0.0);
        let _ = p.frame();
        heard.borrow().begun.len()
    };
    use exact_kernel::PropValue;
    assert_eq!(set(PropId::AccessibilityPosInSet, PropValue::Int(9)), 0);
    assert_eq!(set(PropId::AccessibilitySetSize, PropValue::Int(9)), 0);
    assert_eq!(
        set(PropId::AccessibilityLabel, PropValue::Str("r".into())),
        1
    );
}
