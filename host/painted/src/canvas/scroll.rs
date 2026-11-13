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
use exact_runner::DataSource;

fn module_under<D: DataSource>(
    p: &mut crate::presenter::Presenter<D>,
    x: f32,
    y: f32,
    scale: f32,
) -> [i32; 10] {
    let module = p.hit(x / scale, y / scale).filter(|id| {
        p.host().kernel().node(*id).is_some_and(|node| {
            node.node_type == exact_kernel::NodeType::NativeView
                && p.host().retained_action_eligible(node.key)
        })
    });
    let rect = module.and_then(|id| p.rect_of(id)).unwrap_or_default();
    let key = module.and_then(|id| p.host().kernel().node(id).map(|node| node.key));
    let bits = |value: f32| (value * scale).to_bits() as i32;
    [
        module.unwrap_or(0) as i32,
        bits(rect.0),
        bits(rect.1),
        bits(rect.2),
        bits(rect.3),
        key.map_or(0, |key| key.index as i32),
        key.map_or(0, |key| key.generation as i32),
        p.long_presses() as i32,
        p.brush.native_session as i32,
        (p.brush.native_session >> 32) as i32,
    ]
}

#[cfg(target_os = "android")]
impl<D: DataSource + Default> CanvasHost<D> {
    /// What is under a point of the view (pixels), for the reader to decide
    /// what a drag from there is: the scrollers, innermost first, and the
    /// axes the presenter's own gestures claim
    /// ([`crate::presenter::Presenter::reader_under`] has the words); then
    /// the module under it (ten words), and each of those scrollers'
    /// snap ([`crate::presenter::Presenter::reader_snaps`]).
    pub fn under(&mut self, x: f32, y: f32) -> Vec<i32> {
        let mut words = self.p.reader_under(x, y, self.scale);
        words.extend(module_under(&mut self.p, x, y, self.scale));
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
        self.scrolled = self.prefetching;
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

    /// What the reader is told of the scrollers it knows, when that is not
    /// what it was last told
    /// ([`crate::presenter::Presenter::reader_scrollers`] has the words).
    pub fn scrollers(&mut self) -> Option<Vec<i32>> {
        let scale = self.scale;
        GROUPS.with(|g| self.p.reader_scrollers(&g.borrow(), scale))
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
    use super::*;
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
        let hit = module_under(&mut p, 20., 20., 2.);
        let token = |words: [i32; 10]| words[8] as u32 as u64 | ((words[9] as u32 as u64) << 32);
        assert_ne!(hit[0], 0);
        assert_eq!(hit[5] as u32 as u64, before["slot"].as_u64().unwrap());
        assert_eq!(hit[6] as u32 as u64, before["generation"].as_u64().unwrap());
        assert_eq!(token(hit), before["session"].as_u64().unwrap());
        assert_eq!(f32::from_bits(hit[3] as u32), 200.);
        assert!(p
            .reload(&crate::test_plan(&source.replace("menu\"", "other\"")), ())
            .unwrap()
            .is_none());
        let after = props(&mut p);
        let next = module_under(&mut p, 20., 20., 2.);
        assert_eq!(hit[..8], next[..8]);
        for key in ["module", "slot", "generation"] {
            assert_eq!(before[key], after[key]);
        }
        assert_ne!(before["session"], after["session"]);
        assert_eq!(token(next), after["session"].as_u64().unwrap());
    }
}
