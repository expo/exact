//! The instances that own requests (LLP 1035.005.004 D1, D6), found after
//! each render with the frames their requests' gates and arguments read,
//! and the frames a send's `then` runs in.

use super::*;

/// One live instance that owns requests or has sent.
pub struct OwnerInst {
    /// Its storage, holding what its requests show.
    pub storage: RowSlots,
    /// Its frames, as the render evaluates its sites: its own last.
    pub frames: Vec<Frame>,
    /// Its instance path: each region crossed, with the row's key or the
    /// arm shown (LLP 1035.002 D6).
    pub path: String,
}

impl Tree {
    /// Every instance that owns a request or has sent, in tree order. A virtualized
    /// list holds none (the compiler refuses one there), so its rows are not walked.
    pub fn owners(&self) -> Vec<OwnerInst> {
        let mut out = Vec::new();
        visit_children(&self.children, &mut Vec::new(), &mut |storage, scopes| {
            let owned = storage.borrow();
            if owned.requests.is_empty() && owned.sends.is_empty() && owned.tasks.is_empty() {
                return;
            }
            drop(owned);
            let mut path = String::new();
            let frames = scopes
                .iter()
                .map(|(region, scope)| match scope {
                    OwnerScope::Arm(arm, frame) => {
                        exact_num::push_text!(&mut path, "/{}:{}", region.0, arm);
                        frame.scope()
                    }
                    OwnerScope::Row(row) => {
                        exact_num::push_text!(&mut path, "/{}=", region.0);
                        keys::key_text_into(&row.key, &mut path);
                        if row.dup > 0 {
                            exact_num::push_text!(&mut path, "#{}", row.dup);
                        }
                        row.frame.clone()
                    }
                })
                .collect();
            out.push(OwnerInst {
                storage: storage.clone(),
                frames,
                path,
            });
        });
        out
    }

    /// Include an instance that has not sent its first mutation yet.
    pub(crate) fn save_storage(&self) -> Vec<(RowSlots, vm::Owned)> {
        let mut out = Vec::new();
        visit_children(&self.children, &mut Vec::new(), &mut |storage, _| {
            out.push((storage.clone(), storage.borrow().clone()));
        });
        out
    }
}

enum OwnerScope<'a> {
    Arm(usize, &'a ArmFrame),
    Row(&'a Row),
}

fn visit_children<'a>(
    children: &'a [Child],
    scopes: &mut Vec<(RegionsId, OwnerScope<'a>)>,
    visit: &mut impl FnMut(&RowSlots, &[(RegionsId, OwnerScope<'a>)]),
) {
    for child in children {
        let region = match child {
            Child::Node(node) => {
                if node.contains_regions {
                    visit_children(&node.children, scopes, visit);
                }
                continue;
            }
            Child::Region(region) => region,
        };
        match &region.active {
            Active::Arm { arm, frame, roots } => {
                let Some(arm) = arm else { continue };
                scopes.push((region.region, OwnerScope::Arm(*arm, frame)));
                if let Some(storage) = &frame.row {
                    visit(storage, scopes);
                }
                visit_children(roots, scopes, visit);
                scopes.pop();
            }
            Active::Rows { rows } => {
                for row in rows {
                    scopes.push((region.region, OwnerScope::Row(row)));
                    if let Some(storage) = &row.slots {
                        visit(storage, scopes);
                    }
                    visit_children(&row.roots, scopes, visit);
                    scopes.pop();
                }
            }
        }
    }
}
