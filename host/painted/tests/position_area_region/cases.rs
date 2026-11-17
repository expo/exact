//! Chrome geometry from host/web/tests/position-area-region.test.mjs.

#[derive(Debug)]
pub struct Case {
    pub anchor: (f32, f32),
    pub area: String,
    pub fallbacks: &'static str,
    pub rect: (f32, f32, f32, f32),
    pub boxes: usize,
    pub css: String,
}

pub fn cases(rtl: bool) -> Vec<Case> {
    let rows = [
        (0., 100., "bottom span-right", "none", (0., 120., 400., 36.), 12, ""),
        (150., 100., "bottom span-right", "none", (150., 120., 250., 54.), 12, ""),
        (150., 100., "bottom span-right", "flip-inline", (150., 120., 250., 54.), 12, ""),
        (300., 100., "bottom span-right", "none", (300., 120., 100., 108.), 12, ""),
        (300., 100., "bottom span-right", "flip-inline", (300., 120., 100., 108.), 12, ""),
        (380., 100., "bottom span-right", "none", (370., 120., 50., 216.), 12, ""),
        (380., 100., "bottom span-right", "flip-inline", (0., 120., 420., 36.), 12, ""),
        (150., 100., "bottom", "none", (0., 120., 400., 36.), 12, ""),
        (150., 100., "center", "none", (145., 2., 50., 216.), 12, ""),
        (10., 100., "center", "none", (5., 2., 50., 216.), 12, ""),
        (150., 550., "bottom span-right", "flip-block", (150., 496., 250., 54.), 12, ""),
        (150., 100., "right", "none", (190., 83., 210., 54.), 12, ""),
        (150., 100., "left", "none", (0., 74., 150., 72.), 12, ""),
        (150., 100., "top span-left", "none", (0., 28., 190., 72.), 12, ""),
        (300., 100., "bottom span-left", "none", (0., 120., 340., 36.), 12, ""),
        (20., 100., "bottom span-left", "flip-inline", (0., 120., 60., 216.), 12, ""),
        (150., 550., "bottom span-right", "none", (150., 546., 250., 54.), 12, ""),
        (300., 100., "bottom span-right", "flip-inline", (300., 120., 100., 54.), 6, ""),
        (300., 560., "bottom span-right", "flip-block", (300., 506., 100., 54.), 6, ""),
        (60., 100., "bottom span-left", "flip-inline", (0., 120., 100., 54.), 6, ""),
        (100., 500., "bottom span-right", "flip-start", (140., 500., 180., 60.), 6, "width:60px;height:180px"),
        (100., 500., "bottom span-right", "flip-start", (140., 500., 160., 90.), 6, "width:60px;height:180px;min-width:90px;min-height:140px;max-width:110px;max-height:160px"),
        (100., 500., "bottom span-right", "flip-start", (160., 510., 180., 36.), 6, "height:180px;margin-top:20px;margin-right:30px;margin-bottom:5px;margin-left:10px"),
        (100., 500., "bottom span-right", "flip-start", (144., 526., 180., 36.), 6, "height:180px;margin-top:4px;margin-right:5%;margin-bottom:8px;margin-left:10%"),
        (100., 550., "bottom span-right", "flip-start flip-block", (160., 500., 180., 60.), 6, "width:60px;height:180px;margin-top:20px;margin-right:30px;margin-bottom:5px;margin-left:10px"),
    ];
    let mirror = |s: &str| {
        s.replace("left", "MIRROR")
            .replace("right", "left")
            .replace("MIRROR", "right")
    };
    rows.into_iter()
        .map(|(ax, ay, area, fallbacks, (x, y, w, h), boxes, css)| Case {
            anchor: (if rtl { 400. - ax - 40. } else { ax }, ay),
            area: if rtl { mirror(area) } else { area.into() },
            fallbacks,
            rect: (if rtl { 400. - x - w } else { x }, y, w, h),
            boxes,
            css: if rtl { mirror(css) } else { css.into() },
        })
        .collect()
}

pub fn source(c: &Case, rtl: bool, scale: f32, cells: bool) -> String {
    let length = |n: f32| {
        if cells {
            format!("\"{}ch\"", n * scale / 8.)
        } else {
            (n * scale).to_string()
        }
    };
    let css = c
        .css
        .split(';')
        .filter_map(|row| row.split_once(':'))
        .map(|(k, v)| {
            let value = v.trim_end_matches("px");
            match value.parse::<f32>() {
                Ok(n) => format!("{k}={}", length(n)),
                Err(_) => format!("{k}=\"{value}\""),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    let limits = ["max-width", "max-height"]
        .into_iter()
        .filter(|key| {
            !c.css
                .split(';')
                .any(|row| row.starts_with(&format!("{key}:")))
        })
        .map(|key| format!("{key}=\"none\""))
        .collect::<Vec<_>>()
        .join(" ");
    let boxes = format!(
        "        box width={} height={} flex-shrink=0\n",
        length(50.),
        length(18.)
    )
    .repeat(c.boxes);
    format!(
        r#"component App
  state at = []
  action read
    at = [frame("pop").x, frame("pop").y, frame("pop").width, frame("pop").height]
  view
    box {direction}
      button id="anchor" testId="anchor" popovertarget="pop" position="absolute" left={ax} top={ay} width={aw} height={ah} appearance="none" padding=0 border-width=0
      row id="pop" testId="pop" popover="manual" position-area="{area}" position-try-fallbacks="{fallbacks}" flex-wrap="wrap" margin=0 padding=0 border-width=0 overflow="visible" {limits} {css}
{boxes}"#,
        direction = if cells {
            String::new()
        } else {
            format!("direction=\"{}\"", if rtl { "rtl" } else { "ltr" })
        },
        ax = length(c.anchor.0),
        ay = length(c.anchor.1),
        aw = length(40.),
        ah = length(20.),
        area = c.area,
        fallbacks = c.fallbacks
    )
}
