//! Inline geometry follows the acknowledged paragraph and its identities.
use super::*;

#[test]
fn inline_geometry_keeps_sibling_ownership_and_placement_until_successful_ack() {
    let source = format!(
        r#"component App
  state showing = true
  action hide
    showing = false
  view
    column
      box press=hide testId="hide" height=20
      scroll testId="port" width=200 height=100
        text font-size=16 line-height="20px" white-space="pre-wrap" padding=(showing ? 8 : 0)
          if showing
            text "{}"
          text "Tail" id="tail" testId="tail"
"#,
        "Line\\n".repeat(12)
    );
    let (mut p, _, fail) = boot_app(&source);
    let first = submit(&mut p).unwrap();
    assert!(complete(&mut p, &first));
    let tail = id(&p, "tail");
    let before = p.rect_of(tail).unwrap();
    assert!(before.1 > 200., "{before:?}");
    let key = p.host.kernel().node(tail).unwrap().key;
    let parent = p.host.kernel().node(tail).unwrap().parent;
    let pending = submit(&mut p).unwrap();
    viewport_action(&mut p, "hide");
    assert_eq!(p.host.kernel().node(tail).unwrap().key, key);
    assert_eq!(p.host.kernel().node(tail).unwrap().parent, parent);
    assert_eq!(p.rect_of(tail), Some(before), "pending sibling removal");
    assert!(complete(&mut p, &pending));
    assert_eq!(
        p.rect_of(tail),
        Some(before),
        "acknowledging older siblings"
    );
    fail.set(true);
    let failed = submit(&mut p).unwrap();
    assert!(!p.last_frame_succeeded);
    assert_eq!(p.rect_of(tail), Some(before));
    assert!(complete(&mut p, &failed));
    assert_eq!(p.rect_of(tail), Some(before), "failed replacement");
    p.scroll_element_into_view(&["tail".into(), "nearest".into()]);
    assert!(p.scroll_of(id(&p, "port")).1 > 100.);
    fail.set(false);
    let next = submit(&mut p).unwrap();
    assert_eq!(p.rect_of(tail), Some(before), "unacknowledged replacement");
    assert!(complete(&mut p, &next));
    let after = p.rect_of(tail).unwrap();
    assert!(after.0 < before.0 && after.1 < 40., "{after:?}");
}

#[test]
fn inline_geometry_keeps_canonical_run_indices_after_an_image() {
    let source = r#"component App
  view
    text font-size=16 line-height="20px"
      image width=32 height=16
      text "Tail" testId="tail"
"#;
    for display in [false, true] {
        let (mut p, _, _) = boot_app(source);
        if display {
            let frame = submit(&mut p).unwrap();
            assert!(complete(&mut p, &frame));
        }
        let tail = id(&p, "tail");
        let rect = p.rect_of(tail).expect("text after an image has geometry");
        assert!((rect.0 - 32.).abs() < 0.01 && rect.2 > 20., "{rect:?}");
    }
}

#[test]
fn inline_witness_and_scroll_geometry_survive_spatial_transforms() {
    for attrs in ["translate=\"0 0 10px\"", "rotate=\"x 30deg\""] {
        projected_inline(attrs);
    }
}

fn projected_inline(attrs: &str) {
    let source = format!(
        r#"component App
  view
    scroll testId="port" width=200 height=100
      column testId="container" width=200 height=300
        text testId="paragraph" font-size=16 line-height="20px" white-space="pre-wrap" {attrs} transform-origin="0 0"
          text "{}"
          text "Tail" id="tail" testId="tail"
"#,
        "Line\\n".repeat(12)
    );
    let (mut p, _, _) = boot_app(&source);
    let first = submit(&mut p).unwrap();
    assert!(complete(&mut p, &first));
    let tail = id(&p, "tail");
    assert!(p.box_of(id(&p, "paragraph")).unwrap().projected());
    let before = p.rect_of(tail).expect(attrs);
    assert!(before.1 > 150., "{attrs}: {before:?}");
    let pending = submit(&mut p).unwrap();
    p.scroll_element_into_view(&["tail".into(), "nearest".into()]);
    let offset = p.scroll_of(id(&p, "port"));
    assert!(offset.1 > 100., "{attrs}: {offset:?}");
    assert!(complete(&mut p, &pending));
    let scrolled = submit(&mut p).unwrap();
    assert!(complete(&mut p, &scrolled));
    let rect = p.rect_of(tail).unwrap();
    assert!(
        rect.1 >= 0. && rect.1 + rect.3 <= 100.01,
        "{attrs}: {rect:?}"
    );
    p.scroll_element_into_view(&["tail".into(), "nearest".into()]);
    assert_eq!(p.scroll_of(id(&p, "port")), offset, "{attrs}");
}

#[test]
fn inline_frame_failed_submission_keeps_the_acknowledged_paragraph() {
    use exact_runner::geometry::InlineGeometry;
    let source = r#"component App
  state large = false
  action change
    large = true
  view
    column
      button testId="change" press=change
        text "change"
      text width=300
        text "Hi" testId="run" font-size=(large ? 32 : 16)
"#;
    let (mut p, _, fail) = boot_app(source);
    let first = submit(&mut p).unwrap();
    assert!(complete(&mut p, &first));
    let run = id(&p, "run");
    let before = p.ranges.frame(run);
    assert!(!before.unavailable);
    let rect = p.rect_of(run).unwrap();
    let change = id(&p, "change");
    assert!(p.host.dispatch_at(change, Event::Press, 1.).is_none());
    assert!(p.after_commit().is_none());
    fail.set(true);
    let failed = submit(&mut p).unwrap();
    assert!(!p.last_frame_succeeded);
    assert_eq!(p.ranges.frame(run), before);
    assert!(complete(&mut p, &failed));
    assert_eq!(p.ranges.frame(run), before);
    assert_eq!(p.rect_of(run), Some(rect));
    fail.set(false);
    let next = submit(&mut p).unwrap();
    assert!(complete(&mut p, &next));
    let after = p.ranges.frame(run);
    assert!(!after.unavailable);
    assert!((after.height - before.height * 2.).abs() < 0.001);
    assert!((p.rect_of(run).unwrap().3 - rect.3 * 2.).abs() < 0.001);
}

#[test]
fn inline_witness_allows_geometry_and_scroll_into_view_after_acknowledgement() {
    let source = format!(
        r#"component App
  view
    scroll testId="port" width=200 height=100
      text font-size=16 line-height="20px" white-space="pre-wrap"
        text "{}"
        text testId="group" id="group"
          text "Tail" id="tail" testId="tail"
"#,
        "Line\\n".repeat(12)
    );
    for pending in [false, true] {
        let (mut p, _, _) = boot_app(&source);
        let first = submit(&mut p).unwrap();
        assert!(complete(&mut p, &first));
        let tail = id(&p, "tail");
        let group = id(&p, "group");
        assert!(p.inline_box(tail).is_some());
        assert!(p.boxes.iter().all(|b| b.id != tail && b.id != group));
        let next = pending.then(|| submit(&mut p).unwrap());
        assert!(p.rect_of(tail).is_some(), "the acknowledged inline leaf");
        assert!(p.rect_of(group).is_some(), "the inline ancestor");
        p.scroll_element_into_view(&["tail".into(), "nearest".into()]);
        let port = id(&p, "port");
        let offset = p.scroll_of(port);
        assert!(offset.1 > 100., "pending={pending}: {offset:?}");
        if let Some(next) = next {
            assert!(complete(&mut p, &next));
        }
        let scrolled = submit(&mut p).unwrap();
        assert!(complete(&mut p, &scrolled));
        let (_, y, _, height) = p.box_of(tail).unwrap().rect;
        assert!(y >= 0. && y + height <= 100.01, "{y}, {height}");
        p.scroll_element_into_view(&["group".into(), "nearest".into()]);
        assert_eq!(p.scroll_of(port), offset);
    }
}

const INLINE_APP: &str = r#"component App
  state showing = true
  action hide
    showing = false
  action show
    showing = true
  view
    column
      box press=hide testId="hide" height=20
      box press=show testId="show" height=20
      text testId="paragraph" font-size=16
        text "Before "
        if showing
          text testId="group"
            text "Tail" id="tail" testId="tail"
      text testId="other" font-size=16
        text "Other "
"#;

#[test]
fn inline_witness_failed_and_pending_frames_cannot_authorize_new_children() {
    let (mut p, _, fail) =
        boot_app(&INLINE_APP.replace("state showing = true", "state showing = false"));
    let first = submit(&mut p).unwrap();
    assert!(complete(&mut p, &first));
    viewport_action(&mut p, "show");
    let tail = id(&p, "tail");
    fail.set(true);
    let failed = submit(&mut p).unwrap();
    assert!(!p.last_frame_succeeded);
    assert_eq!(p.rect_of(tail), None);
    assert!(complete(&mut p, &failed));
    assert_eq!(p.rect_of(tail), None);
    fail.set(false);
    let next = submit(&mut p).unwrap();
    assert_eq!(p.rect_of(tail), None, "submission alone authorizes nothing");
    assert!(complete(&mut p, &next));
    assert!(p.rect_of(tail).is_some());
}

#[test]
fn inline_witness_obsolete_frames_cannot_authorize_replaced_children() {
    for reload in [false, true] {
        let (mut p, _, _) = boot_app(INLINE_APP);
        let first = submit(&mut p).unwrap();
        assert!(complete(&mut p, &first));
        let tail = id(&p, "tail");
        assert!(p.rect_of(tail).is_some());
        let key = p.host.kernel().node(tail).unwrap().key;
        let obsolete = submit(&mut p).unwrap();
        if reload {
            p.reload(&crate::test_plan(INLINE_APP), Empty).unwrap();
            assert_eq!(id(&p, "tail"), tail, "reload reuses the wire id");
            assert_eq!(p.host.kernel().node(tail).unwrap().key, key);
        } else {
            viewport_action(&mut p, "hide");
            viewport_action(&mut p, "show");
            let current = id(&p, "tail");
            assert_ne!(p.host.kernel().node(current).unwrap().key, key);
        }
        let current = id(&p, "tail");
        assert_eq!(p.rect_of(current), None);
        assert!(complete(&mut p, &obsolete));
        assert_eq!(p.rect_of(current), None);
        let next = submit(&mut p).unwrap();
        assert!(complete(&mut p, &next));
        assert!(p.rect_of(current).is_some());
    }
}

#[test]
fn inline_witness_reparented_ancestors_require_a_new_acknowledgement() {
    use exact_kernel::Op;
    let (mut p, _, _) = boot_app(INLINE_APP);
    let first = submit(&mut p).unwrap();
    assert!(complete(&mut p, &first));
    let tail = id(&p, "tail");
    let group = id(&p, "group");
    assert!(p.rect_of(tail).is_some());
    let key = p.host.kernel().node(tail).unwrap().key;
    let parent = p.host.kernel().node(tail).unwrap().parent;
    let obsolete = submit(&mut p).unwrap();
    let paragraph = id(&p, "paragraph");
    let other = id(&p, "other");
    let from = p
        .host
        .kernel()
        .node(paragraph)
        .unwrap()
        .children()
        .into_iter()
        .filter(|id| *id != group)
        .collect();
    let mut to = p.host.kernel().node(other).unwrap().children();
    to.push(group);
    let receipt = p
        .host
        .runner_mut()
        .kernel_mut()
        .apply(
            0,
            0,
            &[
                Op::SetChildren {
                    id: paragraph,
                    children: from,
                },
                Op::SetChildren {
                    id: other,
                    children: to,
                },
            ],
        )
        .unwrap();
    let now = p.host.now();
    p.host.row_dirty.commit(&receipt, now);
    p.host.layout().unwrap();
    p.dirty = true;
    assert_eq!(p.host.kernel().node(tail).unwrap().key, key);
    assert_eq!(p.host.kernel().node(tail).unwrap().parent, parent);
    assert_eq!(p.rect_of(group), None);
    assert_eq!(
        p.rect_of(tail),
        None,
        "its ancestor moved to another paragraph"
    );
    assert!(complete(&mut p, &obsolete));
    assert_eq!(p.rect_of(tail), None);
    let next = submit(&mut p).unwrap();
    assert!(complete(&mut p, &next));
    assert!(p.rect_of(group).is_some());
    assert!(p.rect_of(tail).is_some());
}
