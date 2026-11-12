use super::*;

// neutral.css styles every direct route header, including one whose two
// headings make it ineligible for a native bar. A button may be nested.
const HEADERS: &str = r##"component App
  state taps = 0
  action go
    taps = taps + 1
  view
    column navigationKey="route" width="100%" color-scheme="SCHEME" color="#aa00cc" background-color="#fedcba"
      header testId="twin" height=40 min-height=0 padding=0 box-sizing="content-box" display="flex" align-items="center" gap=8
        text "MMMM" role="heading" font-size=20 font-weight=700 line-height=1 width=72 height=24 margin=0 testId="ink"
        text "Two" role="heading" font-size=20 margin=0
        row gap=8
          button press=go width=48 height=24 testId="nested"
            text "Go"
          button width=48 height=24 border="2px solid #123456" background-color="#abcdef" testId="own-button"
            text "Own"
      text "After" height=10 testId="after"
      header testId="own" height=32 min-height=0 padding=0 box-sizing="content-box" color="#aa00cc" background-color="#abcdef" border-bottom="2px solid #654321"
        text "MMMM" font-size=20 font-weight=700 line-height=1 width=72 height=24 margin=0 testId="own-ink"
      header testId="none" height=20 min-height=0 padding=0 box-sizing="content-box" border-bottom="none"
      column
        header testId="not-direct" height=20
      text "MMMM" role="heading" font-size=20 font-weight=700 line-height=1 width=72 height=24 margin=0 color="-exact-label" background-color="-exact-background" testId="ink-reference"
      text "MMMM" font-size=20 font-weight=700 line-height=1 width=72 height=24 margin=0 color="#aa00cc" background-color="#abcdef" testId="own-ink-reference"
      row height=0 background-color="-exact-background" border-bottom="1px solid -exact-separator" testId="edge-reference"
      text `Taps ${taps}` testId="taps"
"##;

pub(super) fn route_headers_keep_the_page_tree_and_take_the_neutral_defaults() {
    for (scheme, ground) in [("light", [255; 3]), ("dark", [18; 3])] {
        let plan = contract::compile(&HEADERS.replace("SCHEME", scheme)).unwrap();
        for width in [320.0, 640.0] {
            let (mut p, error) = Presenter::boot_with(
                &plan.encode(),
                (),
                (width, 220.0),
                1.0,
                std::path::PathBuf::new(),
                PainterChoice::Cpu,
            )
            .unwrap();
            // Read under its own roles: the test's other sessions live too.
            let _roles = p.host().runner().bind_roles();
            assert!(error.is_none(), "{error:?}");
            // The CSS border occupies room, unlike a list's painted divider.
            assert_eq!(size(&mut p, "twin"), (width, 41.0), "{scheme}");
            assert_eq!(p.rect_of(id(&p, "after")).unwrap().1, 41.0);
            assert_eq!(pixel(&mut p, "twin", width - 2.0, 2.0), ground);
            assert_eq!(pixel(&mut p, "twin", width - 2.0, 39.0), ground);
            // The same translucent role goes through the ordinary border painter.
            assert_eq!(
                pixel(&mut p, "twin", width - 2.0, 40.0),
                pixel(&mut p, "edge-reference", width - 2.0, 0.0)
            );
            assert_eq!(
                text_pixels(&mut p, "ink"),
                text_pixels(&mut p, "ink-reference"),
                "the header replaces inherited page ink, {scheme} at {width}"
            );
            assert_eq!(size(&mut p, "nested"), (48.0, 24.0));
            assert_eq!(pixel(&mut p, "nested", 0.0, 12.0), ground);
            assert_eq!(size(&mut p, "own-button"), (52.0, 28.0));
            assert_eq!(pixel(&mut p, "own-button", 0.0, 12.0), [0x12, 0x34, 0x56]);
            assert_eq!(pixel(&mut p, "own-button", 4.0, 12.0), [0xab, 0xcd, 0xef]);
            assert_eq!(size(&mut p, "own"), (width, 34.0));
            assert_eq!(pixel(&mut p, "own", width - 2.0, 31.0), [0xab, 0xcd, 0xef]);
            assert_eq!(pixel(&mut p, "own", width - 2.0, 33.0), [0x65, 0x43, 0x21]);
            // Written ink wins even when it equals the inherited page ink.
            assert_eq!(
                text_pixels(&mut p, "own-ink"),
                text_pixels(&mut p, "own-ink-reference")
            );
            assert_eq!(size(&mut p, "none"), (width, 20.0));
            assert_eq!(pixel(&mut p, "none", width - 2.0, 19.0), ground);
            assert_eq!(size(&mut p, "not-direct"), (width, 20.0));
            assert_eq!(
                pixel(&mut p, "not-direct", width - 2.0, 19.0),
                [0xfe, 0xdc, 0xba]
            );
            p.tap(id(&p, "nested")).unwrap();
            let k = p.host().kernel();
            let text: String = k
                .node(id(&p, "taps"))
                .unwrap()
                .text_runs()
                .iter()
                .map(|r| &*r.text)
                .collect();
            assert_eq!(text, "Taps 1", "the nested control keeps its page action");
        }
    }
}

pub(super) fn route_header_colors_follow_each_sessions_theme() {
    for (theme, grounds) in [
        (
            r##"theme
  background = "light-dark(#112233, #223344)"
  label = "light-dark(#ddeeff, #ccddee)"
  separator = "light-dark(#778899, #8899aa)"
"##,
            [[0x11, 0x22, 0x33], [0x22, 0x33, 0x44]],
        ),
        (
            r##"theme
  background = "light-dark(#aabbcc, #bbccdd)"
  label = "light-dark(#445566, #556677)"
  separator = "light-dark(#123456, #234567)"
"##,
            [[0xaa, 0xbb, 0xcc], [0xbb, 0xcc, 0xdd]],
        ),
        ("", [[255; 3], [18; 3]]),
    ] {
        for (scheme, ground) in ["light", "dark"].into_iter().zip(grounds) {
            let plan =
                contract::compile(&format!("{theme}\n{}", HEADERS.replace("SCHEME", scheme)))
                    .unwrap();
            for width in [320., 640.] {
                let (mut p, error) = Presenter::boot_with(
                    &plan.encode(),
                    (),
                    (width, 220.),
                    1.,
                    std::path::PathBuf::new(),
                    PainterChoice::Cpu,
                )
                .unwrap();
                let _roles = p.host().runner().bind_roles();
                assert!(error.is_none(), "{error:?}");
                assert_eq!(pixel(&mut p, "twin", width - 2., 2.), ground);
                assert_eq!(
                    pixel(&mut p, "twin", width - 2., 40.),
                    pixel(&mut p, "edge-reference", width - 2., 0.)
                );
                assert_eq!(
                    text_pixels(&mut p, "ink"),
                    text_pixels(&mut p, "ink-reference"),
                    "the session's header ink, {scheme} at {width}"
                );
                assert_eq!(pixel(&mut p, "own", width - 2., 31.), [0xab, 0xcd, 0xef]);
                assert_eq!(pixel(&mut p, "own", width - 2., 33.), [0x65, 0x43, 0x21]);
                assert_eq!(
                    text_pixels(&mut p, "own-ink"),
                    text_pixels(&mut p, "own-ink-reference")
                );
            }
        }
    }
}
