//! Headless painted-host geometry beside host/web/tests/neutral-look.test.mjs.
use exact_painted::{presenter::PainterChoice, Presenter};

// The browser comparison lays out these cases with the shipped stylesheet.
const APP: &str = r##"component App
  view
    column width=480
      column navigationKey="page"
        header testId="page"
          text "Inbox" role="heading" aria-level=1 font-size="1.6875rem" line-height=1.2 testId="page-title"
      column navigationKey="normal"
        header testId="normal"
          text "Inbox" role="heading" aria-level=1 font-size="1.6875rem" testId="normal-title"
      column navigationKey="empty"
        header testId="empty"
          text "" role="heading" aria-level=1 font-size="1.6875rem" line-height=1.2 testId="empty-title"
      column navigationKey="space"
        header testId="space"
          text " " role="heading" aria-level=1 font-size="1.6875rem" line-height=1.2 testId="space-title"
      column navigationKey="small"
        header testId="small"
          text "Inbox" role="heading" aria-level=1 font-size=17 line-height=1.2 testId="small-title"
      column navigationKey="tight"
        header testId="tight"
          text "Inbox" role="heading" aria-level=1 font-size="1.6875rem" line-height=1 testId="tight-title"
      column navigationKey="button"
        header display="flex" align-items="center" gap=8 testId="button"
          text "Inbox" role="heading" aria-level=1 font-size="1.6875rem" line-height=1.2 testId="button-title"
          button appearance="none" width=60 height=24 padding=0 border="none" testId="button-item"
            text "Edit"
      column navigationKey="boxed"
        header testId="boxed"
          column
            text "Inbox" role="heading" aria-level=1 font-size="1.6875rem" line-height=1.2 testId="boxed-title"
      column navigationKey="own"
        header testId="own"
          text "Inbox" role="heading" aria-level=1 font-size="1.6875rem" line-height=1.2 margin=0 testId="own-title"
      column navigationKey="one-margin"
        header testId="one-margin"
          text "Inbox" role="heading" aria-level=1 font-size="1.6875rem" line-height=1.2 margin-top=0 testId="one-margin-title"
      column navigationKey="sized"
        header testId="sized"
          text "Inbox" role="heading" aria-level=1 font-size="1.6875rem" line-height=1.2 height=60 testId="sized-title"
      column navigationKey="short"
        header testId="short"
          text "Inbox" role="heading" aria-level=1 font-size="1.6875rem" line-height=1.2 height=20 overflow="hidden" testId="short-title"
      column navigationKey="clamp"
        header testId="clamp"
          text "Inbox of everything" role="heading" aria-level=1 font-size="1.6875rem" line-height=1.2 width=100 line-clamp=1 testId="clamp-title"
      column navigationKey="plain"
        header appearance="none" padding=0 min-height=0 border="none" testId="plain"
          text "Inbox" role="heading" aria-level=1 font-size="1.6875rem" line-height=1.2 testId="plain-title"
      column navigationKey="later"
        column height=0
        header testId="later"
          text "Inbox" role="heading" aria-level=1 font-size="1.6875rem" line-height=1.2 testId="later-title"
      column navigationKey="modal" navigationPresentation="modal"
        header testId="modal"
          text "Inbox" role="heading" aria-level=1 font-size="1.6875rem" line-height=1.2 testId="modal-title"
      button testId="open-dialog" commandfor="dialog-host" command="show-modal"
        text "Open dialog"
      dialog id="dialog-host" width=480 padding=0 border="none"
        column navigationKey="dialog"
          header display="flex" align-items="center" testId="dialog"
            text "Inbox" role="heading" aria-level=1 font-size="1.6875rem" line-height=1.2 testId="dialog-title"
            button appearance="none" width=60 height=24 padding=0 border="none"
              text "Edit"
      button testId="open-popover" popovertarget="popover-host"
        text "Open popover"
      column id="popover-host" popover="manual" width=480 padding=0 border="none"
        column navigationKey="popover"
          header display="flex" align-items="center" testId="popover"
            text "Inbox" role="heading" aria-level=1 font-size="1.6875rem" line-height=1.2 testId="popover-title"
            button appearance="none" width=60 height=24 padding=0 border="none"
              text "Edit"
"##;

#[test]
fn route_titles_keep_the_large_title_line() {
    std::env::set_var(
        "EXACT_FONTS",
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../scripts/fixtures/fonts/assets"
        ),
    );
    std::env::set_var("EXACT_FONT", "DejaVu Sans");
    exact_kernel::style::roles::set_column(exact_kernel::style::roles::Column::Neutral);
    let plan = contract::compile(APP).unwrap();
    let (mut p, error) = Presenter::boot_with(
        &plan.encode(),
        (),
        (480., 2400.),
        1.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.serve_overlays();
    let id = |p: &Presenter<()>, name: &str| {
        let k = p.host().kernel();
        k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
    };
    let names = [
        "page",
        "normal",
        "empty",
        "space",
        "small",
        "tight",
        "button",
        "boxed",
        "own",
        "one-margin",
        "sized",
        "short",
        "clamp",
        "plain",
        "later",
        "modal",
        "dialog",
        "popover",
    ];
    let mut layouts = Vec::new();
    for root in [16., 20., 32., 16.] {
        assert!(p.set_root_font_size(root).is_none());
        let mut boxes = serde_json::Map::new();
        for name in names {
            let surface = matches!(name, "dialog" | "popover");
            if surface {
                p.tap(id(&p, &format!("open-{name}"))).unwrap();
                p.run_commands(|| ());
                p.frame();
            }
            let (_, y, _, height) = p.rect_of(id(&p, name)).unwrap();
            let (_, ty, _, th) = p.rect_of(id(&p, &format!("{name}-title"))).unwrap();
            boxes.insert(name.into(), serde_json::json!([height, ty - y, th]));
            if matches!(
                name,
                "page"
                    | "empty"
                    | "space"
                    | "small"
                    | "tight"
                    | "button"
                    | "boxed"
                    | "short"
                    | "clamp"
            ) {
                let expected = root as f32 * 2.0625 * 1.2 + 13.;
                assert!(
                    (height - expected).abs() < 0.02,
                    "{name} root {root}: {height} != {expected}"
                );
            }
            if surface {
                let expected = root as f32 * 1.6875 * 1.2 + 13.;
                assert!(
                    (height - expected).abs() < 0.02,
                    "{name} root {root}: {height} != {expected}"
                );
                p.hardware_key("Escape", "Escape", true, false);
                p.hardware_key("Escape", "Escape", false, false);
                p.frame();
            }
        }
        p.frame();
        layouts.push(serde_json::json!({"root": root, "boxes": boxes}));
    }
    println!(
        "ROUTE_TITLE_LAYOUT {}",
        serde_json::to_string(&layouts).unwrap()
    );
}
