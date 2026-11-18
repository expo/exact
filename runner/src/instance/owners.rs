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
    /// list holds none (the compiler refuses one there), so its rows are
    /// not walked.
    pub fn owners(&self) -> Vec<OwnerInst> {
        self.walk_owners(false)
    }

    /// Include an instance that has not sent its first mutation yet.
    pub(crate) fn save_storage(&self) -> Vec<(RowSlots, vm::Owned)> {
        self.walk_owners(true)
            .into_iter()
            .map(|o| {
                let value = o.storage.borrow().clone();
                (o.storage, value)
            })
            .collect()
    }

    fn walk_owners(&self, all: bool) -> Vec<OwnerInst> {
        let mut out = Vec::new();
        walk_children(
            &self.children,
            &mut Vec::new(),
            &mut String::new(),
            &mut out,
            all,
        );
        out
    }
}

fn walk_children(
    children: &[Child],
    frames: &mut Vec<Frame>,
    path: &mut String,
    out: &mut Vec<OwnerInst>,
    all: bool,
) {
    for c in children {
        match c {
            Child::Node(n) => walk_children(&n.children, frames, path, out, all),
            Child::Region(r) => walk_region(r, frames, path, out, all),
        }
    }
}

fn walk_region(
    r: &RegionInst,
    frames: &mut Vec<Frame>,
    path: &mut String,
    out: &mut Vec<OwnerInst>,
    all: bool,
) {
    let at = path.len();
    match &r.active {
        Active::Arm { arm, frame, roots } => {
            let Some(arm) = arm else { return };
            exact_num::push_text!(path, "/{}:{}", r.region.0, arm);
            frames.push(frame.scope());
            visit(frame.row.as_ref(), roots, frames, path, out, all);
            frames.pop();
        }
        Active::Rows { rows } => {
            for row in rows {
                path.truncate(at);
                exact_num::push_text!(path, "/{}=", r.region.0);
                path.push_str(&key_text(&row.key).unwrap_or_default());
                if row.dup > 0 {
                    exact_num::push_text!(path, "#{}", row.dup);
                }
                frames.push(row.frame.clone());
                visit(row.slots.as_ref(), &row.roots, frames, path, out, all);
                frames.pop();
            }
        }
    }
    path.truncate(at);
}

fn visit(
    storage: Option<&RowSlots>,
    roots: &[Child],
    frames: &mut Vec<Frame>,
    path: &mut String,
    out: &mut Vec<OwnerInst>,
    all: bool,
) {
    if let Some(storage) = storage.filter(|s| {
        let s = s.borrow();
        all || !s.requests.is_empty() || !s.sends.is_empty() || !s.tasks.is_empty()
    }) {
        out.push(OwnerInst {
            storage: storage.clone(),
            frames: frames.clone(),
            path: path.clone(),
        });
    }
    walk_children(roots, frames, path, out, all);
}
