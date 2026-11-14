//! LLP 1084 D7: a grouped list's row spans its card with its insets inside
//! it, and the separator under it is decoration the kernel places
//! (`Kernel::grouped_separator`): drawn over the row's ground and under its
//! children, from its content edge to the card's trailing edge, not under
//! the card's last row, and not where the author wrote that edge, whose own
//! border is drawn instead.
use exact_kernel::{ColorValue, ViewId};
use exact_painted::presenter::PainterChoice;
use exact_painted::Presenter;

const APP: &str = r##"component App
  view
    column width=402 height=600 background-color="#f2f2f7"
      list appearance="auto" flex=1
        section
          row width="100%" testId="full"
            text "Full width" flex=1
            text "42"
          row testId="covered"
            view width="100%" height=52 background-color="#00ff00" testId="cover"
          row border-bottom-width=2 border-bottom-style="solid" border-bottom-color="#ff0000" testId="own"
            text "Own edge"
          row testId="last"
            text "Last"
"##;

fn id(p: &Presenter<()>, name: &str) -> ViewId {
    let k = p.host().kernel();
    k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
}

/// The pixel `(x, y)` into `name`'s box.
fn pixel(p: &mut Presenter<()>, name: &str, x: f32, y: f32) -> [u8; 3] {
    let (left, top, _, _) = p.rect_of(id(p, name)).unwrap();
    let shot = p.frame();
    let c = shot.pixel((left + x) as u32, (top + y) as u32).unwrap();
    [c.red(), c.green(), c.blue()]
}

#[test]
fn a_grouped_rows_separator_is_decoration_under_its_children() {
    let plan = contract::compile(APP).unwrap();
    let (mut p, error) = Presenter::boot_with(
        &plan.encode(),
        (),
        (402., 600.),
        1.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let k = p.host().kernel();
    let line = k.grouped_separator(id(&p, "full")).unwrap();
    assert!(matches!(line.color, ColorValue::Role(_)), "the platform's");
    let (_, _, width, height) = p.rect_of(id(&p, "full")).unwrap();
    assert_eq!(width, 370.0, "a full-width row is its card's width");
    let card = pixel(&mut p, "full", 4.0, height / 2.0);
    let foot = height - 1.0;
    assert_eq!(pixel(&mut p, "full", 8.0, foot), card, "not in the inset");
    // The separator's translucent ink over the card.
    let ink = pixel(&mut p, "full", 20.0, foot);
    assert_ne!(ink, card, "from the content edge");
    assert_eq!(
        pixel(&mut p, "full", 20.0, foot - 1.0),
        card,
        "one point high"
    );
    assert_eq!(
        pixel(&mut p, "full", width - 1.0, foot),
        ink,
        "to the card's trailing edge"
    );
    assert_eq!(
        pixel(&mut p, "covered", 200.0, foot),
        [0, 255, 0],
        "under the row's children"
    );
    let (_, _, _, own) = p.rect_of(id(&p, "own")).unwrap();
    assert_eq!(
        pixel(&mut p, "own", 200.0, own - 1.0),
        [255, 0, 0],
        "the author's edge"
    );
    assert_eq!(k_none(&p, "own"), None);
    let (_, _, _, last) = p.rect_of(id(&p, "last")).unwrap();
    assert_eq!(
        pixel(&mut p, "last", 200.0, last - 1.0),
        card,
        "none under the last row"
    );
}

fn k_none(p: &Presenter<()>, name: &str) -> Option<exact_kernel::GroupedSeparator> {
    p.host().kernel().grouped_separator(id(p, name))
}

#[test]
fn a_grouped_card_casts_its_sections_shadow_outside_its_rounded_clip() {
    let source = r##"component App
  view
    column width=402 height=600 background-color="#ffffff"
      list appearance="auto" background-color="#ffffff" flex=1 testId="list"
        section box-shadow="0 8px 0 #ff0000" testId="section"
          header
            text "Account"
          row testId="first"
            text "Profile"
          row testId="last"
            text "Notifications"
          footer
            text "A note"
"##;
    let plan = contract::compile(source).unwrap();
    let (mut p, error) = Presenter::boot_with(
        &plan.encode(),
        (),
        (402., 600.),
        1.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let (_, _, width, height) = p.rect_of(id(&p, "last")).unwrap();
    assert_eq!(
        pixel(&mut p, "last", width / 2., height + 3.),
        [255, 0, 0],
        "under the card, outside its overflow clip"
    );
    assert_eq!(
        pixel(&mut p, "last", 1., height + 3.),
        [255, 255, 255],
        "the rounded corner cuts away the shadow"
    );
    let (_, _, section_width, section_height) = p.rect_of(id(&p, "section")).unwrap();
    assert_eq!(
        pixel(&mut p, "section", section_width / 2., section_height + 3.),
        [255, 255, 255],
        "no full-width section shadow"
    );
}

#[test]
fn a_grouped_cards_shadow_transition_paints_an_intermediate_frame() {
    for transition in [
        "box-shadow 1s linear, opacity 2s linear",
        "all 1s linear, opacity 2s linear",
    ] {
        let source = format!(
            r##"component App
  state on = false
  action flip
    on = not on
  view
    column width=402 height=600 background-color="#ffffff"
      button testId="flip" press=flip
        text "Flip"
      list appearance="auto" background-color="#ffffff" flex=1
        section box-shadow=(on ? "0 8px 0 #ff0000" : "0 8px 0 #ffffff") transition="{transition}" opacity=(on ? 0.5 : 1) testId="section"
          row testId="last"
            text "Profile"
"##
        );
        let plan = contract::compile(&source).unwrap();
        let (mut p, error) = Presenter::boot_with(
            &plan.encode(),
            (),
            (402., 600.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let (_, _, width, height) = p.rect_of(id(&p, "last")).unwrap();
        let before = pixel(&mut p, "last", width / 2., height + 3.);
        p.tap(id(&p, "flip")).unwrap();
        p.tick(500.);
        let halfway = pixel(&mut p, "last", width / 2., height + 3.);
        assert!(
            halfway[1] > 130 && halfway[1] < 200,
            "{transition}: halfway {halfway:?}, before {before:?}"
        );
        p.tick(2000.);
        let end = pixel(&mut p, "last", width / 2., height + 3.);
        assert!(
            end[1] < halfway[1],
            "{transition}: end {end:?}, halfway {halfway:?}"
        );
    }
}

#[test]
fn a_grouped_cards_hover_shadow_follows_its_section_header() {
    for disabled in [false, true] {
        let source = format!(
            r##"component App
  view
    column width=402 height=600 background-color="#ffffff"
      list appearance="auto" background-color="#ffffff" flex=1
        section disabled={disabled} testId="section"
          :hover
            box-shadow="0 8px 0 #ff0000"
          header testId="header"
            text "Account"
          row testId="last"
            text "Profile"
          footer
            text "A note"
"##
        );
        let (mut p, error) = Presenter::boot_with(
            &contract::compile(&source).unwrap().encode(),
            (),
            (402., 600.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let (_, _, width, height) = p.rect_of(id(&p, "last")).unwrap();
        let (x, y, w, h) = p.rect_of(id(&p, "header")).unwrap();
        p.pointer_move(x + w / 2., y + h / 2., 0.).unwrap();
        assert_eq!(
            pixel(&mut p, "last", width / 2., height + 3.),
            if disabled {
                [255, 255, 255]
            } else {
                [255, 0, 0]
            }
        );
        let (_, _, w, h) = p.rect_of(id(&p, "section")).unwrap();
        assert_eq!(pixel(&mut p, "section", w / 2., h + 3.), [255, 255, 255]);
        p.pointer_move(401., 599., 1.).unwrap();
        assert_eq!(
            pixel(&mut p, "last", width / 2., height + 3.),
            [255, 255, 255]
        );
    }
}
