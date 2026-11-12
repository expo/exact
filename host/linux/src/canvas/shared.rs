//! Shared live subtrees: state precedes teardown, drawings remain reader-owned.
use super::*;
use crate::paint::{SharedElement, SharedState};
use exact_kernel::motion::motion_node;

pub(super) const STATE: u32 = 49;
pub(super) const SET: u32 = 50;
pub(super) const DRAW: u32 = 51;

#[derive(Default)]
pub(super) struct Shared {
    open: Vec<Open>,
    order: u32,
    active: bool,
}
struct Open {
    header: Vec<u32>,
    ops: Vec<u32>,
    clips: Vec<clip::Clip>,
    matrix: Option<[f32; 6]>,
}

impl Recorder {
    pub(super) fn shared_state(&mut self, state: &SharedState) {
        let active = !state.alive.is_empty() || !state.frames.is_empty();
        if !active && !self.shared.active {
            return;
        }
        self.shared.active = active;
        self.shared.order = 0;
        let mut words = vec![
            STATE,
            3 + state.frames.len() as u32 * 5 + state.alive.len() as u32 * 2,
            u32::from(state.reduced),
            state.frames.len() as u32,
        ];
        for f in &state.frames {
            for key in [f.from, f.to] {
                let key = motion_node(key);
                words.extend([key as u32, (key >> 32) as u32]);
            }
            words.push(f.progress.to_bits());
        }
        words.push(state.alive.len() as u32);
        for key in &state.alive {
            words.extend([*key as u32, (key >> 32) as u32]);
        }
        // begin() may already have announced freed images. Capture first.
        words.append(&mut self.ops);
        self.ops = words;
    }

    pub(super) fn shared_open(&mut self, e: &SharedElement<'_>) -> bool {
        if !self.planes.is_empty() {
            return false;
        }
        self.need(None);
        let rect = |r: Rect4| {
            let r = crate::paint::bbox(e.transform, r);
            [
                (r.0 - self.origin.0) * self.scale,
                (r.1 - self.origin.1) * self.scale,
                r.2 * self.scale,
                r.3 * self.scale,
            ]
        };
        let bounds = rect(e.rect);
        let crop = e.image.map(|(_, r)| rect(r)).unwrap_or([0.0; 4]);
        let content = rect(e.content);
        let natural = e
            .image
            .map(|(image, _)| image.natural())
            .unwrap_or_default();
        let unit = e
            .image
            .map(|(image, _)| image.placed((0.0, 0.0, 1.0, 1.0)))
            .unwrap_or((0.0, 0.0, 1.0, 1.0));
        let image = e.image.map_or(0, |(image, _)| {
            self.image_id(&picture::Picture::Bitmap(image.clone()))
        });
        self.shared.order += 1;
        let scale = e.transform.sx.hypot(e.transform.ky) * self.scale;
        let mut header = vec![
            e.key as u32,
            (e.key >> 32) as u32,
            self.shared.order,
            self.group.unwrap_or(0),
            self.row.as_ref().map_or(0, |r| r.id),
        ];
        header.extend([self.origin.0 * self.scale, self.origin.1 * self.scale].map(f32::to_bits));
        header.extend(bounds.map(f32::to_bits));
        header.push((e.radius * scale).to_bits());
        header.push(image);
        header.extend(crop.map(f32::to_bits));
        header.extend([e.fit, natural.0, natural.1]);
        header.extend([unit.0, unit.1, unit.2, unit.3].map(f32::to_bits));
        header.extend(content.map(f32::to_bits));
        header.push(e.opacity.to_bits());
        let layers = self.layers.shared_ancestors();
        header.push(layers.len() as u32);
        header.extend(layers);
        header.push(self.shared.open.len() as u32);
        for parent in &self.shared.open {
            header.extend(&parent.header[..2]);
        }
        self.ops.extend([DRAW, e.key as u32, (e.key >> 32) as u32]);
        self.shared.open.push(Open {
            header,
            ops: std::mem::take(&mut self.ops),
            clips: std::mem::take(&mut self.clips),
            matrix: self.matrix.take(),
        });
        true
    }

    pub(super) fn shared_close(&mut self) {
        let Some(Open {
            header,
            ops,
            clips,
            matrix,
        }) = self.shared.open.pop()
        else {
            return;
        };
        let body = std::mem::replace(&mut self.ops, ops);
        self.ops.extend([SET, (header.len() + body.len()) as u32]);
        self.ops.extend(header);
        self.ops.extend(body);
        self.clips = clips;
        self.matrix = matrix;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presenter::{PainterChoice, Presenter};

    fn take() -> Vec<u32> {
        FINISHED
            .with(|f| f.borrow_mut().take())
            .expect("painted stream")
    }

    fn definitions(ops: &[u32], out: &mut Vec<(u64, u32, Vec<u64>)>) {
        let mut at = 0;
        while at < ops.len() {
            let end = stream::end(ops, at).expect("valid op");
            assert!(end <= ops.len(), "op {} runs past its body", ops[at]);
            if ops[at] == SET {
                let key = |i| u64::from(ops[i]) | u64::from(ops[i + 1]) << 32;
                let h = at + 2;
                let parents = h + 30 + ops[h + 29] as usize;
                let names = (0..ops[parents] as usize)
                    .map(|i| key(parents + 1 + i * 2))
                    .collect();
                out.push((key(h), ops[h + 4], names));
                definitions(&ops[parents + 1 + ops[parents] as usize * 2..end], out);
            } else if ops[at] == ROW_BEGIN {
                definitions(&ops[at + 6..end - 1], out);
            }
            at = end;
        }
    }

    #[test]
    fn shared_flights_record_lifecycle_before_nested_live_subtrees() {
        let app = r##"component App
  state open = false
  action toggle
    open = not open
  view
    column
      button "Toggle" press=toggle testId="toggle"
      box height=100
      scroll width=350 height=300
        column
          box height=80
          if open
            column sharedElement="body" width=300 height=200 -exact-layout-transition="1s linear"
              text "Detail" sharedElement="title" -exact-layout-transition="1s linear"
          else
            column sharedElement="body" width=120 height=90 -exact-layout-transition="1s linear"
              text "Card" sharedElement="title" -exact-layout-transition="1s linear"
"##;
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(app),
            (),
            (400., 800.),
            2.,
            std::path::PathBuf::new(),
            PainterChoice::Canvas,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.host_mut().flights.enabled = true;
        p.frame();
        let first = take();
        assert_eq!(first[0], STATE);
        assert_eq!((first[3], first[4]), (0, 2));
        let mut before = Vec::new();
        definitions(&first, &mut before);
        assert_eq!(before.len(), 2);
        assert_ne!(
            before[0].1, 0,
            "shared subtree is recorded inside a scroll row"
        );
        assert_eq!(
            before[1].2,
            vec![before[0].0],
            "child retains its shared parent"
        );
        let toggle = p.host().kernel().find_by_test_id("toggle")[0];
        let toggle = p.host().kernel().node_by_key(toggle).unwrap().id;
        p.host_mut()
            .dispatch_at(toggle, exact_runner::Event::Press, 100.);
        p.after_commit();
        p.frame();
        let after = take();
        assert_eq!(
            after[0], STATE,
            "capture source before any frees or node mutations"
        );
        assert_eq!(after[3], 2, "parent and child have their own flights");
        assert_eq!(after[14], 2, "only the arriving names remain alive");
        let mut arrived = Vec::new();
        definitions(&after, &mut arrived);
        assert_eq!(arrived.len(), 2);
        assert!(arrived.iter().all(|a| before.iter().all(|b| a.0 != b.0)));
        assert!(
            drawing(&after).unwrap().1,
            "lifecycle streams cannot be skipped as unchanged drawing"
        );
    }
}
