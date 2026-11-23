use super::*;
use crate::presenter::{Material, Palette, MATERIAL_ROLES};
use exact_kernel::Dimension;

fn boot(source: &str) -> Presenter<()> {
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(source),
        (),
        (400., 600.),
        2.4375,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.frame();
    p
}

fn rect(p: &mut Presenter<()>, name: &str) -> Rect4 {
    p.rect_of(id(p, name)).unwrap()
}

#[test]
fn materials_grouped_item_uses_one_leading_gap() {
    if !material_process() {
        return;
    }
    let mut p = boot(
        r#"component App
  view
    list appearance="auto"
      section
        row
          image "symbol:person" testId="symbol"
          text "Account" testId="label"
        row
          view width=24 height=24 testId="box"
          text "Profile" testId="profile"
        column
          view width=20 height=20 testId="top"
          view width=20 height=20 testId="bottom"
"#,
    );
    let inline = [("symbol", "label"), ("box", "profile")].map(|(leading, label)| {
        let part = rect(&mut p, leading);
        rect(&mut p, label).0 - (part.0 + part.2)
    });
    let top = rect(&mut p, "top");
    let block = rect(&mut p, "bottom").1 - (top.1 + top.3);
    assert_eq!((inline, block), ([16.; 2], 8.));
}

#[test]
fn materials_grouped_item_height_follows_supporting_text() {
    if !material_process() {
        return;
    }
    let mut p = boot(
        r#"component App
  state detail: option<string> = none
  action show
    detail = some("Supporting text")
  view
    list appearance="auto"
      section
        column testId="single"
          text "Title"
          text "Hidden" display="none"
        column testId="double"
          text "Title"
          text "Supporting text"
        row testId="nested"
          image "symbol:person"
          column testId="lines"
            text "Title"
            match detail
              case some(s)
                text s
              case none
                text "" display="none"
        button press=show testId="show"
          text "Show"
"#,
    );
    let heights = ["single", "double", "nested"].map(|name| rect(&mut p, name).3);
    p.tap(id(&p, "show")).unwrap();
    assert_eq!((heights, rect(&mut p, "nested").3), ([56., 72., 56.], 72.));
    let k = p.host().kernel();
    for (name, padding) in [("double", 8.), ("nested", 8.), ("lines", 0.)] {
        let style = k.node(id(&p, name)).unwrap().style;
        assert_eq!(
            [&style.padding_top, &style.padding_bottom],
            [&Dimension::Points(padding); 2],
            "{name}"
        );
    }
}

#[test]
fn materials_grouped_item_destructive_toggle_keeps_its_height() {
    if !material_process() {
        return;
    }
    let mut p = boot(
        r#"component App
  state danger = false
  action flip
    danger = not danger
  view
    list appearance="auto"
      section
        button destructive=danger press=flip testId="remove"
          text "Remove"
        button destructive=danger press=flip testId="double"
          column
            text "Erase"
            text "Supporting text"
"#,
    );
    let row = id(&p, "remove");
    let heights = |p: &mut Presenter<()>| [p.rect_of(row).unwrap().3, rect(p, "double").3];
    assert_eq!(heights(&mut p), [56., 72.]);
    p.tap(row).unwrap();
    assert_eq!(id(&p, "remove"), row);
    assert_eq!(heights(&mut p), [56., 72.]);
}

#[test]
fn materials_grouped_lists_take_the_device_surface_and_keep_authored_borders() {
    if !material_process() {
        return;
    }
    let mut p = boot(
        r##"component App
  view
    row
      list appearance="auto" width=128 height=160 testId="inset"
        section
          text "Inset"
      list appearance="auto" listStyle="plain" width=128 height=160 testId="plain"
        section
          text "Plain"
      list appearance="auto" listStyle="grouped" width=128 height=160 testId="grouped"
        section
          text "Edged" testId="edged" border-bottom="2px solid #ff0000"
"##,
    );
    let mut light = [0xff000000; MATERIAL_ROLES];
    light[Material::Surface as usize] = 0xffab_cdef;
    p.set_palette(&Palette { light, dark: light });
    for name in ["inset", "plain", "grouped"] {
        let list = rect(&mut p, name);
        assert_eq!(
            pixel(&mut p, list.0 + 8., list.1 + 145.),
            [171, 205, 239, 255],
            "{name}"
        );
    }

    let k = p.host().kernel();
    let row = id(&p, "edged");
    let card = k.node(k.node(row).unwrap().parent.unwrap()).unwrap();
    assert_eq!(card.style.border_widths(), [0.; 4]);
    let row = rect(&mut p, "edged");
    assert_eq!(
        pixel(&mut p, row.0 + 8., row.1 + row.3 - 1.),
        [255, 0, 0, 255]
    );
}
