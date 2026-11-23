//! The reader scrolls by itself (LLP 1115.001 D7.1 to D7.3; the boundary is
//! `presenter/scrollers.rs`): a finger's drag and its fling are the
//! reader's, with Android's own classes. It asks what is under a touch,
//! moves the scroller's rows itself where the stream gave them a node of
//! their own, and says here how far it moved which scroller. After each
//! frame it is told where the scrollers it knows are and how far they go,
//! and whether the app moved one, which ends its fling.
//!
//! The feed is the scroller a finger last moved up or down (or the one a
//! driver's `scroll` found): the one whose travel the windows lead by and
//! whose rows the collection passes are timed for (`refine.rs`).

#[cfg(target_os = "android")]
use super::{CanvasHost, Section, GROUPS};
#[cfg(target_os = "android")]
use crate::presenter::PAGE;
#[cfg(target_os = "android")]
use exact_runner::DataSource;
#[cfg(any(target_os = "android", test))]
use std::collections::BTreeMap;

/// A move-only frame can move just the offsets with a recorded group.
#[cfg(any(target_os = "android", test))]
pub(super) fn shift_groups(
    before: &BTreeMap<u32, (f32, f32)>,
    now: &BTreeMap<u32, (f32, f32)>,
    groups: &[(u32, (f32, f32))],
    scale: f32,
) -> Option<Vec<u32>> {
    let others = |offsets: &BTreeMap<u32, (f32, f32)>| {
        offsets
            .iter()
            .filter(|(id, _)| !groups.iter().any(|(g, _)| g == *id))
            .map(|(id, o)| (*id, o.0.to_bits(), o.1.to_bits()))
            .collect::<Vec<_>>()
    };
    if others(now) != others(before) {
        return None;
    }
    let mut ops = vec![super::SHIFT, groups.len() as u32];
    for (id, at) in groups {
        let to = now.get(id).copied().unwrap_or_default();
        let shift = crate::paint::grid::pixel_delta(*at, to, scale);
        ops.extend([*id, shift.0.to_bits(), shift.1.to_bits()]);
    }
    Some(ops)
}

/// Scroll deferral state shared by the Android canvas loop's `frame`,
/// `requests`, `refine`, and input handlers: `scrolled` stays true until
/// `refine` runs or non-scroll input ends the scroll, while `scroll_turn` is
/// true only for the scroll step's own frame and requests turn so later turns
/// still pump and tell the shell (including `Request::Back`).
#[cfg(any(target_os = "android", test))]
#[derive(Default)]
pub(super) struct ScrollHold {
    scrolled: bool,
    scroll_turn: bool,
}

#[cfg(any(target_os = "android", test))]
impl ScrollHold {
    pub(super) fn note_scroll(&mut self, prefetching: bool) {
        self.scrolled = prefetching;
        self.scroll_turn = prefetching;
    }
    pub(super) fn end(&mut self) {
        self.scrolled = false;
        self.scroll_turn = false;
    }
    pub(super) fn touch(&mut self, action: i32) {
        self.scroll_turn = false;
        if action != 2 {
            self.scrolled = false;
        }
    }
    pub(super) fn poll(&mut self) {
        self.scroll_turn = false;
    }
    pub(super) fn holding_collections(&self) -> bool {
        self.scrolled
    }
    pub(super) fn quick(&self) -> bool {
        self.scroll_turn
    }
    pub(super) fn take_request_skip(&mut self) -> bool {
        std::mem::take(&mut self.scroll_turn)
    }
    pub(super) fn owed(&self) -> bool {
        self.scrolled
    }
}

#[cfg(target_os = "android")]
impl<D: DataSource + Default> CanvasHost<D> {
    /// What is under a point of the view (pixels), for the reader to decide
    /// what a drag from there is: the scrollers, innermost first, and the
    /// axes the presenter's own gestures claim
    /// ([`crate::presenter::Presenter::reader_under`] has the words); then
    /// the module under it (thirteen words), and each of those scrollers'
    /// snap ([`crate::presenter::Presenter::reader_snaps`]).
    pub fn under(&mut self, x: f32, y: f32) -> Vec<i32> {
        let mut words = self.p.reader_under(x, y, self.scale);
        words.extend(self.p.reader_module(x, y, self.scale));
        let ids: Vec<_> = (0..words[1] as usize)
            .map(|k| words[3 + 11 * k] as exact_kernel::ViewId)
            .collect();
        words.extend(self.p.reader_snaps(&ids, self.scale));
        words
    }

    /// List `id`'s snap areas around a release's projection, in pixels
    /// ([`crate::presenter::Presenter::reader_snaps_around`] has the words).
    pub fn snaps_around(
        &mut self,
        id: u32,
        vertical: bool,
        release: f32,
        projected: f32,
    ) -> Vec<i32> {
        self.p
            .reader_snaps_around(id, vertical, release, projected, self.scale)
    }

    /// The native drag's sheet position and release velocity, in window pixels.
    pub fn sheet_drag(&mut self, index: u32, generation: u32, top: f32, release: Option<f32>) {
        let now = self.now();
        self.p.reader_sheet_drag(
            exact_kernel::NodeKey { index, generation },
            top / self.scale,
            release.map(|velocity| velocity / self.scale),
            now,
        );
    }

    /// The reader moved scroller `id` by (`dx`, `dy`) pixels, knowing
    /// revision `rev` of it. Its rows mount after the frame, as a driver's
    /// scroll leaves them.
    pub fn scrolled(&mut self, id: u32, dx: f32, dy: f32, rev: u32) {
        let _s = Section::begin(c"exact scroll");
        let s = self.scale;
        self.p.hold_collections(self.prefetching);
        let at = self.now();
        let went = self.p.reader_sample_at(id, (dx / s, dy / s), rev, at);
        self.p.hold_collections(false);
        let Some((_, down)) = went else {
            return;
        };
        self.scroll_hold.note_scroll(self.prefetching);
        self.scrolled_at = at;
        if down != 0.0 && id != PAGE {
            if self.feed != Some(id) {
                self.feed = Some(id);
                self.travel = crate::travel::Travel::default();
            }
            self.travel.scrolled(down, self.scrolled_at);
        }
    }

    /// The reader's finger began dragging scroller `id`: its
    /// `scrolldragstart` ([`crate::presenter::Presenter::reader_drag_began`]).
    pub fn drag_began(&mut self, id: u32) {
        if let Some(error) = self.p.reader_drag_began(id) {
            self.p.host_mut().log(error);
        }
    }

    /// The finger let go of scroller `id`: the platform's velocity
    /// (`vx`, `vy`, pixels per second, positive as the offset grows) and,
    /// with `rest`, where its fling will stop (`OverScroller.getFinalX/Y`),
    /// in pixels, and whether it flings ([`crate::presenter::Presenter::reader_drag_ended`]).
    pub fn drag_ended(
        &mut self,
        id: u32,
        vx: f32,
        vy: f32,
        rest: Option<(f32, f32)>,
        flings: bool,
    ) {
        let s = self.scale;
        let velocity = ((vx / s) as f64, (vy / s) as f64);
        let rest = rest.map(|(x, y)| (x / s, y / s));
        if let Some(error) = self.p.reader_drag_ended(id, velocity, rest, flings) {
            self.p.host_mut().log(error);
        }
    }

    /// The fling the reader threw from its drag of scroller `id` stopped
    /// ([`crate::presenter::Presenter::reader_fling_ended`]).
    pub fn fling_ended(&mut self, id: u32) {
        self.p.reader_fling_ended(id);
    }

    /// The reader's pull to refresh of scroller `id` was let go past its
    /// threshold ([`crate::presenter::Presenter::reader_refresh`]).
    pub fn refresh(&mut self, id: u32) {
        if let Some(error) = self.p.reader_refresh(id) {
            self.p.host_mut().log(error);
        }
    }

    /// The reader's pinch of zoomable scroller `id` (LLP 1057.001.000 §5):
    /// `phase` 0 began, 1 scaled by `factor` while its focal point moved
    /// from `from` to `to`, 2 ended; in pixels.
    pub fn zoom(&mut self, id: u32, phase: i32, factor: f32, from: (f32, f32), to: (f32, f32)) {
        let s = self.scale;
        match phase {
            0 => self.p.zoom_pinch_began(id),
            1 => self
                .p
                .zoom_pinched(id, factor, (from.0 / s, from.1 / s), (to.0 / s, to.1 / s)),
            _ => self.p.zoom_pinch_ended(id),
        }
    }

    /// The reader's double tap at (`x`, `y`) pixels: `phase` 0 its second
    /// tap came down, 1 it lifted ([`crate::presenter::Presenter::zoom_tap_down`],
    /// [`crate::presenter::Presenter::zoom_double_tap`]).
    pub fn zoom_tap(&mut self, phase: i32, x: f32, y: f32) {
        let at = (x / self.scale, y / self.scale);
        if phase == 0 {
            self.p.zoom_tap_down(at);
        } else {
            self.p.zoom_double_tap(at);
        }
    }

    /// A wheel with `ctrl` held at (`x`, `y`) pixels, `dy` points toward
    /// the end: a trackpad's or a mouse's pinch, about the pointer.
    pub fn zoom_wheel(&mut self, x: f32, y: f32, dy: f32) {
        let s = self.scale;
        self.p.wheel_zoom(x / s, y / s, dy);
    }

    /// What the reader is told of the scrollers it knows, when that changes.
    /// The recording's scale precedes the words from
    /// [`crate::presenter::Presenter::reader_scrollers`].
    pub fn scrollers(&mut self) -> Option<Vec<i32>> {
        let scale = self.scale;
        let mut words = GROUPS.with(|g| self.p.reader_scrollers(&g.borrow(), scale))?;
        words.insert(0, scale.to_bits() as i32);
        Some(words)
    }

    /// Before a driver's scroll: a feed whose node is gone (its page was
    /// left) is found again, and the one it moves is the driver's from now
    /// on, not the reader's.
    pub(super) fn driven(&mut self) {
        if (self.feed).is_some_and(|id| self.p.host().kernel().node(id).is_none()) {
            self.feed = None;
        }
        if let Some(id) = self.feed {
            self.p.reader_forgets(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::presenter::{PainterChoice, Presenter};

    fn props(p: &mut Presenter<()>) -> serde_json::Value {
        p.frame();
        let ops = super::super::FINISHED
            .with(|f| f.borrow_mut().take())
            .unwrap();
        let mut at = 0;
        while at < ops.len() {
            if ops[at] == super::super::NATIVE && ops[at + 2] == 2 {
                let len = ops[at + 15] as usize;
                let bytes: Vec<_> = ops[at + 16..at + 16 + len.div_ceil(4)]
                    .iter()
                    .flat_map(|word| word.to_le_bytes())
                    .take(len)
                    .collect();
                return serde_json::from_slice(&bytes).unwrap();
            }
            at = super::super::stream::end(&ops, at).unwrap();
        }
        panic!("module view was not painted");
    }

    #[test]
    fn module_props_and_hit_answers_identify_the_session_despite_reload_collisions() {
        let source = "component App\n  view\n    native-menu width=100 height=40 testId=\"menu\"\n";
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(source),
            (),
            (200., 200.),
            2.,
            std::path::PathBuf::new(),
            PainterChoice::Canvas,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.brush.native_session = 0x1234_5678_fedc_ba98;
        let before = props(&mut p);
        let hit = p.reader_module(20., 20., 2.);
        let token = |words: [i32; 13]| words[8] as u32 as u64 | ((words[9] as u32 as u64) << 32);
        assert_ne!(hit[0], 0);
        assert_eq!(hit[5] as u32 as u64, before["slot"].as_u64().unwrap());
        assert_eq!(hit[6] as u32 as u64, before["generation"].as_u64().unwrap());
        assert_eq!(token(hit), before["session"].as_u64().unwrap());
        assert_eq!(f32::from_bits(hit[3] as u32), 200.);
        assert!(
            p.reload(&crate::test_plan(&source.replace("menu\"", "other\"")), ())
                .unwrap()
                == crate::presenter::ReloadResult::Applied(None)
        );
        let after = props(&mut p);
        let next = p.reader_module(20., 20., 2.);
        assert_eq!(hit[..8], next[..8]);
        for key in ["module", "slot", "generation"] {
            assert_eq!(before[key], after[key]);
        }
        assert_ne!(before["session"], after["session"]);
        assert_eq!(token(next), after["session"].as_u64().unwrap());
    }

    /// Diary 1ef6ac05: a plain `<scroll>` scroll must not leave `tell_shell()`
    /// suppressed on subsequent turns when `refine` did not immediately run.
    #[test]
    fn scroll_hold_skips_only_the_scroll_turn_and_clears_on_input_or_refine() {
        let mut hold = super::ScrollHold::default();
        assert!(!hold.quick() && !hold.owed() && !hold.take_request_skip());
        // Before `refine` has ever run (`prefetching == false`), scrolling defers nothing.
        hold.note_scroll(false);
        assert!(!hold.quick() && !hold.owed() && !hold.take_request_skip());
        // After a plain `<scroll>` step once prefetching is active:
        hold.note_scroll(true);
        assert!(hold.quick() && hold.holding_collections() && hold.owed());
        // The scroll step's own requests() skips tell_shell() once, then releases scroll_turn.
        assert!(hold.take_request_skip());
        assert!(
            !hold.quick(),
            "the following frame is no longer a quick scroll frame"
        );
        assert!(
            !hold.take_request_skip(),
            "the following requests() tells the shell"
        );
        assert!(hold.owed(), "refine() is still owed to settle the scroll");
        // Non-move touch (down/up/cancel) clears both flags; MOVE keeps collection deferral.
        hold.note_scroll(true);
        hold.touch(2);
        assert!(!hold.quick() && hold.holding_collections());
        hold.touch(0);
        assert!(!hold.quick() && !hold.holding_collections() && !hold.owed());
        // Non-scroll input (`back`, `key`, `bar`, `refine`) clears both flags.
        hold.note_scroll(true);
        hold.end();
        assert!(!hold.quick() && !hold.holding_collections() && !hold.owed());
        // `poll` ends the scroll turn without clearing pending collection deferral.
        hold.note_scroll(true);
        hold.poll();
        assert!(!hold.quick() && !hold.take_request_skip() && hold.owed());
    }

    #[cfg(not(windows))]
    #[test]
    fn kept_rows_and_shift_cross_the_readers_fractional_pixel_boundary_together() {
        use super::super::{stream, FINISHED, GROUPS, ROW_DRAW, SHIFT};

        let scale = 2.75;
        let mut p = boot_at(
            r#"component App
  view
    scroll width=200 height=100 scrollTop=7.444 testId="port"
      column
        box width=200 height=30 background-color="red"
        box width=200 height=200 background-color="blue"
"#,
            scale,
        );
        let port = id(&p, "port");
        let before = super::BTreeMap::from([(port, p.scroll_of(port))]);
        let groups = GROUPS.with(|g| g.borrow().clone());
        assert_eq!(groups.len(), 1);
        let row_top = || {
            let ops = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
            let mut at = 0;
            while ops[at] != ROW_DRAW {
                at = stream::end(&ops, at).unwrap();
            }
            f32::from_bits(ops[at + 3])
        };
        let top = row_top();
        let words = p.reader_under(10., 10., scale);
        let start = before[&port].1;
        assert_eq!(f32::from_bits(words[7] as u32), start * scale);
        let rev = words[5] as u32;
        let target = 7.451_704_5_f32;
        p.reader_sample_at(port, (0., target - start), rev, 1.)
            .unwrap();
        let words = p.reader_scrollers(&groups, scale).unwrap();
        assert_eq!(f32::from_bits(words[4] as u32), target);
        assert_eq!(f32::from_bits(words[6] as u32), 20.);
        let now = super::BTreeMap::from([(port, p.scroll_of(port))]);
        let shift = super::shift_groups(&before, &now, &groups, scale).unwrap();
        assert_eq!(shift, [SHIFT, 1, port, 0, (-1_f32).to_bits()]);
        p.frame();
        assert_eq!(row_top() - top, -1.);
    }

    #[test]
    fn equal_layout_edge_and_scroll_offset_cancel() {
        let mut p = boot_at(
            r#"component App
  view
    scroll width=200 height=100 scrollTop=7.4517045
      box height=7.4517045
      box width=200 height=150 background-color="red" testId="row"
"#,
            2.75,
        );
        assert_eq!(p.rect_of(id(&p, "row")).unwrap().1, 0.);
    }

    #[test]
    fn sticky_under_a_projected_wrapper_uses_the_ports_drawn_scroll() {
        let mut p = boot_at(
            r#"component App
  view
    scroll width=200 height=100 scrollTop=0.75
      box width=200 height=200 translate="0px 0px 1px"
        box width=200 height=20 position="sticky" top=0 background-color="red" testId="heading"
"#,
            1.,
        );
        assert_eq!(p.rect_of(id(&p, "heading")).unwrap().1, 0.);
    }

    #[test]
    fn rows_touch_at_a_scroll_offset_between_pixels() {
        use super::super::{stream, FINISHED, ROW_BEGIN, ROW_DRAW, RRECT};
        // An offset the app set: 100 px is 262.5 pixels at 2.625 a px. Each
        // row rounded where it fell, a float's last bits put neighbours a
        // pixel apart or over each other.
        let row = "        box height=20.3 width=200 background-color=\"#2468ac\"\n".repeat(40);
        let source = format!(
            "component App\n  view\n    scroll width=200 height=200 scrollTop=100\n      column\n{row}"
        );
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(&source),
            (),
            (200., 200.),
            2.625,
            std::path::PathBuf::new(),
            PainterChoice::Canvas,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.frame();
        let ops = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
        let (mut tops, mut heights, mut at) = (Vec::new(), Vec::new(), 0);
        while at < ops.len() {
            at = match ops[at] {
                ROW_BEGIN => at + 6,
                RRECT => {
                    heights.push((f32::from_bits(ops[at + 5]) * 2.625).round());
                    stream::end(&ops, at).unwrap()
                }
                ROW_DRAW => {
                    tops.push(f32::from_bits(ops[at + 3]));
                    at + 4
                }
                _ => stream::end(&ops, at).unwrap(),
            };
        }
        assert!(
            tops.len() > 4 && tops.len() == heights.len(),
            "{tops:?} {heights:?}"
        );
        for i in 1..tops.len() {
            assert_eq!(tops[i] - tops[i - 1], heights[i - 1], "row {i}: {tops:?}");
        }
    }

    fn boot(source: &str) -> Presenter<()> {
        boot_at(source, 2.625)
    }

    fn boot_at(source: &str, scale: f32) -> Presenter<()> {
        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(source),
            (),
            (2000., 400.),
            scale,
            std::path::PathBuf::new(),
            PainterChoice::Canvas,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.frame();
        p
    }

    fn id(p: &Presenter<()>, name: &str) -> exact_kernel::ViewId {
        let k = p.host().kernel();
        k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
    }

    #[test]
    fn a_retained_sticky_header_stays_at_the_port() {
        let mut p = boot(
            r#"component App
  view
    scroll width=200 height=100 testId="port"
      box width=200 height=200
        box position="sticky" top=0 width=200 height=20 background-color="red" testId="heading"
        box width=200 height=150 background-color="blue"
      box width=200 height=200
"#,
        );
        let (port, heading) = (id(&p, "port"), id(&p, "heading"));
        assert_eq!(p.rect_of(heading).unwrap().1, 0.);
        p.scroll_to(port, (0., 20. / 2.625)).unwrap();
        p.frame();
        assert_eq!(p.rect_of(heading).unwrap().1, 0.);
    }

    #[test]
    fn a_sticky_scroller_is_not_offered_for_reader_translation() {
        let mut p = boot(
            r#"component App
  view
    scroll width=200 height=100 testId="port"
      box position="sticky" top=0 width=200 height=20 background-color="red"
      box width=200 height=200 background-color="blue"
"#,
        );
        let port = id(&p, "port");
        let words = p.reader_under(10., 10., 2.625);
        assert_eq!(words[3], port as i32);
        let groups = super::super::GROUPS.with(|groups| groups.borrow().clone());
        let before = std::collections::BTreeMap::from([(port, p.scroll_of(port))]);
        p.scroll_to(port, (0., 20. / 2.625)).unwrap();
        let now = std::collections::BTreeMap::from([(port, p.scroll_of(port))]);
        assert!(
            super::shift_groups(&before, &now, &groups, 2.625).is_none(),
            "a move-only frame would move the sticky box"
        );
        assert_eq!(
            words[4] & 4,
            0,
            "a reader translation would move the sticky box"
        );
    }

    #[test]
    fn moving_a_row_a_fraction_of_a_pixel_repositions_its_child() {
        let source = r#"component App
  state lead = 0
  action move
    lead = 0.125
  view
    column
      scroll width=200 height=100
        column
          box height=lead
          column width=200 height=8 padding-top=0.125 box-sizing="border-box" testId="row"
            box width=200 height=4 background-color="red" testId="child"
          box height=200
      button width=20 height=20 press=move testId="move"
"#;
        let mut p = boot(source);
        let (row, child, button) = (id(&p, "row"), id(&p, "child"), id(&p, "move"));
        let row_at = p.rect_of(row).unwrap();
        assert_eq!(p.rect_of(child).unwrap().1, 0.);
        assert_eq!(
            p.host_mut()
                .dispatch_at(button, exact_runner::Event::Press, 0.),
            None
        );
        assert_eq!(p.after_commit(), None);
        p.frame();
        assert_eq!(p.rect_of(row).unwrap(), row_at);
        let replay = p.rect_of(child).unwrap();
        p.reader_replaced(Box::new(super::super::Recorder::new()));
        p.frame();
        let fresh = p.rect_of(child).unwrap();
        assert_eq!(fresh.1 * 2.625, 1.);
        assert_eq!(replay, fresh);
    }

    #[test]
    fn zoom_scales_the_layout_box_and_reaches_its_far_edge() {
        let mut p = boot(
            r#"component App
  view
    scroll width=100 height=100 -exact-max-zoom=10 scrollZoom=10 testId="port"
      box width=100.1 height=30 position="relative"
        box position="absolute" left=0 top=0 width=0.1 height=2 background-color="red" testId="divider"
        box position="absolute" right=0 bottom=0 width=2 height=2 background-color="blue" testId="edge"
"#,
        );
        let port = id(&p, "port");
        let divider = id(&p, "divider");
        let edge = id(&p, "edge");
        let pixels = p.rect_of(divider).unwrap().2 * 2.625;
        assert!((pixels - 2.625).abs() < 0.001, "{pixels} px");
        let end = p.scroll_to(port, (10000., 10000.)).unwrap();
        assert_eq!(end, (901., 200.));
        p.frame();
        let (x, y, w, h) = p.rect_of(edge).unwrap();
        assert!(
            (x + w - 100.).abs() < 0.001 && (y + h - 100.).abs() < 0.001,
            "{:?}",
            (x, y, w, h)
        );
    }

    #[test]
    fn a_zoom_port_and_its_child_share_layout_geometry() {
        let mut p = boot(
            r#"component App
  view
    row
      box width=4 height=20 background-color="red" testId="before"
      scroll width=100 height=20 -exact-max-zoom=10 background-color="blue" testId="port"
        box width=120 height=40 testId="child"
"#,
        );
        let before = p.rect_of(id(&p, "before")).unwrap();
        let port = p.rect_of(id(&p, "port")).unwrap();
        let child = id(&p, "child");
        let child = p.boxes().iter().find(|b| b.id == child).unwrap();
        assert_eq!(before.0 + before.2, 11. / 2.625);
        assert_eq!(port.0, 4.);
        assert_eq!(child.clip.unwrap().0, port.0);
        assert_eq!(child.rect.0, 4.);
    }

    #[test]
    fn a_scaled_sticky_header_stays_on_its_stick_line() {
        use super::super::{stream, FINISHED, MATRIX, ROW_BEGIN, ROW_DRAW, RRECT};

        let mut p = boot(
            r#"component App
  view
    scroll width=400 height=100 testId="port"
      column
        box height=4
        box position="sticky" top=0 width=100 height=20 scale=3 transform-origin="0 0" background-color="red" overflow="hidden" testId="heading"
          box width=0.1 height=2 background-color="blue" testId="divider"
        box height=200
"#,
        );
        let (port, header, divider) = (id(&p, "port"), id(&p, "heading"), id(&p, "divider"));
        p.scroll_to(port, (0., 5.)).unwrap();
        p.frame();
        fn red_top(ops: &[u32]) -> Option<f32> {
            let mut at = 0;
            let mut matrix = (0., 0.);
            let mut rows = std::collections::HashMap::new();
            while at < ops.len() {
                let end = stream::end(ops, at).unwrap();
                let float = |i| f32::from_bits(ops[at + i]);
                match ops[at] {
                    MATRIX => matrix = (float(4), float(6)),
                    RRECT if ops[at + 1] == 0xffff0000 => {
                        return Some(matrix.0 * float(3) + matrix.1);
                    }
                    ROW_BEGIN => {
                        if let Some(top) = red_top(&ops[at + 6..end - 1]) {
                            rows.insert(ops[at + 1], top);
                        }
                    }
                    ROW_DRAW => {
                        if let Some(top) = rows.get(&ops[at + 1]) {
                            return Some(top + float(3));
                        }
                    }
                    _ => {}
                }
                at = end;
            }
            None
        }

        let ops = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
        let top = red_top(&ops).expect("painted header");
        assert!(top.abs() < 0.001, "painted top: {top} px");
        let offset = p.rect_of(header).unwrap().1 - p.rect_of(port).unwrap().1;
        assert!(offset.abs() < 0.0001, "hit top relative to port: {offset}");
        assert!((p.rect_of(divider).unwrap().2 - 0.3).abs() < 0.0001);
        let clip = p
            .boxes()
            .iter()
            .find(|b| b.id == divider)
            .unwrap()
            .clip
            .unwrap();
        assert!((clip.2 * 2.625 - 787.5).abs() < 0.001);
    }

    #[test]
    fn a_distant_row_replays_its_hits_at_the_drawn_edge() {
        use super::super::{stream, FINISHED, ROW_BEGIN, ROW_DRAW, ROW_PAD};

        let mut p = boot(
            r#"component App
  view
    scroll width=200 height=100 testId="port"
      column
        box height=400001
        box width=200 height=20 background-color="red" testId="row"
        box height=200
"#,
        );
        let first = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
        let (port, row) = (id(&p, "port"), id(&p, "row"));
        let mut at = 0;
        let mut recorded = Vec::new();
        while at < first.len() {
            if first[at] == ROW_BEGIN {
                recorded.push(first[at + 1]);
                at += 6;
            } else {
                at = stream::end(&first, at).unwrap();
            }
        }
        assert!(p.rect_of(row).unwrap().1 > 400000.);
        p.scroll_to(port, (0., 400000.)).unwrap();
        p.frame();
        let second = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
        let mut at = 0;
        let mut replayed = false;
        while at < second.len() {
            if second[at] == ROW_DRAW && second[at + 1] == recorded[1] {
                assert_eq!(f32::from_bits(second[at + 3]) + ROW_PAD * 2.625, 3.);
                replayed = true;
            }
            at = stream::end(&second, at).unwrap();
        }
        assert!(replayed);
        assert_eq!(p.hit(10., 3.01 / 2.625), Some(row));
    }

    #[test]
    fn kept_row_origins_and_child_boxes_at_large_scroll_offsets() {
        use super::super::{stream, FINISHED, ROW_DRAW, ROW_PAD};

        let mut positions = Vec::new();
        for (scale, row, offset, expected, expected_child) in [
            (2.625, 400001., 400000., 3., 5.),
            (2.625, 65544.76, 65543.24, 4., 7.),
            (3., 65536.664, 65536.336, 1., 4.),
        ] {
            let mut p = boot_at(
                &format!(
                    r#"component App
  view
    scroll width=200 height=100 scrollTop={offset}
      column
        box height={row}
        column width=200 height=20 background-color="blue" testId="row"
          box width=100 height=2 margin-top=1 background-color="red" testId="child"
        box height=200
"#
                ),
                scale,
            );
            let (row, child) = (id(&p, "row"), id(&p, "child"));
            let ops = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
            let mut at = 0;
            let mut tops = Vec::new();
            while at < ops.len() {
                if ops[at] == ROW_DRAW {
                    tops.push(f32::from_bits(ops[at + 3]) + ROW_PAD * scale);
                }
                at = stream::end(&ops, at).unwrap();
            }
            positions.push((tops[1], expected));
            let row_top = p.rect_of(row).unwrap().1 * scale;
            let child_top = p.rect_of(child).unwrap().1 * scale;
            positions.push((row_top, expected));
            positions.push((child_top, expected_child));
        }
        assert!(
            positions
                .iter()
                .all(|(found, expected)| (found - expected).abs() < 0.0001),
            "{positions:?}"
        );
    }

    #[test]
    fn a_label_as_wide_as_its_text_keeps_its_one_line_on_whole_pixels() {
        // A label's box is its text's width, rounded up to a point. Drawn on
        // whole pixels the box can be a fraction of one narrower, and lines
        // broken at that width would put a label's last word on a second line.
        let words: Vec<String> = (2..40)
            .map(|n| format!("{} ab", "m".repeat(n % 9 + 1)))
            .collect();
        let texts: String = (words.iter().enumerate())
            .map(|(i, w)| format!("      text testId=\"t{i}\" \"{w}\"\n"))
            .collect();
        for lead in [10.0, 10.07, 10.13, 10.19, 10.26, 10.31] {
            let p = boot(&format!(
                "component App\n  view\n    row width=2000 flex-wrap=\"wrap\"\n      box width={lead} height=10\n{texts}"
            ));
            for (i, word) in words.iter().enumerate() {
                let lines = p
                    .paragraph(id(&p, &format!("t{i}")))
                    .unwrap()
                    .line_bottoms()
                    .len();
                assert_eq!(lines, 1, "{word:?} after {lead} px");
            }
        }
    }

    #[test]
    fn kept_rows_are_drawn_on_whole_pixels_and_touch() {
        use super::super::{stream, FINISHED, ROW_BEGIN, ROW_DRAW, ROW_PAD, RRECT};
        // Rows 20.3 px tall, 53.29 pixels at 2.625 a px.
        let row = "        box height=20.3 width=200 background-color=\"#2468ac\"\n".repeat(4);
        let source =
            format!("component App\n  view\n    scroll width=200 height=200\n      column\n{row}");
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(&source),
            (),
            (200., 200.),
            2.625,
            std::path::PathBuf::new(),
            PainterChoice::Canvas,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.frame();
        let ops = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
        let (mut tops, mut heights, mut at) = (Vec::new(), Vec::new(), 0);
        while at < ops.len() {
            at = match ops[at] {
                // Into the row, for its background.
                ROW_BEGIN => at + 6,
                RRECT => {
                    heights.push((f32::from_bits(ops[at + 5]) * 2.625).round());
                    stream::end(&ops, at).unwrap()
                }
                ROW_DRAW => {
                    tops.push(f32::from_bits(ops[at + 3]) + ROW_PAD * 2.625);
                    at + 4
                }
                _ => stream::end(&ops, at).unwrap(),
            };
        }
        assert_eq!(tops, [0., 53., 107., 160.]);
        assert_eq!(heights, [53., 54., 53., 53.]);
    }
}
