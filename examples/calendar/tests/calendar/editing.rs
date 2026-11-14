use super::*;

#[test]
fn calendar_artwork_and_controls_clear_the_screen_insets_once() {
    let root = Root::new();
    let mut app = App::open(&root);
    app.runner
        .kernel_mut()
        .set_env(Env::new(62.0, 0.0, 34.0, 0.0))
        .unwrap();
    app.resize(402.0, 874.0);
    let route = app
        .runner
        .kernel()
        .node_by_key(app.key("calendar-main"))
        .unwrap()
        .parent
        .unwrap();
    app.runner
        .kernel_mut()
        .set_host_cover(
            route,
            Some(exact_kernel::HostCover::Edges([62.0, 0.0, 34.0, 0.0])),
        )
        .unwrap();
    app.layout();
    assert_eq!(app.frame("calendar-header").y, 62.0);
    let add = app.frame("new-item");
    assert_eq!(add.y + add.height, 874.0 - 34.0 - 24.0);
    app.press("theme-button");
    app.press("theme-1");
    app.finish_motion();
    assert_eq!(app.frame("theme-wallpaper").y, 0.0);
    app.resize(1180.0, 800.0);
    assert_eq!(app.frame("calendar-header").y, 62.0);
    assert_eq!(app.frame("inspector").y, 0.0);
}

#[test]
fn editor_kind_choices_stay_in_the_body_below_the_native_bar() {
    let root = Root::new();
    let mut app = App::open(&root);
    app.press("new-item");
    let kernel = app.runner.kernel();
    let route = kernel.node_by_key(app.key("schedule-editor")).unwrap().id;
    let bar = kernel
        .header_shape(route, Some("cancel-editor"), false, &|id| {
            ["kind-event", "kind-plan", "kind-todo"]
                .iter()
                .any(|name| kernel.node_by_key(app.key(name)).unwrap().id == id)
        })
        .unwrap();
    assert!(
        bar.items.is_empty(),
        "kind choices must not become bar buttons: {bar:?}"
    );
    assert_eq!(
        bar.heading,
        Some(kernel.node_by_key(app.key("editor-title")).unwrap().id)
    );
    assert!(app.frame("kind-tabs").y >= app.frame("editor-fields").y);
}

#[test]
fn new_opens_a_modal_route_that_switches_kinds_and_saves_on_enter() {
    let root = Root::new();
    let mut app = App::open(&root);
    app.press("date-2026-09-2026-09-25");
    app.press("new-item");
    assert_eq!(app.state("editorOpen"), &Value::Bool(true));
    assert_eq!(app.text("editor-title"), "New Event");
    assert_eq!(app.derived("startDate").as_str(), Some("2026-09-25"));
    assert_eq!(
        app.runner
            .kernel()
            .node_by_key(app.key("schedule-editor"))
            .unwrap()
            .props
            .str(PropId::NavigationPresentation),
        Some("modal"),
        "the editor is presented by the host"
    );
    assert_eq!(
        app.runner
            .kernel()
            .node_by_key(app.key("schedule-editor"))
            .unwrap()
            .props
            .str(PropId::NavigationKey),
        Some("editor")
    );

    app.press("kind-todo");
    assert!(app.mounted("todo-editor"));
    assert!(!app.mounted("end-date"));
    assert_eq!(app.text("editor-title"), "New Todo");
    app.press("kind-plan");
    assert_eq!(app.state("editorKind").as_str(), Some("plan"));
    app.input("schedule-title", "Offsite");
    app.input("end-date", "2026-09-27");
    app.event("schedule-title", Event::Submit);
    app.finish_motion();
    assert_eq!(app.state("editorOpen"), &Value::Bool(false));
    let id = app.agenda_id("Offsite");
    let _ = bar_handle(&app, "2026-09", &id);
    drop(app);

    let mut app = App::open(&root);
    app.press("date-2026-09-2026-09-26");
    app.press(&format!("agenda-item-{id}"));
    app.finish_motion();
    assert_eq!(app.text("editor-title"), "Edit Plan");
    assert!(!app.mounted("kind-tabs"), "an item keeps its kind");
    assert_eq!(app.derived("endDate").as_str(), Some("2026-09-27"));
}

#[test]
fn escape_and_cancel_close_the_editor_without_writing() {
    let root = Root::new();
    let mut app = App::open(&root);
    let revision = app.derived("revision").clone();
    app.press("date-2026-09-2026-09-21");
    app.event("date-2026-09-2026-09-21", Event::Dblclick);
    assert_eq!(app.state("editorOpen"), &Value::Bool(true));
    assert_eq!(app.derived("startDate").as_str(), Some("2026-09-21"));
    assert_eq!(app.derived("day"), &day(20));
    app.input("schedule-title", "Unsaved");
    assert_eq!(app.state("editorOpen"), &Value::Bool(true));
    assert_eq!(app.derived("title").as_str(), Some("Unsaved"));
    app.key_press("schedule-title", "Enter");
    app.key_press("schedule-title", "Escape");
    assert_eq!(app.state("editorOpen"), &Value::Bool(false));
    app.finish_motion();
    assert_eq!(app.state("editorOpen"), &Value::Bool(false));
    assert!(!app.mounted("schedule-editor"));

    app.event("date-2026-09-2026-09-21", Event::Dblclick);
    app.press("cancel-editor");
    app.finish_motion();
    assert_eq!(app.state("editorOpen"), &Value::Bool(false));
    assert_eq!(app.derived("revision"), &revision);
}

#[test]
fn the_context_menu_edits_toggles_and_asks_before_deleting() {
    check_context_menu(false);
}

#[test]
fn compact_grid_context_menu_edits_toggles_and_asks_before_deleting() {
    check_context_menu(true);
}

fn check_context_menu(compact: bool) {
    let root = Root::new();
    let mut app = App::open(&root);
    app.press("date-2026-09-2026-09-01");
    let id = app.agenda_id("Check passport");
    let completed = |app: &App| {
        items(&app.agenda()[3])
            .iter()
            .find(|item| fields(item)[0].as_str() == Some(&*id))
            .map(|item| fields(item)[13].clone())
            .unwrap()
    };
    let before = completed(&app);
    let handle = if compact {
        app.resize(402.0, 874.0);
        bar_handle(&app, "2026-09", &id)
    } else {
        format!("agenda-item-{id}")
    };
    app.event(&handle, Event::Message("toggle".into()));
    assert_ne!(completed(&app), before);
    if compact {
        app.event(&handle, Event::Message("toggle".into()));
    } else {
        app.press(&format!("todo-check-{id}"));
    }
    assert_eq!(completed(&app), before);

    app.event(&handle, Event::Message("edit".into()));
    assert_eq!(app.state("editorOpen"), &Value::Bool(true));
    assert_eq!(app.state("editId").as_str(), Some(&*id));
    assert_eq!(app.text("editor-title"), "Edit Todo");
    assert_eq!(app.state("confirmDelete"), &Value::Bool(false));
    app.finish_motion();
    assert_eq!(
        app.state("titleFocusPending"),
        &Value::Bool(false),
        "the title is focused once its notes arrive"
    );
    app.press("cancel-editor");
    app.finish_motion();

    app.event(&handle, Event::Message("delete".into()));
    assert_eq!(app.state("editorOpen"), &Value::Bool(true));
    assert_eq!(app.state("confirmDelete"), &Value::Bool(true));
    assert!(app.mounted("confirm-delete"));
    assert!(
        app.mounted("cancel-editor"),
        "the sheet can still be dismissed"
    );
    app.press("keep-schedule");
    assert_eq!(app.state("confirmDelete"), &Value::Bool(false));
    assert!(app.mounted("cancel-editor"));
    app.finish_motion();
    app.press("delete-schedule");
    app.press("confirm-delete");
    app.finish_motion();
    assert_eq!(app.state("editorOpen"), &Value::Bool(false));
    assert!(!app.agenda_ids().contains(&id));
    drop(app);

    let mut app = App::open(&root);
    app.press("date-2026-09-2026-09-01");
    assert!(!app.agenda_ids().contains(&id));
}

#[test]
fn a_failed_save_keeps_the_draft_and_its_retry_writes_once() {
    let root = Root::new();
    let mut app = App::open(&root);
    app.press("date-2026-09-2026-09-21");
    app.press("new-item");
    app.input("schedule-title", "Retry me");
    app.fail_write_reply_after = Some(1);
    app.press("save-schedule");
    app.finish_motion();
    assert_eq!(app.state("editorOpen"), &Value::Bool(true));
    assert!(app.mounted("editor-error"));
    assert_eq!(app.derived("title").as_str(), Some("Retry me"));
    let token = app.state("retryToken").clone();
    assert_ne!(token, Value::str(""));
    app.press("save-schedule");
    app.finish_motion();
    assert_eq!(app.state("editorOpen"), &Value::Bool(false));
    let saved = items(&app.agenda()[3])
        .iter()
        .filter(|item| fields(item)[1].as_str() == Some("Retry me"))
        .count();
    assert_eq!(saved, 1);
}

#[test]
fn the_theme_popover_keeps_its_choice_across_restart() {
    let root = Root::new();
    let mut app = App::open(&root);
    app.press("theme-button");
    assert_eq!(app.state("themeOpen"), &Value::Bool(true));
    assert_eq!(
        app.runner
            .kernel()
            .node_by_key(app.key("theme-popup"))
            .unwrap()
            .props
            .str(PropId::Popover),
        Some("auto")
    );
    app.press("theme-popup");
    assert_eq!(
        app.state("themeOpen"),
        &Value::Bool(true),
        "its own surface keeps it"
    );
    app.press("close-themes");
    app.finish_motion();
    assert_eq!(app.state("themeOpen"), &Value::Bool(false));

    app.press("theme-button");
    app.press("theme-3");
    app.finish_motion();
    assert_eq!(app.state("themeOpen"), &Value::Bool(false));
    assert_eq!(app.runner.resource("theme").unwrap(), &Value::Number(3.0));
    assert!(app.mounted("theme-wallpaper"));
    drop(app);

    let mut app = App::open(&root);
    assert_eq!(app.runner.resource("theme").unwrap(), &Value::Number(3.0));
    app.press("theme-button");
    app.press("close-themes");
    app.finish_motion();
    assert_eq!(app.state("themeOpen"), &Value::Bool(false));
}

#[test]
fn the_san_francisco_theme_is_offered_and_survives_restart() {
    let root = Root::new();
    let mut app = App::open(&root);
    app.press("theme-button");
    assert!(app.mounted("theme-6"));
    app.press("theme-6");
    app.finish_motion();
    assert_eq!(app.state("themeOpen"), &Value::Bool(false));
    assert_eq!(app.runner.resource("theme").unwrap(), &Value::Number(6.0));
    assert!(app.mounted("theme-wallpaper"));
    drop(app);

    let app = App::open(&root);
    assert_eq!(app.runner.resource("theme").unwrap(), &Value::Number(6.0));
    assert!(app.mounted("theme-wallpaper"));
}

#[test]
fn a_hovered_row_shows_a_delete_button_that_deletes_at_once() {
    let root = Root::new();
    let mut app = App::open(&root);
    app.press("date-2026-09-2026-09-01");
    let id = app.agenda_id("City workshop");
    let delete = format!("delete-{id}");
    assert!(!app.mounted(&delete), "the button shows only on hover");
    app.event(&format!("agenda-row-{id}"), Event::Hover(true));
    assert!(app.mounted(&delete));
    let button = app.frame(&delete);
    let row = app.frame(&format!("agenda-row-{id}"));
    assert!(button.x + button.width <= row.x + row.width && button.x > row.x + row.width / 2.0);
    app.press(&delete);
    assert_eq!(
        app.state("editorOpen"),
        &Value::Bool(false),
        "no dialog asks first"
    );
    assert!(!app.agenda_ids().contains(&id));
    drop(app);

    let app = App::open(&root);
    assert!(!app.agenda_ids().contains(&id));
}

#[test]
fn a_trackpad_swipe_over_the_month_turns_one_page_unless_a_dialog_is_open() {
    let root = Root::new();
    let mut app = App::open(&root);
    app.event("calendar-input", Event::Message("swipe:next".into()));
    assert_eq!(app.derived("month"), &Value::Number(SEPTEMBER + 1.0));
    assert_eq!(app.text("month-label"), "October");
    app.event("calendar-input", Event::Message("swipe:previous".into()));
    app.event("calendar-input", Event::Message("swipe:previous".into()));
    assert_eq!(app.text("month-label"), "August");
    app.press("new-item");
    app.event("calendar-input", Event::Message("swipe:next".into()));
    assert_eq!(
        app.text("month-label"),
        "August",
        "a swipe behind the dialog turns nothing"
    );
}

#[test]
fn compact_picker_and_editor_preserve_a_draft_across_the_breakpoint() {
    let root = Root::new();
    let mut app = App::open(&root);
    app.resize(402.0, 874.0);
    assert_eq!(app.derived("compact"), &Value::Bool(true));
    assert!(!app.mounted("inspector"));
    app.press("new-item");
    assert_eq!(
        app.runner
            .kernel()
            .node_by_key(app.key("type-picker"))
            .unwrap()
            .props
            .str(PropId::Popover),
        Some("auto")
    );
    app.press("add-plan");
    app.input("schedule-title", "Shared draft");
    app.input("end-date", "2026-09-04");
    app.resize(860.0, 800.0);
    assert_eq!(app.derived("compact"), &Value::Bool(false));
    assert!(app.mounted("inspector"));
    assert_eq!(app.derived("title").as_str(), Some("Shared draft"));
    assert_eq!(app.derived("endDate").as_str(), Some("2026-09-04"));
    app.resize(859.0, 800.0);
    assert!(!app.mounted("inspector"));
    assert_eq!(app.derived("title").as_str(), Some("Shared draft"));
    app.press("save-schedule");
    app.finish_motion();
    assert!(!app.mounted("schedule-editor"));
    let id = app.agenda_id("Shared draft");
    drop(app);
    let mut app = App::open(&root);
    app.resize(402.0, 874.0);
    app.press("date-2026-09-2026-09-01");
    assert!(app.mounted("date-popup"));
    app.press(&format!("agenda-item-{id}"));
    app.finish_motion();
    assert_eq!(app.derived("title").as_str(), Some("Shared draft"));
}

#[test]
fn compact_date_sheet_and_all_seven_themes_use_the_shared_store() {
    let root = Root::new();
    let mut app = App::open(&root);
    app.resize(402.0, 874.0);
    app.press("date-2026-09-2026-09-16");
    assert!(app.mounted("date-popup"));
    app.press("sticker-pick-coffee");
    assert!(app.mounted("sticker-agenda-2026-09-16"));
    app.press("close-date");
    for theme in 0..=6 {
        app.press("theme-button");
        app.press(&format!("theme-{theme}"));
        app.finish_motion();
        assert_eq!(
            app.runner.resource("theme").unwrap(),
            &Value::Number(theme as f64)
        );
    }
    app.resize(1180.0, 800.0);
    assert!(app.mounted("sticker-agenda-2026-09-16"));
}

#[test]
fn compact_gestures_page_snap_dismiss_and_cancel_on_resize() {
    let root = Root::new();
    let mut app = App::open(&root);
    app.resize(402.0, 874.0);
    app.event("month-2026-09", Event::Pan(-80.0, 0.0));
    app.event("month-2026-09", Event::PanRelease(0.0, 0.0));
    app.finish_motion();
    assert!(app.mounted("month-scroll-2026-10"));
    app.press("date-2026-10-2026-10-16");
    app.event("date-sheet-handle", Event::Pan(0.0, -80.0));
    app.event("date-sheet-handle", Event::PanRelease(0.0, 0.0));
    assert_eq!(app.state("sheetSnap"), &Value::Number(2.0));
    app.event("date-sheet-handle", Event::Pan(0.0, 140.0));
    app.event("date-sheet-handle", Event::PanRelease(0.0, 0.0));
    assert!(!app.mounted("date-popup"));
    app.event("month-2026-10", Event::Pan(60.0, 0.0));
    app.resize(1180.0, 800.0);
    assert_eq!(app.state("monthPan"), &Value::Number(0.0));
    assert!(app.mounted("month-scroll-2026-10"));
}

#[test]
fn compact_event_segments_select_the_date_under_each_segment() {
    let root = Root::new();
    let mut app = App::open(&root);
    let id = app.agenda_id("City workshop");
    app.resize(402.0, 874.0);
    let first = bar_handle(&app, "2026-09", &id);
    let second = first.replace("-2026-09-01", "-2026-09-02");
    for (handle, date, expected) in [
        (first, "2026-09-01", day(0)),
        (second, "2026-09-02", day(1)),
    ] {
        let hit = app.frame(&handle);
        let cell = app.frame(&format!("date-2026-09-{date}"));
        assert!(hit.x >= cell.x && hit.x + hit.width <= cell.x + cell.width + 0.01);
        app.press(&handle);
        assert_eq!(app.derived("day"), &expected);
        assert!(app.mounted("date-popup"));
        assert!(app.agenda_ids().contains(&id));
        app.press("close-date");
    }
}

#[test]
fn compact_sheet_keeps_agenda_height_at_small_snaps_and_short_viewports() {
    let root = Root::new();
    let mut app = App::open(&root);
    for height in [874.0, 480.0] {
        app.resize(402.0, height);
        app.press("date-2026-09-2026-09-01");
        for collapsed in [false, true] {
            if collapsed {
                app.event("date-sheet-handle", Event::Pan(0.0, 85.0));
                app.event("date-sheet-handle", Event::PanRelease(0.0, 0.0));
                assert_eq!(app.state("sheetSnap"), &Value::Number(0.0));
            }
            let content = app.frame("date-content");
            let sheet = app.frame("date-popup");
            assert!(content.height > 0.0 && content.height < sheet.height);
            assert!(content.y + content.height <= sheet.y + sheet.height + 0.01);
            assert!(app.frame("agenda-list").height > 30.0);
            assert!(app.frame("sticker-pick-picnic").height > 0.0);
        }
        // Restore the initial snap before trying the next viewport.
        app.event("date-sheet-handle", Event::Pan(0.0, -85.0));
        app.event("date-sheet-handle", Event::PanRelease(0.0, 0.0));
        app.press("close-date");
    }
}
