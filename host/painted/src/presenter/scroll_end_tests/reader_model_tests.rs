use super::*;

#[test]
fn a_non_anchor_model_picture_does_not_delay_reader_completions() {
    reader_completions_during_model_picture(false);
}

#[test]
fn a_non_anchor_model_picture_does_not_delay_preserved_reader_completions() {
    reader_completions_during_model_picture(true);
}

fn reader_completions_during_model_picture(preserved: bool) {
    let mut p = boot_source(
        r#"component App
  state top = 0
  state ends = 0
  state endedAt = 0
  state order = ""
  action moved(x: number, y: number)
    order = `${order}s`
  action ended
    ends = ends + 1
    endedAt = performanceNow()
    order = `${order}e`
  action jump
    top = 100
  view
    column
      button "Jump" press=jump testId="jump"
      list virtualized=true width=200 height=100 scrollTop=top testId="port" scroll=moved scrollend=ended
        for x in [0, 1, 2, 3, 4, 5, 6, 7, 8, 9] key=x
          box height=100
"#,
    );
    assert_eq!(p.settle_collections(), None);
    let first = p.display_frame().unwrap();
    assert!(p.display_complete(&first));
    let port = named(&p, "port");
    let b = p.box_of(port).unwrap();
    let rev = p.reader_under(b.rect.0 + 10., b.rect.1 + 10., 1.)[5] as u32;
    assert_eq!(
        p.reader_sample_at(port, (0., 20.), rev, if preserved { 4850. } else { 5000. }),
        Some((0., 20.))
    );
    if preserved {
        assert_eq!(
            p.reader_sample_at(port, (0., 20.), rev, 5000.),
            Some((0., 20.))
        );
    }
    assert_eq!(p.host.now(), 0.);
    press(&mut p, "jump", 50.);
    assert_eq!(p.settle_collections(), None);
    let acknowledged = if preserved { 40. } else { 20. };
    let scrolls = if preserved { "ss" } else { "s" };
    assert_eq!(p.scroll_of(port).1, acknowledged);
    assert!(p.pending_model_scroll(port).is_some());
    assert!(!p.anchor_scroll_pending(port));
    let jump = p.display_frame().unwrap();
    let first_due = if preserved { 4950. } else { 5100. };
    assert_eq!(p.timer_or_scroll_due_ms(), Some(first_due));
    assert_eq!(p.clock(first_due - 1.).1, None);
    assert_eq!(number(&p, "ends"), 0.);
    assert_eq!(p.clock(first_due).1, None);
    assert_eq!(number(&p, "ends"), 1.);
    assert_eq!(number(&p, "endedAt"), first_due);
    assert_eq!(text(&p, "order"), format!("{scrolls}e"));
    if preserved {
        assert_eq!(p.timer_or_scroll_due_ms(), Some(5100.));
        assert_eq!(p.clock(5100.).1, None);
    }
    let reader_ends = if preserved { 2. } else { 1. };
    let before_ack = if preserved { "ssee" } else { "se" };
    assert_eq!(number(&p, "ends"), reader_ends);
    assert_eq!(number(&p, "endedAt"), 5100.);
    assert_eq!(text(&p, "order"), before_ack);
    assert_eq!(p.timer_or_scroll_due_ms(), None);
    assert_eq!(p.scroll_of(port).1, acknowledged);
    assert_eq!(p.clock(5200.).1, None);
    assert!(p.display_complete(&jump));
    assert_eq!(p.scroll_of(port).1, 100.);
    assert_eq!(p.clock(5200.).1, None);
    assert_eq!(number(&p, "ends"), reader_ends + 1.);
    assert_eq!(number(&p, "endedAt"), 5200.);
    assert_eq!(text(&p, "order"), format!("{before_ack}se"));
    assert_eq!(p.clock(6000.).1, None);
    assert_eq!(number(&p, "ends"), reader_ends + 1.);
    assert_eq!(p.timer_or_scroll_due_ms(), None);
}
