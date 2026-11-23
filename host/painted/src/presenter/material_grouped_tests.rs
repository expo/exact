use super::*;
use crate::presenter::{Material, Palette, MATERIAL_ROLES};
use exact_kernel::{Dimension, Display};

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
fn materials_grouped_item_height_uses_a_static_subtitle() {
    if !material_process() {
        return;
    }
    let mut p = boot(
        r#"style Hidden
  display="none"

component App
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
            text "Supporting text"
        row testId="leading"
          input type="radio"
          column
            text "Title"
            text "Supporting text"
        row testId="trailing"
          column
            text "Title"
            text "Supporting text"
          input type="radio"
        row testId="horizontal"
          column flex-direction="row"
            text "Title"
            text "Beside title"
        column testId="hidden"
          text "Title"
          text "Supporting text" class=Hidden
"#,
    );
    let expected = [
        ("single", 56.),
        ("double", 72.),
        ("nested", 72.),
        ("leading", 72.),
        ("trailing", 72.),
        ("horizontal", 56.),
        ("hidden", 56.),
    ];
    assert_eq!(
        expected.map(|(name, _)| (name, rect(&mut p, name).3)),
        expected
    );
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
fn materials_grouped_item_with_conditional_text_keeps_the_one_line_minimum() {
    if !material_process() {
        return;
    }
    let mut p = boot(
        r#"component App
  state show = true
  state detail: option<string> = some("Supporting text")
  action toggle
    let next = not show
    show = next
    detail = next ? some("Supporting text") : none
  view
    list appearance="auto"
      section
        button press=toggle testId="toggle"
          text "Toggle"
        column testId="owned"
          text "Title"
          if show
            Detail()
        row testId="match"
          image "symbol:person"
          column
            text "Title"
            match detail
              case some(s)
                text s
              case none
                text "" display="none"
        column testId="display"
          text "Title"
          text "Supporting text" display=(show ? "flex" : "none") testId="bound-detail"
        row testId="zero-or-two"
          column
            if show
              text "Title"
              text "Supporting text"
        column testId="two-or-three"
          text "Title"
          text "Supporting text"
          if show
            text "Third line"

component Detail
  state expanded = true
  view
    if expanded
      text "Supporting text" testId="child-detail"
"#,
    );
    let mut heights = Vec::new();
    for (step, show) in [true, false, true].into_iter().enumerate() {
        if step > 0 {
            p.tap(id(&p, "toggle")).unwrap();
        }
        let k = p.host().kernel();
        let journal: Vec<_> = p.host().runner().journal().collect();
        assert!(
            !journal.iter().any(|line| line.contains("BadScope")),
            "{journal:?}"
        );
        assert_eq!(!k.find_by_test_id("child-detail").is_empty(), show);
        for name in ["owned", "match", "display", "zero-or-two", "two-or-three"] {
            let row = k.node(id(&p, name)).unwrap();
            assert_eq!(row.style.min_height, Dimension::Points(56.), "{name}");
        }
        let bound = k.node(id(&p, "bound-detail")).unwrap();
        assert_eq!(bound.style.display != Display::None, show);
        heights.push(rect(&mut p, "two-or-three").3);
    }
    assert!(
        heights[0] > heights[1] && heights[0] == heights[2],
        "{heights:?}"
    );
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
    main navigationKey="settings" navigationBack="" width=400 height=600
      column navigationKey="settings" navigationScroll="inset"
        header
          text "Settings" role="heading" aria-level=1
        view height=32 testId="band"
        row
          list appearance="auto" id="inset" width=128 height=160 testId="inset"
            section
              text "Inset" testId="inset-row"
          list appearance="auto" listStyle="plain" width=128 height=160 testId="plain"
            section
              text "Plain" testId="plain-row"
          list appearance="auto" listStyle="grouped" width=128 height=160 testId="grouped"
            section
              text "Edged" testId="edged" border-bottom="2px solid #ff0000"
"##,
    );
    let mut light = [0xff000000; MATERIAL_ROLES];
    light[Material::Surface as usize] = 0xffab_cdef;
    light[Material::SurfaceContainerLow as usize] = 0xff12_3456;
    p.set_palette(&Palette { light, dark: light });
    let surface = [171, 205, 239, 255];
    let list_points = ["inset", "plain", "grouped"].map(|name| {
        let list = rect(&mut p, name);
        (name, list.0 + 8., list.1 + 145.)
    });
    let row_points = ["inset-row", "plain-row", "edged", "band"].map(|name| {
        let part = rect(&mut p, name);
        (name, part.0 + 8., part.1 + part.3 / 2.)
    });

    let k = p.host().kernel();
    let row = id(&p, "edged");
    let card = k.node(k.node(row).unwrap().parent.unwrap()).unwrap();
    assert_eq!(card.style.border_widths(), [0.; 4]);
    let row = rect(&mut p, "edged");
    let frame = p.frame();
    let sample = |x, y| {
        let at = |v: f32| (v * 2.4375) as u32;
        let c = frame.pixel(at(x), at(y)).unwrap().demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    };
    let points: Vec<_> = list_points.into_iter().chain(row_points).collect();
    let colors: Vec<_> = points
        .iter()
        .map(|&(name, x, y)| (name, sample(x, y)))
        .collect();
    assert_eq!(
        colors,
        points
            .iter()
            .map(|(name, _, _)| (*name, surface))
            .collect::<Vec<_>>()
    );
    assert_eq!(sample(row.0 + 8., row.1 + row.3 - 1.), [255, 0, 0, 255]);
}
