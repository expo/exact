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
use super::{CanvasHost, Section};
use super::{GROUPS, SHIFT};
#[cfg(target_os = "android")]
use crate::presenter::PAGE;
use crate::presenter::{still::Still, Presenter};
use exact_kernel::ViewId;
use exact_runner::DataSource;
use std::collections::BTreeMap;

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
    fn can_skip(&self, lead: bool) -> bool {
        lead && self.quick()
    }
    pub(super) fn take_request_skip(&mut self) -> bool {
        std::mem::take(&mut self.scroll_turn)
    }
    pub(super) fn owed(&self) -> bool {
        self.scrolled
    }
}

pub(super) struct Painted {
    pub(super) still: Still,
    scroll: BTreeMap<ViewId, (f32, f32)>,
    pub(super) groups: Vec<(ViewId, (f32, f32))>,
}

impl Painted {
    pub(super) fn capture<D: DataSource>(p: &Presenter<D>) -> Option<Self> {
        Some(Self {
            still: p.still()?,
            scroll: p.scroll_offsets().clone(),
            groups: GROUPS.with(|g| g.borrow().clone()),
        })
    }

    pub(super) fn shift<D: DataSource>(
        &self,
        p: &mut Presenter<D>,
        quiet: Option<u64>,
        scale: f32,
    ) -> Option<Vec<u32>> {
        if p.reader_selection_active() {
            return None;
        }
        let now = p.scroll_offsets();
        let grouped = |id: &ViewId| self.groups.iter().any(|(g, _)| g == id);
        let others = |m: &BTreeMap<ViewId, (f32, f32)>| {
            m.iter()
                .filter(|(id, _)| !grouped(id))
                .map(|(id, o)| (*id, o.0.to_bits(), o.1.to_bits()))
                .collect::<Vec<_>>()
        };
        let still = p.still()?;
        let same =
            still == self.still || quiet.is_some_and(|epoch| self.still.same_at(&still, epoch));
        if others(now) != others(&self.scroll) || !same {
            return None;
        }
        let mut ops = vec![SHIFT, self.groups.len() as u32];
        for (id, at) in &self.groups {
            let to = now.get(id).copied().unwrap_or((0.0, 0.0));
            ops.extend([
                *id,
                ((at.0 - to.0) * scale).to_bits(),
                ((at.1 - to.1) * scale).to_bits(),
            ]);
        }
        p.moved_without_paint();
        Some(ops)
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

    /// What the reader is told of the scrollers it knows
    /// ([`crate::presenter::Presenter::reader_scrollers`] has the words).
    pub fn scrollers(&mut self) -> Option<Vec<i32>> {
        let scale = self.scale;
        let words = GROUPS.with(|g| self.p.reader_scrollers(&g.borrow(), scale));
        self.told = Some(self.p.host().kernel().layout_stamp());
        words
    }

    /// After the reader's scroll step: whether its frame would only move the
    /// grouped rows the reader already moved. Skip only a quick scroll turn
    /// when the reader's offsets and layout agree with the host and no other
    /// work needs a frame. [`Self::shift`] takes the move as a frame would,
    /// leaving no stream to send.
    pub fn moved(&mut self) -> bool {
        let now = self.now();
        let quiet = self.scroll_hold.can_skip(self.lead)
            && !self.responding
            && !self.surfaces
            && !self.owed_surfaces
            && self.p.turn_due_ms(now).is_none_or(|due| due > 0.0)
            && self.told == Some(self.p.host().kernel().layout_stamp())
            && self.p.reader_moves_stand()
            && !self.p.bar_scroll_changed()
            && (!self.move_tracks || self.p.host().lowered_epoch() == self.tracks.epoch)
            && !super::agent::waiting();
        quiet && self.shift(now).is_some()
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
    use super::{Painted, ScrollHold};
    use crate::presenter::{PainterChoice, Presenter};
    use exact_kernel::ViewId;

    const LIST: &str = r#"component App
  view
    scroll testId="list" width=200 height=200
      box height=80 flex-shrink=0
      text "Selected word" testId="text" user-select="text" font-size=16
      box height=400 flex-shrink=0
"#;

    fn boot(plan: &[u8]) -> Presenter<()> {
        let (p, error) = Presenter::boot_with(
            plan,
            (),
            (200., 200.),
            1.,
            Default::default(),
            PainterChoice::Canvas,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p
    }

    fn named(p: &Presenter<()>, name: &str) -> ViewId {
        let k = p.host().kernel();
        k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
    }

    fn paint(p: &mut Presenter<()>) -> Vec<u32> {
        let frame = p.display_frame().unwrap();
        assert!(p.display_complete(&frame));
        super::super::FINISHED
            .with(|f| f.borrow_mut().take())
            .unwrap()
    }

    #[test]
    fn a_grouped_scroll_keeps_its_turn_until_prefetching_begins() {
        let mut p = boot(&crate::test_plan(LIST));
        paint(&mut p);
        let list = named(&p, "list");
        let painted = Painted::capture(&p).unwrap();
        assert!(painted.groups.iter().any(|(id, _)| *id == list));
        p.reader_under(30., 50., 1.);
        let mut hold = ScrollHold::default();
        assert_eq!(p.reader_scrolled(list, (0., 40.), 0), Some((0., 40.)));
        hold.note_scroll(false);
        assert!(!hold.can_skip(true));
        hold.note_scroll(true);
        assert!(hold.can_skip(true));
        assert!(painted.shift(&mut p, None, 1.).is_some());
    }

    #[test]
    fn selection_marks_and_handles_follow_a_grouped_scroll() {
        let mut p = boot(&crate::test_plan(LIST));
        paint(&mut p);
        let list = named(&p, "list");
        let text = named(&p, "text");
        let painted = Painted::capture(&p).unwrap();
        assert!(painted.groups.iter().any(|(id, _)| *id == list));
        p.reader_under(30., 50., 1.);
        let (x, y, _, _) = p.rect_of(text).unwrap();
        let before = p.reader_select(0, x + 10., y + 8., 1.).unwrap();

        assert_eq!(p.reader_scrolled(list, (0., 40.), 0), Some((0., 40.)));
        if painted.shift(&mut p, None, 1.).is_none() {
            paint(&mut p);
        }
        let after = p.reader_select(4, 0., 0., 1.).unwrap();
        for (old, new) in [
            (&before["marks"][0], &after["marks"][0]),
            (&before["start"], &after["start"]),
            (&before["end"], &after["end"]),
        ] {
            for (i, delta) in [(0, 0.), (1, -40.), (2, 0.), (3, -40.)] {
                assert_eq!(new[i].as_f64().unwrap(), old[i].as_f64().unwrap() + delta);
            }
        }
        assert_eq!(after["clip"], before["clip"]);
    }

    #[test]
    fn a_zoomed_pan_paints_the_ungrouped_child_at_its_new_offset() {
        let mut p = boot(&crate::test_plan(
            r#"component App
  view
    scroll testId="zoom" width=200 height=200 scrollZoom=2 -exact-max-zoom=4
      box testId="image" width=160 height=160 background-color="red"
"#,
        ));
        paint(&mut p);
        let zoom = named(&p, "zoom");
        let child = named(&p, "image");
        let painted = Painted::capture(&p).unwrap();
        let hit = p.reader_under(50., 50., 1.);
        assert_eq!(hit[3] as ViewId, zoom);
        assert_eq!(hit[4] & 4, 0, "no row node in the touch answer");
        let words = p.reader_scrollers(&painted.groups, 1.).unwrap();
        assert_eq!(words[1] & 4, 0, "no row node in the scroller statement");
        let before = p.rect_of(child).unwrap();
        assert_eq!(p.reader_scrolled(zoom, (20., 30.), 0), Some((20., 30.)));
        assert!(painted.shift(&mut p, None, 1.).is_none());
        let ops = paint(&mut p);
        assert_ne!(ops.first(), Some(&super::SHIFT));
        let after = p.rect_of(child).unwrap();
        assert_eq!(after, (before.0 - 20., before.1 - 30., before.2, before.3));
    }

    #[test]
    fn an_adopted_reader_observes_the_epoch_after_animated_layers_leave() {
        use super::super::{layer, Recorder};

        let mut p = boot(&crate::test_plan(
            r#"keyframes enter
  from opacity=0
  to opacity=1

component App
  state shown = false
  action toggle
    shown = !shown
  view
    column
      button "Toggle" testId="toggle" press=toggle
      if shown
        box width=50 height=50 background-color="red" animation="enter 300ms linear both"
"#,
        ));
        p.host_mut().lowering = true;
        p.host_mut()
            .engine
            .set_lowered_properties(&[exact_motion::Property::Opacity]);
        let toggle = named(&p, "toggle");
        let mut tracks = super::super::tracks::Tracks::default();
        p.tap(toggle).unwrap();
        let mut ops = paint(&mut p);
        assert!(!layer::ALIVE.with(|a| a.borrow().is_empty()));
        tracks.send(&p, 0, &mut ops);
        p.tap(toggle).unwrap();
        let mut ops = paint(&mut p);
        assert!(layer::ALIVE.with(|a| a.borrow().is_empty()));
        tracks.send(&p, 0, &mut ops);
        let epoch = p.host().lowered_epoch();
        assert!(epoch > 0);

        p.reader_replaced(Box::new(Recorder::new()));
        let mut adopted = super::super::tracks::Tracks::default();
        paint(&mut p);
        assert!(layer::ALIVE.with(|a| a.borrow().is_empty()));
        let mut tracks = Vec::new();
        adopted.send(&p, 0, &mut tracks);
        assert!(tracks.is_empty());
        assert_eq!(adopted.epoch, epoch);
    }

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
}
