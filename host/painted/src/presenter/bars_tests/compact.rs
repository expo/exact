use super::*;

#[test]
fn authored_sheet_header_height_agrees_with_projected_bar_and_reserved_room() {
    let source = SHELF.replace(
        "header display=\"flex\"",
        "header height=56 display=\"flex\"",
    );
    let (mut presenter, _) = barred(&source);
    tap(&mut presenter, "add-book").unwrap();
    presenter.tick(presenter.host().now() + 1000.);
    let _ = presenter.frame();
    let kernel = presenter.host().kernel();
    let form = kernel
        .node_by_key(kernel.find_by_id("add-form")[0])
        .unwrap()
        .id;
    assert_eq!(kernel.local_insets(form).unwrap()[0], SHEET_HANDLE + 56.0);
    assert_eq!(presenter.bars.sheet_top.as_ref().unwrap().1.frame[3], 56.0);
    assert_eq!(presenter.bars().0.unwrap().frame[3], 52.0 + top_height());
    let cancel = presenter.bar_box(id(&presenter, "add-cancel")).unwrap();
    assert_eq!((cancel.2, cancel.3), (48.0, 48.0));
    assert_eq!(presenter.set_text_scale(2.5), None);
    let _ = presenter.frame();
    assert_eq!(
        presenter.host().kernel().local_insets(form).unwrap()[0],
        SHEET_HANDLE + 70.0
    );
    assert_eq!(presenter.bars.sheet_top.as_ref().unwrap().1.frame[3], 70.0);
    let cancel = presenter.bar_box(id(&presenter, "add-cancel")).unwrap();
    let page = presenter.chrome().frame;
    let x = cancel.0 + cancel.2 / 2.0 - page[0];
    let y = cancel.1 + cancel.3 - 1.0 - page[1];
    let now = presenter.host().now();
    assert!(presenter.pointer_down(x, y, now).unwrap());
    assert!(presenter.pointer_up(x, y, now + 20.0).unwrap());
    presenter.run_commands(|| Nothing);
    assert!(presenter.top_sheet().is_none());
}

#[test]
fn sheet_bar_defaults_and_minimum_touch_target() {
    for (height, expected) in [
        ("", top_height()),
        (" height=0", top_height()),
        (" height=32", 48.0),
    ] {
        let source = SHELF.replace(
            "header display=\"flex\"",
            &format!("header{height} display=\"flex\""),
        );
        let (mut presenter, _) = barred(&source);
        tap(&mut presenter, "add-book").unwrap();
        presenter.tick(presenter.host().now() + 1000.0);
        let _ = presenter.frame();
        let kernel = presenter.host().kernel();
        let form = kernel
            .node_by_key(kernel.find_by_id("add-form")[0])
            .unwrap()
            .id;
        assert_eq!(
            kernel.local_insets(form).unwrap()[0],
            SHEET_HANDLE + expected,
            "{height}"
        );
        assert_eq!(
            presenter.bars.sheet_top.as_ref().unwrap().1.frame[3],
            expected,
            "{height}"
        );
    }
}
