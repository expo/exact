use super::*;

#[test]
fn crossing_the_top_bars_scroll_threshold_needs_publication() {
    let (mut p, _) = barred(APP);
    let list = id(&p, "list");
    p.reader_bars(1.0).unwrap();
    for (offset, changed) in [(0.5, false), (0.75, true), (1.0, false), (0.5, true)] {
        assert_eq!(p.scroll_to(list, (0.0, offset)), Some((0.0, offset)));
        assert_eq!(p.bar_scroll_changed(), changed, "offset {offset}");
        assert_eq!(p.reader_bars(1.0).is_some(), changed);
        assert_eq!(top(&mut p).scrolled, offset > 0.5);
        assert!(!p.bar_scroll_changed());
    }
}

#[test]
fn page_scroll_does_not_publish_bars_without_a_top_bar() {
    let (mut p, _) = barred("component App\n  view\n    box height=2000\n");
    p.reader_bars(1.0).unwrap();
    assert_eq!(
        p.scroll_to(crate::presenter::PAGE, (0., 10.)),
        Some((0., 10.))
    );
    assert!(!p.bar_scroll_changed());
    assert_eq!(p.reader_bars(1.0), None);
}
