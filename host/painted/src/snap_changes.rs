//! A changed snap kind owes settlement even when no axis was previously enabled.
use exact_kernel::{CommitReceipt, Kernel, NodeKey, NodeRef, ScrollSnapType};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(crate) struct State {
    kinds: BTreeMap<NodeKey, ScrollSnapType>,
    pub(crate) pending: BTreeSet<NodeKey>,
}

impl State {
    pub(crate) fn seed(&mut self, node: &NodeRef<'_>) {
        if node.style.scroll_snap_type == ScrollSnapType::None {
            self.kinds.remove(&node.key);
        } else {
            self.kinds.insert(node.key, node.style.scroll_snap_type);
        }
    }

    pub(crate) fn commit(&mut self, kernel: &Kernel, receipt: &CommitReceipt) {
        for key in receipt.destroyed.iter().chain(&receipt.renewed) {
            self.kinds.remove(key);
            self.pending.remove(key);
        }
        // Reused keys are new mounts: neither their old kind nor their owed snap survives.
        for key in receipt.created.iter().chain(&receipt.renewed) {
            if let Some(node) = kernel.node_by_key(*key) {
                self.seed(&node);
            }
        }
        for key in &receipt.touched {
            let Some(node) = kernel.node_by_key(*key) else {
                continue;
            };
            let before = self.kinds.get(key).copied().unwrap_or(ScrollSnapType::None);
            let kind = node.style.scroll_snap_type;
            self.seed(&node);
            if kind != before {
                if kind.mandatory() {
                    self.pending.insert(*key);
                } else {
                    self.pending.remove(key);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presenter::{PainterChoice, Presenter};

    #[test]
    fn created_and_renewed_keys_seed_without_inheriting_a_pending_snap() {
        let source = r#"component App
  state kind = "none"
  action x
    kind = "x mandatory"
  action y
    kind = "y mandatory"
  view
    column
      button "X" press=x testId="x"
      button "Y" press=y testId="y"
      scroll testId="pager" width=400 height=100 overflow="scroll" scroll-snap-type=kind
        box width=1200 height=600
"#;
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(source),
            (),
            (400., 600.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert_eq!(error, None);
        let id = |p: &Presenter<()>, name| {
            let k = p.host().kernel();
            k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
        };
        let pager = id(&p, "pager");
        let key = p.host().kernel().node(pager).unwrap().key;
        let mut state = State::default();
        p.tap(id(&p, "x")).unwrap();
        state.commit(
            p.host().kernel(),
            &CommitReceipt {
                created: vec![key],
                touched: vec![key],
                ..Default::default()
            },
        );
        assert!(state.pending.is_empty());
        state.commit(
            p.host().kernel(),
            &CommitReceipt {
                touched: vec![key],
                ..Default::default()
            },
        );
        assert!(state.pending.is_empty(), "same-kind writes owe no probe");
        state.pending.insert(key);
        p.tap(id(&p, "y")).unwrap();
        state.commit(
            p.host().kernel(),
            &CommitReceipt {
                renewed: vec![key],
                touched: vec![key],
                ..Default::default()
            },
        );
        assert!(state.pending.is_empty(), "the old mount's note was removed");
        assert_eq!(
            state.kinds[&key],
            p.host()
                .kernel()
                .node(pager)
                .unwrap()
                .style
                .scroll_snap_type
        );
        p.tap(id(&p, "x")).unwrap();
        state.commit(
            p.host().kernel(),
            &CommitReceipt {
                touched: vec![key],
                ..Default::default()
            },
        );
        assert_eq!(state.pending, BTreeSet::from([key]));
        state.commit(
            p.host().kernel(),
            &CommitReceipt {
                destroyed: vec![key],
                ..Default::default()
            },
        );
        assert!(state.pending.is_empty());
        assert!(state.kinds.is_empty());
    }
}
