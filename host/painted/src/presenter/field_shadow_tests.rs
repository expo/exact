//! Decoration shadows rendered through the shared field presenter.
use super::*;

#[test]
fn decoration_shadows_paint_in_the_field() {
    let source = r##"decoration shade text-shadow="8px 45px 0px #ff0000" text-decoration="underline line-through"
component Fields
  view
    column width=400 height=600 background-color="#ffffff"
      textarea id="field" testId="field" value="HHHH HHHH" decorations=TextDecorations(value="HHHH HHHH",spans=[TextDecorationSpan(name="shade",start=0,end=4)]) appearance="none" width=260 height=140 padding=4 font-size=30 color="#000000" background-color="#ffffff"
"##;
    let (mut p, _) = boot_source(source);
    let pixels = p.frame();
    let red = pixels
        .pixels()
        .iter()
        .filter(|c| c.red() > 128 && c.green() < 64 && c.blue() < 64)
        .count();
    assert!(red > 100, "the decorated run casts a red shadow, got {red}");
    let reference = source
        .replace("8px 45px 0px #ff0000", "none")
        .replace("HHHH HHHH", "HHHH     ");
    let (mut reference, _) = boot_source(&reference);
    let reference = reference.frame();
    let mut covered = 0;
    for y in 0..45 {
        for x in 0..260 {
            let ink = reference.pixel(x, y).unwrap();
            if ink.red() == 0 && ink.green() == 0 && ink.blue() == 0 {
                let shadow = pixels.pixel(x + 8, y + 45).unwrap();
                assert_eq!(
                    (shadow.red(), shadow.green(), shadow.blue()),
                    (255, 0, 0),
                    "glyphs, underlines and strikes share the shadow mask at {x},{y}"
                );
                covered += 1;
            }
        }
    }
    assert!(covered > 100);
}

#[test]
fn decoration_shadows_blur_after_offset_and_clip_to_the_field() {
    let draw = |shadow| {
        let source = format!(
            r##"decoration shade text-shadow="{shadow}"
component Fields
  view
    column width=400 height=600 background-color="#ffffff"
      textarea id="field" testId="field" value="HHHH" decorations=TextDecorations(value="HHHH",spans=[TextDecorationSpan(name="shade",start=0,end=4)]) appearance="none" border="2px solid #000000" width=140 height=100 padding=4 font-size=30 color="#000000" background-color="#ffffff"
"##
        );
        let (mut p, field) = boot_source(&source);
        let rect = p.rect_of(field).unwrap();
        (p.frame(), rect)
    };
    let (base, rect) = draw("none");
    let (sharp, _) = draw("-12px 65px 0px #ff0000");
    let (blurred, _) = draw("-12px 65px 8px #ff0000");
    let coverage =
        |p: &tiny_skia::Pixmap| p.pixels().iter().filter(|c| c.red() > c.green()).count();
    assert!(coverage(&sharp) > 100);
    assert!(
        (2..6).any(|x| (2..98)
            .any(|y| sharp.pixel(x, y).unwrap().red() > sharp.pixel(x, y).unwrap().green())),
        "the shadow reaches into the left padding"
    );
    assert!(
        coverage(&blurred) > coverage(&sharp),
        "blur widens the mask"
    );
    let (right_bottom, _) = draw("110px 65px 8px #ff0000");
    let (left_top, _) = draw("-12px -12px 8px #ff0000");
    assert!(coverage(&right_bottom) > 0 && coverage(&left_top) > 0);
    for pixels in [&blurred, &right_bottom, &left_top] {
        for y in 0..base.height() {
            for x in 0..base.width() {
                if (x as f32) < rect.0 + 2.
                    || (x as f32) >= rect.0 + rect.2 - 2.
                    || (y as f32) < rect.1 + 2.
                    || (y as f32) >= rect.1 + rect.3 - 2.
                {
                    assert_eq!(pixels.pixel(x, y), base.pixel(x, y), "clipped at {x},{y}");
                }
            }
        }
    }
    assert_eq!(draw("400px 0px 8px #ff0000").0.data(), base.data());
}

#[test]
fn decoration_shadows_follow_currentcolor_through_composition_and_removal() {
    let source = r##"decoration shade text-shadow="0px 45px 0px"
decoration ink color="#0000ff"
decoration clear text-shadow="none"
component Fields
  state marks = [TextDecorationSpan(name="shade",start=0,end=9),TextDecorationSpan(name="ink",start=0,end=4),TextDecorationSpan(name="clear",start=5,end=9)]
  action clear
    marks = []
  view
    column width=400 height=600 background-color="#ffffff"
      textarea id="field" testId="field" value="HHHH HHHH" decorations=TextDecorations(value="HHHH HHHH",spans=marks) appearance="none" width=260 height=140 padding=4 font-size=30 color="#000000" background-color="#ffffff" caret-color="transparent"
      button "Clear" press=clear testId="clear"
"##;
    let (mut p, field) = boot_source(source);
    p.set_focus(Some(field), 0.);
    let before = p.frame();
    let blue = |pixels: &tiny_skia::Pixmap| {
        (45..100)
            .flat_map(|y| (0..260).map(move |x| pixels.pixel(x, y).unwrap()))
            .filter(|c| c.blue() > 128 && c.red() < 64 && c.green() < 64)
            .count()
    };
    assert!(blue(&before) > 100, "the later ink supplies currentcolor");
    for y in 45..100 {
        for x in 140..260 {
            let pixel = before.pixel(x, y).unwrap();
            assert_eq!(
                (pixel.red(), pixel.green(), pixel.blue()),
                (255, 255, 255),
                "the later none removes the second run's shadow"
            );
        }
    }
    assert!(p.ime_begin(p.ime_target().unwrap()));
    p.ime_preedit("x", None);
    assert_eq!(
        blue(&p.frame()),
        blue(&before),
        "the unedited run keeps its shadow"
    );
    p.ime_end();
    assert_eq!(
        p.frame().data(),
        before.data(),
        "cancelling restores the field"
    );
    let k = p.host.kernel();
    let clear = k.node_by_key(k.find_by_test_id("clear")[0]).unwrap().id;
    assert_eq!(p.host.dispatch_at(clear, Event::Press, 0.), None);
    assert_eq!(p.after_commit(), None);
    assert_eq!(
        blue(&p.frame()),
        0,
        "removing the spans removes their shadows"
    );
    assert_eq!(p.field_value(field), "HHHH HHHH");
}

#[test]
fn decoration_shadows_in_long_scrolling_fields_match_a_short_field() {
    let draw = |lines: usize, scroll: usize, cast_line: isize, shadow: &str, scale| {
        let value = std::iter::repeat_n("H", lines)
            .collect::<Vec<_>>()
            .join("\n");
        let value = serde_json::to_string(&value).unwrap();
        let start = ((scroll as isize + cast_line) * 2) as usize;
        let end = start + 1;
        let source = format!(
            r##"decoration shade text-shadow="{shadow}" text-decoration="underline line-through"
component Fields
  view
    column width=400 height=600 background-color="#ffffff"
      textarea testId="field" value={value} decorations=TextDecorations(value={value},spans=[TextDecorationSpan(name="shade",start={start},end={end})]) appearance="none" outline="none" border="2px solid #000000" width=140 height=100 padding=4 font-size=20 line-height="24px" color="#000000" background-color="#ffffff"
"##
        );
        let (mut p, field) = boot_source(&source);
        assert!(p.resize_scaled(400., 600., scale).is_none());
        let offset = (0., scroll as f32 * 24.);
        assert_eq!(p.scroll_to(field, offset), Some(offset));
        p.frame()
    };
    for scale in [1., 2.] {
        for (shadow, cast_line) in [
            ("8px 40px 0px #ff0000", 0),
            ("-8px -35px 8px #ff0000", 4),
            ("8px 40px 8px #ff0000", -1),
        ] {
            let short_scroll = if cast_line < 0 { 4 } else { 0 };
            let reference = draw(12, short_scroll, cast_line, shadow, scale);
            assert!(reference.pixels().iter().any(|c| c.red() > c.green()));
            for scroll in [short_scroll, 350, 695] {
                let actual = draw(700, scroll, cast_line, shadow, scale);
                assert!(
                    actual.data() == reference.data(),
                    "scroll={scroll}, shadow={shadow}, scale={scale}"
                );
            }
        }
    }
}

#[test]
fn decoration_shadows_do_not_cast_element_or_ancestor_line_shadows() {
    for line in ["underline", "line-through", "underline line-through"] {
        for parent in [false, true] {
            let draw = |shadow| {
                let node_shadow = format!(r#"text-shadow="{shadow}""#);
                let (parent_shadow, field_shadow) = if parent {
                    (node_shadow.as_str(), "")
                } else {
                    ("", node_shadow.as_str())
                };
                let source = format!(
                    r##"decoration shade text-shadow="8px 40px 2px #0000ff" text-decoration="underline line-through"
component Fields
  view
    column width=400 height=600 background-color="#ffffff" text-decoration="{line}" {parent_shadow}
      textarea testId="field" value="HHHH HHHH" decorations=TextDecorations(value="HHHH HHHH",spans=[TextDecorationSpan(name="shade",start=0,end=4)]) appearance="none" outline="none" width=260 height=140 padding=4 font-size=30 color="#000000" background-color="#ffffff" text-decoration="{line}" {field_shadow}
"##
                );
                let (mut p, _) = boot_source(&source);
                p.frame()
            };
            let reference = draw("none");
            assert!(reference.pixels().iter().any(|c| c.blue() > c.red()));
            assert!(
                draw("0px 40px 0px #ff0000").data() == reference.data(),
                "line={line}, inherited shadow={parent}"
            );
        }
    }
}

#[test]
fn decoration_shadows_follow_aligned_glyphs() {
    for align in ["right", "center"] {
        let draw = |shadow| {
            let source = format!(
                r##"decoration shade text-shadow="{shadow}"
component Fields
  view
    column width=400 height=600 background-color="#ffffff"
      textarea testId="field" value="H" decorations=TextDecorations(value="H",spans=[TextDecorationSpan(name="shade",start=0,end=1)]) appearance="none" outline="none" width=260 height=140 padding=4 font-size=30 text-align="{align}" color="#000000" background-color="#ffffff" caret-color="transparent"
"##
            );
            let (mut p, _) = boot_source(&source);
            p.frame()
        };
        let reference = draw("none");
        let pixels = draw("0px 40px 0px #ff0000");
        let mut covered = 0;
        for y in 0..60 {
            for x in 0..268 {
                let ink = reference.pixel(x, y).unwrap();
                if ink.red() == 0 && ink.green() == 0 && ink.blue() == 0 {
                    let shadow = pixels.pixel(x, y + 40).unwrap();
                    assert_eq!(
                        (shadow.red(), shadow.green(), shadow.blue()),
                        (255, 0, 0),
                        "text-align={align}: the shadow follows the glyph at {x},{y}"
                    );
                    covered += 1;
                }
            }
        }
        assert!(covered > 20, "text-align={align}: the glyph is drawn");
    }
}
