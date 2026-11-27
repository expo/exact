use super::*;
use crate::canvas::{stream, Recorder, CLIP_RRECT, GLYPHS};
use crate::presenter::access::role;
use crate::presenter::bars::look::{top_content_height, Item, Layout};
use crate::presenter::top::look::Role;

const DETAIL_APP: &str = r##"routes nav
  tab home "/"
    detail "/detail"
    sheet "/sheet"
component App
  state sub = "Android 16 · Pixel 7"
  state heading = "Build Details With A Very Long Route Title That Must Truncate"
  action open(url: string)
    nav = push(nav, url)
  action back
    nav = back(nav)
  action clearSub
    sub = ""
  action blankSub
    sub = "   "
  action altSub
    sub = "Android 15 · Pixel 8"
  action restoreSub
    sub = "Android 16 · Pixel 7"
  action share
    sub = sub
  action more
    sub = sub
  view
    main navigationKey=`${top(nav).id}` navigationBack="back" width="100%" height="100%"
      for t in nav.tabs key = t.name
        for e in t.stack key = e.id
          if e.name == "home"
            column navigationKey=`${e.id}` testId="route-home" position="absolute" inset=0 display="flex" flex-direction="column"
              header testId="home-header"
                text "Home" role="heading" aria-level=1 testId="home-title"
              column testId="home-body" flex=1
                button press=open("/detail") testId="open-detail" height=40
                  text "Open detail"
          if e.name == "detail"
            column navigationKey=`${e.id}` testId="route-detail" position="absolute" inset=0 display="flex" flex-direction="column"
              header testId="detail-header"
                button id="back" press=back testId="back"
                  image "symbol:back"
                text heading role="heading" testId="detail-title"
                if sub != ""
                  text sub testId="detail-subtitle"
                button press=share aria-label="Share" testId="share"
                  image "symbol:share"
                button press=more aria-label="More" testId="more"
                  image "symbol:more"
              column testId="detail-body" flex=1 background-color="#ffffff"
                button press=clearSub testId="clear-sub" height=40
                  text "Clear"
                button press=blankSub testId="blank-sub" height=40
                  text "Blank"
                button press=altSub testId="alt-sub" height=40
                  text "Alt"
                button press=restoreSub testId="restore-sub" height=40
                  text "Restore"
                button press=open("/sheet") testId="open-sheet" height=40
                  text "Open sheet"
          if e.name == "sheet"
            column navigationKey=`${e.id}` navigationPresentation="modal" navigationDetent="medium" testId="route-sheet"
              text "Account sheet"
"##;

fn device_at(font_scale: f32) -> Device {
    Device {
        night: false,
        font_scale,
        orientation: None,
        screen_reader: exact_runner::ScreenReader::Unknown,
        locale: "en-US".into(),
        rtl: false,
        hour24: false,
        reduced_motion: false,
        bold_text: false,
        high_contrast: false,
        touch: true,
        tv: false,
        touch_slop: 8.0,
        dates: String::new(),
        format_locale: String::new(),
    }
}

fn owner_config(p: &mut Presenter<Nothing>, route: ViewId) -> u64 {
    p.owner_updates()
        .into_iter()
        .find(|(r, _)| *r == route)
        .unwrap()
        .1
        .configuration
        .unwrap()
}

#[test]
fn detail_route_with_subtitle_back_two_actions_and_long_title_truncates_and_aligns() {
    let (mut p, _) = barred(DETAIL_APP);
    // Home stays single-line at 64 dp.
    let home_json = p.bars_json();
    assert_eq!(home_json["top"]["title"], "Home");
    assert!(home_json["top"]["subtitle"].is_null());
    assert!(home_json["top"]["subtitleNode"].is_null());
    assert_eq!(home_json["top"]["frame"]["h"], 52.0 + 64.0);

    tap(&mut p, "open-detail").unwrap();
    let detail = id(&p, "route-detail");
    let title_id = id(&p, "detail-title");
    let sub_id = id(&p, "detail-subtitle");
    let json = p.bars_json();
    assert_eq!(
        json["top"]["title"],
        "Build Details With A Very Long Route Title That Must Truncate"
    );
    assert_eq!(json["top"]["subtitle"], "Android 16 · Pixel 7");
    assert_eq!(json["top"]["subtitleNode"], sub_id);
    assert_eq!(json["top"]["up"], true);
    assert_eq!(json["top"]["targets"].as_array().unwrap().len(), 3);
    assert_eq!(json["top"]["frame"]["h"], 52.0 + 64.0);
    assert_eq!(p.bar_plans().0.unwrap().trailing.len(), 2);

    let share_id = id(&p, "share");
    let (bar, _) = p.bars();
    let bar = bar.unwrap();
    let title_rect = bar.texts.iter().find(|(n, _)| *n == title_id).unwrap().1;
    let sub_rect = bar.texts.iter().find(|(n, _)| *n == sub_id).unwrap().1;
    let up = bar
        .targets
        .iter()
        .find(|t| t.what == What::Up)
        .unwrap()
        .rect;
    let first_trailing = bar
        .targets
        .iter()
        .find(|t| t.what == What::Item(share_id))
        .unwrap()
        .rect;

    // Title and subtitle share the exact start edge after Up (56 dp), truncate before Share,
    // stack with 0 dp gap and center together vertically in the 64 dp bar under status bar 52.
    assert!(
        (title_rect.0 - sub_rect.0).abs() < 0.05,
        "{title_rect:?} vs {sub_rect:?}"
    );
    assert!(
        title_rect.0 >= up.0 + up.2 + 3.9,
        "clears Up: {title_rect:?} vs {up:?}"
    );
    assert!(
        title_rect.0 + title_rect.2 <= first_trailing.0 - 3.9,
        "title truncates before trailing action: {title_rect:?} vs {first_trailing:?}"
    );
    assert!(
        sub_rect.0 + sub_rect.2 <= first_trailing.0 - 3.9,
        "subtitle fits before trailing action: {sub_rect:?} vs {first_trailing:?}"
    );
    assert!(sub_rect.1 >= title_rect.1 + title_rect.3 - 0.5);
    assert!(sub_rect.1 < title_rect.1 + title_rect.3 + 4.0);
    let pair_mid = (title_rect.1 + (sub_rect.1 + sub_rect.3)) / 2.0;
    assert!(
        (pair_mid - (52.0 + 32.0)).abs() < 3.0,
        "pair_mid={pair_mid}"
    );

    // At 1x scale, replacing or toggling the subtitle keeps the 64 dp bar height and owner config stable.
    let config_1x = owner_config(&mut p, detail);
    tap(&mut p, "alt-sub").unwrap();
    assert_eq!(p.bars_json()["top"]["subtitle"], "Android 15 · Pixel 8");
    assert_eq!(owner_config(&mut p, detail), config_1x);

    // Clearing the conditional subtitle restores single-line layout (title re-centers vertically).
    tap(&mut p, "clear-sub").unwrap();
    assert!(p.bars_json()["top"]["subtitle"].is_null());
    assert!(p.bars_json()["top"]["subtitleNode"].is_null());
    assert_eq!(owner_config(&mut p, detail), config_1x);
    let single_rect = p
        .bars()
        .0
        .unwrap()
        .texts
        .iter()
        .find(|(n, _)| *n == title_id)
        .unwrap()
        .1;
    assert!(
        single_rect.1 > title_rect.1,
        "single-line title sits lower (centered alone) than two-line title: single={single_rect:?} two={title_rect:?}"
    );

    // Whitespace-only conditional subtitle also stays single-line.
    tap(&mut p, "blank-sub").unwrap();
    assert!(p.bars_json()["top"]["subtitle"].is_null());
    assert_eq!(p.bars().0.unwrap().texts.len(), 1);
}

#[test]
fn subtitle_accessibility_emits_text_after_heading_and_respects_aria_inert_and_inline_names() {
    let (mut p, _) = barred(DETAIL_APP);
    tap(&mut p, "open-detail").unwrap();
    let sub_id = id(&p, "detail-subtitle");
    let (bar, _) = p.bars();
    let bar = bar.unwrap();
    let frame = bar.frame;
    let sub_rect = bar.texts.iter().find(|(n, _)| *n == sub_id).unwrap().1;

    let (nodes, _, pane_title) = p.access_tree();
    assert_eq!(
        pane_title.1, "Build Details With A Very Long Route Title That Must Truncate",
        "pane title stays the heading alone"
    );
    let bar_roles: Vec<(i32, &str)> = nodes
        .iter()
        .take(5)
        .map(|n| (n.role, n.name.as_str()))
        .collect();
    assert_eq!(
        bar_roles,
        vec![
            (role::UP, ""),
            (
                role::HEADING,
                "Build Details With A Very Long Route Title That Must Truncate"
            ),
            (role::TEXT, "Android 16 · Pixel 7"),
            (role::BUTTON, "Share"),
            (role::BUTTON, "More"),
        ]
    );
    let sub_ax = &nodes[2];
    assert!(
        (sub_ax.rect[0] - (frame[0] + sub_rect.0)).abs() < 0.1
            && (sub_ax.rect[1] - (frame[1] + sub_rect.1)).abs() < 0.1
            && (sub_ax.rect[2] - sub_rect.2).abs() < 0.1
            && (sub_ax.rect[3] - sub_rect.3).abs() < 0.1,
        "ax={:?} bar_sub={sub_rect:?} frame={frame:?}",
        sub_ax.rect
    );

    // `aria-hidden=true` on the subtitle itself or on a subtitle-only wrapper omits the TalkBack node.
    for variant in [
        DETAIL_APP.replace(
            r#"text sub testId="detail-subtitle""#,
            r#"text sub aria-hidden=true testId="detail-subtitle""#,
        ),
        DETAIL_APP.replace(
            "                  text sub testId=\"detail-subtitle\"",
            "                  box aria-hidden=true\n                    text sub testId=\"detail-subtitle\"",
        ),
    ] {
        let (mut p2, _) = barred(&variant);
        tap(&mut p2, "open-detail").unwrap();
        assert_eq!(p2.bars_json()["top"]["subtitle"], "Android 16 · Pixel 7");
        let (nodes2, _, _) = p2.access_tree();
        assert!(
            !nodes2
                .iter()
                .any(|n| n.role == role::TEXT && n.name == "Android 16 · Pixel 7"),
            "aria-hidden subtitle or subtitle-only wrapper must not emit a TalkBack node"
        );
    }

    // `aria-hidden=true` on a shared ancestor wrapping BOTH heading and subtitle stays consistent with heading.
    let shared_hidden_src = DETAIL_APP.replace(
        "                text heading role=\"heading\" testId=\"detail-title\"\n                if sub != \"\"\n                  text sub testId=\"detail-subtitle\"",
        "                column aria-hidden=true\n                  text heading role=\"heading\" testId=\"detail-title\"\n                  if sub != \"\"\n                    text sub testId=\"detail-subtitle\"",
    );
    let (mut p_shared, _) = barred(&shared_hidden_src);
    tap(&mut p_shared, "open-detail").unwrap();
    let (nodes_shared, _, _) = p_shared.access_tree();
    assert!(
        nodes_shared
            .iter()
            .any(|n| n.role == role::TEXT && n.name == "Android 16 · Pixel 7"),
        "shared ancestor with heading does not inconsistently hide only the subtitle"
    );

    // `inert=true` on the subtitle or its wrapper omits the TalkBack node.
    for variant in [
        DETAIL_APP.replace(
            r#"text sub testId="detail-subtitle""#,
            r#"text sub inert=true testId="detail-subtitle""#,
        ),
        DETAIL_APP.replace(
            "                  text sub testId=\"detail-subtitle\"",
            "                  box inert=true\n                    text sub testId=\"detail-subtitle\"",
        ),
    ] {
        let (mut p_inert, _) = barred(&variant);
        tap(&mut p_inert, "open-detail").unwrap();
        let (nodes_inert, _, _) = p_inert.access_tree();
        assert!(
            !nodes_inert
                .iter()
                .any(|n| n.role == role::TEXT && n.name == "Android 16 · Pixel 7"),
            "inert subtitle or inert wrapper must not emit a TalkBack node"
        );
    }

    // Inline child accessibility semantics (`aria-hidden` and `aria-label`) are honored via `access_name`.
    let inline_src = DETAIL_APP.replace(
        "                  text sub testId=\"detail-subtitle\"",
        "                  text testId=\"detail-subtitle\"\n                    text \"• \" aria-hidden=true\n                    text \"1w\" aria-label=\"1 week\"\n                    text \" muted\"",
    );
    let (mut p_inline, _) = barred(&inline_src);
    tap(&mut p_inline, "open-detail").unwrap();
    assert_eq!(p_inline.bars_json()["top"]["subtitle"], "• 1w muted");
    let (nodes_inline, _, _) = p_inline.access_tree();
    assert!(
        nodes_inline
            .iter()
            .any(|n| n.role == role::TEXT && n.name == "1 week muted"),
        "inline aria-hidden/aria-label must shape spoken name: {:?}",
        nodes_inline
            .iter()
            .take(5)
            .map(|n| (n.role, &n.name))
            .collect::<Vec<_>>()
    );

    // If inline `aria-hidden=true` suppresses all spoken text, omit the node rather than falling back.
    let all_hidden_inline_src = DETAIL_APP.replace(
        "                  text sub testId=\"detail-subtitle\"",
        "                  text testId=\"detail-subtitle\"\n                    text \"Decorative\" aria-hidden=true",
    );
    let (mut p_empty_ax, _) = barred(&all_hidden_inline_src);
    tap(&mut p_empty_ax, "open-detail").unwrap();
    assert_eq!(p_empty_ax.bars_json()["top"]["subtitle"], "Decorative");
    let (nodes_empty, _, _) = p_empty_ax.access_tree();
    assert!(
        !nodes_empty.iter().any(|n| n.role == role::TEXT),
        "subtitle whose inline children are all aria-hidden must emit no TalkBack node"
    );
}

#[test]
fn subtitle_extraction_respects_title_group_visibility_and_exclusions() {
    let check = |header_body: &str| -> Option<String> {
        let src = format!(
            r#"routes nav
  tab home "/"
component App
  state n = 0
  action tapTitle
    n = n + 1
  view
    main navigationKey=`${{top(nav).id}}` width="100%" height="100%"
      column navigationKey=`${{top(nav).id}}`
        header
{header_body}
        text "Body"
"#
        );
        let (mut p, _) = barred(&src);
        p.bars_json()["top"]["subtitle"]
            .as_str()
            .map(ToOwned::to_owned)
    };

    // Title-group wrapper around heading + subtitle works; later text outside wrapper is ignored.
    assert_eq!(
        check(
            r#"          column
            text "Title" role="heading"
            text "Inside wrapper"
          text "Outside wrapper""#
        ),
        Some("Inside wrapper".into())
    );
    // Pressable title group (`shape.tap`) scopes to its own children.
    assert_eq!(
        check(
            r#"          button press=tapTitle
            text "Alice" role="heading"
            text "Last seen 2m ago"
          text "Unrelated""#
        ),
        Some("Last seen 2m ago".into())
    );
    // Title-only pressable group does not steal a sibling outside the tap group.
    assert_eq!(
        check(
            r#"          button press=tapTitle
            text "Alice" role="heading"
          text "Outside tap group""#
        ),
        None
    );
    // Filled pill box after heading is skipped; real subtitle after it is picked.
    assert_eq!(
        check(
            r##"          text "Title" role="heading"
          row background-color="#eeeeee"
            text "BETA"
          text "Real subtitle""##
        ),
        Some("Real subtitle".into())
    );
    // Nested filled badge containing a symbol inside a wrapper column does NOT suppress sibling plain text.
    assert_eq!(
        check(
            r##"          text "Title" role="heading"
          column
            row background-color="#eeeeee"
              image "symbol:star"
            text "Android 16""##
        ),
        Some("Android 16".into())
    );
    // Non-symbol image or visibility:hidden symbol beside text is not a glyph line: text still projects.
    assert_eq!(
        check(
            r#"          text "Title" role="heading"
          row
            image "photo.png"
            text "Beside photo""#
        ),
        Some("Beside photo".into())
    );
    assert_eq!(
        check(
            r#"          text "Title" role="heading"
          row
            image "symbol:star" visibility="hidden"
            text "Visible beside hidden symbol""#
        ),
        Some("Visible beside hidden symbol".into())
    );
    // True glyph-line box (visible `symbol:` image + visible text) is skipped rather than stealing its first word.
    assert_eq!(
        check(
            r#"          text "Title" role="heading"
          row
            image "symbol:star"
            text "Muted""#
        ),
        None
    );
    // Subtitle inside a non-View container (`scroll`) is still reached.
    assert_eq!(
        check(
            r#"          text "Title" role="heading"
          scroll height=24
            text "In scroll""#
        ),
        Some("In scroll".into())
    );
    // Readout (`role="status"`), unplaced inert button, display:none and visibility:hidden are skipped.
    assert_eq!(
        check(
            r#"          text "Title" role="heading"
          row role="status"
            text "Syncing"
          button
            text "Inert"
          text "Gone" display="none"
          text "Hidden" visibility="hidden"
          box visibility="hidden"
            text "Visible override" visibility="visible""#
        ),
        Some("Visible override".into())
    );
    // Blank heading produces no subtitle.
    assert_eq!(
        check(
            r#"          text "   " role="heading"
          text "Orphan subtitle""#
        ),
        None
    );
}

#[test]
fn subtitle_font_scale_grows_live_across_thresholds_and_behind_sheet() {
    let (mut p, _) = barred(DETAIL_APP);
    let home = id(&p, "route-home");
    tap(&mut p, "open-detail").unwrap();
    let detail = id(&p, "route-detail");

    // Baseline 1x: both Home and Detail are 64 dp.
    assert_eq!(p.bars().0.unwrap().frame[3], 52.0 + 64.0);
    assert_eq!(p.rect_of(id(&p, "detail-body")).unwrap().1, 64.0);
    let config_1x = owner_config(&mut p, detail);

    // Threshold just above 64/44 (~1.455): 1.5x grows Detail to 66 dp while Home stays 64 dp.
    assert_eq!(top_content_height(1.5, false), 64.0);
    assert_eq!(top_content_height(1.5, true), 66.0);
    assert_eq!(p.set_device(&device_at(1.5)), None);
    assert_eq!(p.bars().0.unwrap().frame[3], 52.0 + 66.0);
    assert_eq!(p.rect_of(id(&p, "detail-body")).unwrap().1, 66.0);
    assert_eq!(p.host().kernel().local_insets(home).unwrap()[0], 64.0);
    assert_eq!(p.host().kernel().local_insets(detail).unwrap()[0], 66.0);
    let config_15 = owner_config(&mut p, detail);
    assert_ne!(config_15, config_1x);

    // Live set_text_scale to 2.0x on active subtitled route: Detail grows to 88 dp, Home stays 64 dp.
    assert_eq!(p.set_text_scale(2.0), None);
    assert_eq!(p.bars().0.unwrap().frame[3], 52.0 + 88.0);
    assert_eq!(p.rect_of(id(&p, "detail-body")).unwrap().1, 88.0);
    assert_eq!(p.host().kernel().local_insets(home).unwrap()[0], 64.0);
    assert_eq!(p.host().kernel().local_insets(detail).unwrap()[0], 88.0);
    let config_2x = owner_config(&mut p, detail);
    assert_ne!(config_2x, config_15);

    // Above 64/28 (~2.286): 2.5x grows plain Home to 70 dp and subtitled Detail to 110 dp.
    assert_eq!(top_content_height(2.5, false), 70.0);
    assert_eq!(top_content_height(2.5, true), 110.0);
    assert_eq!(p.set_device(&device_at(2.5)), None);
    assert_eq!(p.bars().0.unwrap().frame[3], 52.0 + 110.0);
    assert_eq!(p.host().kernel().local_insets(home).unwrap()[0], 70.0);
    assert_eq!(p.host().kernel().local_insets(detail).unwrap()[0], 110.0);

    // Shrinking back to 1.0x via set_text_scale restores 64 dp live.
    assert_eq!(p.set_text_scale(1.0), None);
    assert_eq!(p.bars().0.unwrap().frame[3], 52.0 + 64.0);
    assert_eq!(p.rect_of(id(&p, "detail-body")).unwrap().1, 64.0);
    assert_eq!(owner_config(&mut p, detail), config_1x);

    // Live font_scale change while a partial modal sheet is open updates Detail's underlying bar & inset.
    tap(&mut p, "open-sheet").unwrap();
    let open_ms = p.host().now() + 1000.0;
    p.tick(open_ms);
    assert_eq!(p.set_text_scale(2.0), None);
    assert_eq!(p.bars().0.unwrap().frame[3], 52.0 + 88.0);
    assert_eq!(p.host().kernel().local_insets(detail).unwrap()[0], 88.0);
    assert_eq!(p.set_device(&device_at(1.0)), None);
    assert_eq!(p.bars().0.unwrap().frame[3], 52.0 + 64.0);
    assert_eq!(p.host().kernel().local_insets(detail).unwrap()[0], 64.0);

    // Back to Detail at 2.0x: clearing the conditional subtitle shrinks Detail back to 64 dp in the same commit.
    let now = p.host().now();
    assert!(p.system_back(now));
    let close_ms = p.host().now() + 1000.0;
    p.tick(close_ms);
    assert_eq!(p.set_text_scale(2.0), None);
    assert_eq!(p.bars().0.unwrap().frame[3], 52.0 + 88.0);
    let config_sub_2x = owner_config(&mut p, detail);
    tap(&mut p, "clear-sub").unwrap();
    assert_eq!(p.bars().0.unwrap().frame[3], 52.0 + 64.0);
    assert_eq!(p.rect_of(id(&p, "detail-body")).unwrap().1, 64.0);
    assert_ne!(owner_config(&mut p, detail), config_sub_2x);
}

#[test]
fn subtitle_uses_authored_bar_ink_and_appears_under_partial_modal_sheet() {
    // 1. Default bar paints subtitle in OnSurfaceVariant; authored dark bar paints it in white ink.
    let (mut p_default, _) = barred(DETAIL_APP);
    tap(&mut p_default, "open-detail").unwrap();
    let sub_id = id(&p_default, "detail-subtitle");
    let _ = p_default.bars();
    let default_sub_color = p_default
        .bars
        .top
        .as_ref()
        .unwrap()
        .layout
        .items
        .iter()
        .find_map(|item| match item {
            Item::Text(_, _, color, node, _) if *node == sub_id => Some(*color),
            _ => None,
        })
        .unwrap();
    assert_eq!(default_sub_color, Role::OnSurfaceVariant.color(false));

    let dark_bar_src = DETAIL_APP.replace(
        r#"header testId="detail-header""#,
        r##"header background-color="#173529" testId="detail-header""##,
    );
    let (mut p_dark, _) = barred(&dark_bar_src);
    tap(&mut p_dark, "open-detail").unwrap();
    let dark_sub_id = id(&p_dark, "detail-subtitle");
    let _ = p_dark.bars();
    let dark_sub_color = p_dark
        .bars
        .top
        .as_ref()
        .unwrap()
        .layout
        .items
        .iter()
        .find_map(|item| match item {
            Item::Text(_, _, color, node, _) if *node == dark_sub_id => Some(*color),
            _ => None,
        })
        .unwrap();
    assert_eq!(dark_sub_color, [255, 255, 255, 255]);

    // 2. Behind a partially expanded modal sheet, compare two DIFFERENT nonempty subtitles so title
    // position, bar height and column geometry stay identical and pixel diffs prove subtitle ink alone.
    tap(&mut p_default, "open-sheet").unwrap();
    let open_ms = p_default.host().now() + 1000.0;
    p_default.tick(open_ms);
    let frame_a = p_default.frame();
    let title_id = id(&p_default, "detail-title");
    let bar_a = p_default.bars().0.unwrap();
    let title_a = bar_a.texts.iter().find(|(n, _)| *n == title_id).unwrap().1;
    let sub_rect = bar_a.texts.iter().find(|(n, _)| *n == sub_id).unwrap().1;

    let alt_sub_src = DETAIL_APP.replace(
        r#"state sub = "Android 16 · Pixel 7""#,
        r#"state sub = "Android 15 · Pixel 8""#,
    );
    let (mut p_alt, _) = barred(&alt_sub_src);
    tap(&mut p_alt, "open-detail").unwrap();
    tap(&mut p_alt, "open-sheet").unwrap();
    let open_ms = p_alt.host().now() + 1000.0;
    p_alt.tick(open_ms);
    let frame_b = p_alt.frame();
    let alt_title_id = id(&p_alt, "detail-title");
    let bar_b = p_alt.bars().0.unwrap();
    let title_b = bar_b
        .texts
        .iter()
        .find(|(n, _)| *n == alt_title_id)
        .unwrap()
        .1;
    assert_eq!(
        title_a, title_b,
        "title geometry must stay identical across subtitle swap"
    );

    let x0 = (sub_rect.0 as u32).max(1);
    let x1 = ((sub_rect.0 + sub_rect.2).ceil() as u32).min(411);
    let y0 = (sub_rect.1 - 52.0).max(0.0) as u32;
    let y1 = ((sub_rect.1 - 52.0 + sub_rect.3).ceil() as u32).min(64);
    let mut diffs = 0usize;
    for y in y0..y1 {
        for x in x0..x1 {
            if frame_a.pixel(x, y) != frame_b.pixel(x, y) {
                diffs += 1;
            }
        }
    }
    assert!(
        diffs > 10,
        "swapping nonempty subtitle text behind a partial sheet must repaint subtitle ink (diffs={diffs})"
    );
}

#[test]
fn subtitle_aligns_and_clips_to_allocated_column_in_ltr_and_rtl() {
    let mut engine = crate::text::TextEngine::from_font_files(
        &[std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../scripts/fixtures/fonts/assets"
        ))],
        "DejaVu Sans",
    );
    let device_inks = Default::default();
    let inputs = super::look::Inputs {
        device_inks: &device_inks,
        colors: super::look::BarColors::default(),
        dark: false,
        font_scale: 1.0,
        width: 300.0,
        edge: 0.0,
        sides: (0.0, 0.0),
    };
    let trailing = vec![Button {
        node: 4,
        title: "Star".into(),
        direction: exact_kernel::Direction::Ltr,
        symbol: Some("star".into()),
        symbol_direction: exact_kernel::Direction::Ltr,
        badge: None,
        enabled: true,
        destructive: false,
        tint: None,
    }];

    // LTR: a wide title shifts left from 16 dp toward 8 dp; the short subtitle shifts with it.
    let ltr = Top {
        route: 1,
        heading: Some(2),
        direction: exact_kernel::Direction::Ltr,
        title: "A wide title nearly filling the bar".into(),
        title_direction: exact_kernel::Direction::Ltr,
        subtitle_node: Some(3),
        subtitle: "Short".into(),
        subtitle_direction: exact_kernel::Direction::Ltr,
        up: false,
        leading: vec![],
        trailing: trailing.clone(),
        scrolled: false,
    };
    let layout_ltr = Layout::top(&mut engine, &ltr, &inputs);
    let texts_ltr = layout_ltr.texts();
    let t_rect = texts_ltr.iter().find(|(n, _)| *n == 2).unwrap().1;
    let s_rect = texts_ltr.iter().find(|(n, _)| *n == 3).unwrap().1;
    assert!(
        (t_rect.0 - s_rect.0).abs() < 0.05,
        "LTR squeezed start edges must match: title={t_rect:?} sub={s_rect:?}"
    );

    // RTL header with mixed LTR subtitle: physical right edges align at width - x.
    let rtl = Top {
        direction: exact_kernel::Direction::Rtl,
        title: "عنوان طويل يملأ شريط التطبيق تقريباً".into(),
        title_direction: exact_kernel::Direction::Rtl,
        subtitle: "Android 16".into(),
        subtitle_direction: exact_kernel::Direction::Ltr,
        trailing: trailing.clone(),
        ..ltr.clone()
    };
    let layout_rtl = Layout::top(&mut engine, &rtl, &inputs);
    let texts_rtl = layout_rtl.texts();
    let rt_rect = texts_rtl.iter().find(|(n, _)| *n == 2).unwrap().1;
    let rs_rect = texts_rtl.iter().find(|(n, _)| *n == 3).unwrap().1;
    assert!(
        ((rt_rect.0 + rt_rect.2) - (rs_rect.0 + rs_rect.2)).abs() < 0.1,
        "RTL start (physical right) edges must match: title={rt_rect:?} sub={rs_rect:?}"
    );

    // Narrow bar where allocated room (10 dp) is smaller than even the ellipsis advance:
    // both LTR and RTL clip painted glyphs and accessibility bounds strictly clear of trailing actions.
    let two_trailing = vec![
        trailing[0].clone(),
        Button {
            node: 5,
            ..trailing[0].clone()
        },
    ];
    let narrow_inputs = super::look::Inputs {
        width: 170.0,
        ..inputs
    };
    for dir in [exact_kernel::Direction::Ltr, exact_kernel::Direction::Rtl] {
        let narrow = Top {
            direction: dir,
            up: true,
            trailing: two_trailing.clone(),
            ..ltr.clone()
        };
        let layout = Layout::top(&mut engine, &narrow, &narrow_inputs);
        let first_trailing = layout
            .targets
            .iter()
            .find(|t| t.what == What::Item(4))
            .unwrap()
            .rect;
        for (node, rect) in layout.texts() {
            assert!(
                rect.2 <= 10.05,
                "narrow {dir:?} text {node} width must clip to 10 dp room: {rect:?}"
            );
            if dir == exact_kernel::Direction::Ltr {
                assert!(
                    rect.0 + rect.2 <= first_trailing.0 - 3.9,
                    "narrow LTR text {node} must not cross into trailing action: {rect:?} vs {first_trailing:?}"
                );
            } else {
                assert!(
                    rect.0 >= first_trailing.0 + first_trailing.2 + 3.9,
                    "narrow RTL text {node} must not cross into trailing action: {rect:?} vs {first_trailing:?}"
                );
            }
        }
        let ops = Recorder::record_surface(1.0, |r| layout.paint(r, &mut engine, None));
        let mut clips = 0usize;
        let mut glyphs = 0usize;
        let mut i = 0;
        while i < ops.len() {
            if ops[i] == CLIP_RRECT {
                clips += 1;
            } else if ops[i] == GLYPHS {
                glyphs += 1;
            }
            i = stream::end(&ops, i).expect("well-formed op");
        }
        assert_eq!(glyphs, 2, "both title and subtitle emit glyphs");
        assert!(
            clips >= 2,
            "title and subtitle each paint inside a column clip"
        );
    }

    // Presenter-driven LTR header with an RTL subtitle preserves direction and projects cleanly.
    let rtl_sub_src = DETAIL_APP
        .replace(
            r#"state sub = "Android 16 · Pixel 7""#,
            r#"state sub = "تفاصيل البناء""#,
        )
        .replace(
            r#"text sub testId="detail-subtitle""#,
            r#"text sub direction="rtl" testId="detail-subtitle""#,
        );
    let (mut p_rtl, _) = barred(&rtl_sub_src);
    tap(&mut p_rtl, "open-detail").unwrap();
    assert_eq!(p_rtl.bars_json()["top"]["subtitle"], "تفاصيل البناء");
    assert_eq!(
        p_rtl.bar_plans().0.unwrap().subtitle_direction,
        exact_kernel::Direction::Rtl
    );
}
