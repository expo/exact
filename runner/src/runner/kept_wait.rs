//! A kept answer whose identity is not yet known waits for it (LLP 1027.005
//! D3 and D5, amended 2026-11-13). An identifying argument that differs from
//! the kept one and is empty, `none`, or reads a store reader still on its
//! fallback is waiting, not mismatched. The entry waits while every
//! differing argument is waiting, and the first commit whose identifying
//! arguments are known and equal the kept ones admits it, before or after
//! readiness. A known, non-empty mismatch consumes it.

use super::{DataSource, ResourceState, Runner};
use exact_plan::Value;

/// A kept entry waiting for its identity: its identifying arguments and a
/// digest of the stored bytes, never a second answer. Admission reads the
/// entry again, so one replaced or forgotten meanwhile admits nothing.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Waiting {
    pub(super) args: Vec<Value>,
    pub(super) digest: u64,
    /// The seed slept while idle: admitted, it is no answer until a fresh
    /// one after the wake (LLP 1161.001 D1).
    pub(super) slept: bool,
}

pub(super) fn digest(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

/// What a kept identity is to this commit's.
#[derive(PartialEq)]
enum Match {
    /// Known and equal: admitted.
    Admit,
    /// Every differing argument is waiting, or the identity is equal but
    /// not known yet or the request is invalid.
    Wait,
    /// A known, non-empty mismatch: consumed.
    Mismatch,
}

/// What [`Runner::admit_kept`] decided for a resource this pass.
pub(super) struct Admission {
    /// It shows a kept seed, which its ask may reuse before readiness.
    pub(super) kept_seed: bool,
    /// What stands is not to be reused: a seed now waiting, or one admitted
    /// after readiness, whose ask must still go out.
    pub(super) hold: bool,
    /// A waiting entry was admitted before readiness: it shows as a boot
    /// seed does, neither pending nor awaiting.
    pub(super) admitted_early: bool,
}

impl<D: DataSource> Runner<D> {
    /// Admit, hold or consume resource `i`'s kept answer for this pass's
    /// `args`: the boot seed `states[i]` marks, or the entry waiting for
    /// its identity. A kept surface fact keeps its own rule: shown while
    /// its record is unpublished, consumed on any mismatch.
    pub(super) fn admit_kept(
        &mut self,
        i: usize,
        args: &[Value],
        valid: bool,
        ready: bool,
        states: &mut [Option<ResourceState>],
    ) -> Admission {
        let row = self.resource_row(i);
        let now = &args[..row.args.len as usize - usize::from(row.context)];
        let mut out = Admission {
            kept_seed: false,
            hold: false,
            admitted_early: false,
        };
        let seed = (states[i].as_ref())
            .filter(|s| s.kept_seed)
            .map(|s| s.args.clone());
        if let Some(kept) = seed {
            let mut matched = self.kept_match(i, &kept, now, valid, states);
            // An admitted seed that departs its identity is consumed, not
            // readmitted when the identity returns (D3).
            if matched == Match::Wait && self.resources[i].kept_admitted {
                matched = Match::Mismatch;
            }
            if super::kept::kept_surface(&self.plan, i) {
                out.kept_seed = matched == Match::Admit && self.surface_seed_waiting(i);
                if !ready && !out.kept_seed {
                    self.refuse_kept(i, &kept, now, states);
                }
            } else if !ready {
                match matched {
                    Match::Admit => {
                        out.kept_seed = true;
                        self.resources[i].kept_admitted = true;
                    }
                    Match::Wait => {
                        out.hold = true;
                        self.start_waiting(i, kept, false);
                    }
                    Match::Mismatch => self.refuse_kept(i, &kept, now, states),
                }
            } else if states[i].as_ref().is_some_and(|s| s.slept) {
                // Idle through readiness, so never compared until its gate
                // opened (LLP 1161.002 §7). Admitted, it stands while its
                // ask is pending; otherwise the fallback shows.
                out.hold = true;
                match matched {
                    Match::Admit => self.resources[i].kept_admitted = true,
                    Match::Wait => {
                        self.start_waiting(i, kept, true);
                        states[i] = None;
                    }
                    Match::Mismatch => {
                        self.refuse_kept(i, &kept, now, states);
                        states[i] = None;
                    }
                }
            }
            // Otherwise activation consumed it (D4).
        } else if let Some(waiting) = self.resources[i].waiting.clone() {
            match self.kept_match(i, &waiting.args, now, valid, states) {
                Match::Admit => {
                    self.resources[i].waiting = None;
                    if let Some(value) = self.waiting_entry(i, &waiting) {
                        // Provisional, as a boot seed is (D4), and keeping
                        // its sleep history (LLP 1161.001 D1).
                        let slept = waiting.slept || states[i].as_ref().is_some_and(|s| s.slept);
                        states[i] = Some(ResourceState {
                            inspection_generation: 0,
                            args: waiting.args.clone(),
                            value: crate::held::Held::new(value),
                            store_revision: self.store.revision(),
                            placeholder: false,
                            kept_seed: true,
                            answered_for: None,
                            asleep: false,
                            slept,
                            origin: 0,
                        });
                        self.resources[i].stale = true;
                        self.resources[i].kept_admitted = true;
                        out.kept_seed = !ready;
                        out.hold = ready;
                        out.admitted_early = !ready;
                        let mut shown = String::new();
                        crate::agent::untyped_json(&Value::list(waiting.args), &mut shown);
                        let name = self.plan.str(self.resource_row(i).name);
                        let line = super::lines::kept_shown(name, &shown);
                        self.keeping.refused.push((i, line));
                    }
                }
                Match::Wait => {}
                Match::Mismatch => {
                    self.resources[i].waiting = None;
                    self.refuse_kept(i, &waiting.args, now, states);
                }
            }
        }
        if let Some(state) = &mut states[i] {
            state.kept_seed = out.kept_seed;
        }
        out
    }

    /// How the kept identity `kept` compares with this pass's `now`.
    fn kept_match(
        &self,
        i: usize,
        kept: &[Value],
        now: &[Value],
        valid: bool,
        states: &[Option<ResourceState>],
    ) -> Match {
        if kept.len() != now.len() {
            return Match::Mismatch;
        }
        if kept == now {
            let known = (0..now.len()).all(|k| !self.arg_unknown(i, k, states));
            return if valid && known {
                Match::Admit
            } else {
                Match::Wait
            };
        }
        let waiting = |k: usize| {
            let v = &now[k];
            v.as_str() == Some("")
                || matches!(v, Value::Option(None))
                || self.arg_unknown(i, k, states)
        };
        if (0..now.len()).filter(|&k| kept[k] != now[k]).all(waiting) {
            Match::Wait
        } else {
            Match::Mismatch
        }
    }

    /// Whether resource `i`'s identifying argument `k` reads, directly or
    /// through derives, a store reader still on its placeholder, or on its
    /// compiled answer while it waits for its source (`stale`): its value is
    /// not yet this launch's. A fresh answer equal to the compiled one keeps
    /// the compiled object (settlement) but is known. An account resource
    /// with no kept answer of its own shows its placeholder (`did: ""`),
    /// and that empty id would match whatever a signed-out launch kept
    /// (LLP 1027.005 D3, 2026-10-10). Kept seeds, carried and checkpoint
    /// answers, fresh answers and non-store values are known.
    fn arg_unknown(&self, i: usize, k: usize, states: &[Option<ResourceState>]) -> bool {
        use crate::instance::Input;
        self.identity_inputs(
            i,
            |j| j == k,
            |input| match input {
                Input::Resource(r) | Input::PendingResource(r) | Input::FailedResource(r) => {
                    self.resources[r].store_reader
                        && states[r].as_ref().is_some_and(|s| {
                            s.placeholder || s.value.is_compiled() && self.resources[r].stale
                        })
                }
                _ => false,
            },
        )
    }

    /// Resource `i`'s boot seed, kept for `kept`, waits for its identity;
    /// its fallback shows meanwhile. `slept`: the seed slept while idle.
    fn start_waiting(&mut self, i: usize, kept: Vec<Value>, slept: bool) {
        let name = self.plan.str(self.resource_row(i).name);
        let Some(entry) = self.store.kept(&super::kept::kept_name(name)) else {
            return;
        };
        let digest = digest(entry);
        let line = super::lines::kept_waiting(name);
        self.keeping.refused.push((i, line));
        self.resources[i].waiting = Some(Waiting {
            args: kept,
            digest,
            slept,
        });
    }

    /// The value of resource `i`'s waiting entry, when the store still holds
    /// the bytes it waited on and they still decode to as many identifying
    /// arguments as the call has and a value of its shape.
    fn waiting_entry(&mut self, i: usize, waiting: &Waiting) -> Option<Value> {
        let name = super::kept::kept_name(self.plan.str(self.resource_row(i).name));
        if self.store.kept(&name).map(digest) != Some(waiting.digest) {
            return None;
        }
        self.kept_seed(i)
            .filter(|(args, _)| *args == waiting.args)
            .map(|(_, value)| value)
    }

    /// A fresh answer to resource `i` for `args` consumes its waiting entry
    /// when it answers that entry's identity, kept or not; keeping or
    /// forgetting the stored entry (`replaced`) consumes it too.
    pub(super) fn answered_waiting(&mut self, i: usize, args: &[Value], replaced: bool) {
        let row = self.resource_row(i);
        let identity = row.args.len as usize - usize::from(row.context);
        let ours = (self.resources[i].waiting.as_ref())
            .is_some_and(|w| args.get(..identity) == Some(&w.args[..]));
        if replaced || ours {
            self.resources[i].waiting = None;
        }
    }

    /// The declared resources' waiting entries, for a reload to carry.
    pub(super) fn carry_waiting(&self) -> Vec<(String, String, Vec<Value>, u64)> {
        (0..self.declared_resources().len())
            .filter_map(|i| {
                let w = self.resources[i].waiting.as_ref()?;
                let row = self.resource_row(i);
                Some((
                    self.plan.str(row.name).to_string(),
                    self.plan.str(row.source).to_string(),
                    w.args.clone(),
                    w.digest,
                ))
            })
            .collect()
    }

    /// A reload keeps a waiting entry for a store reader or a kept module
    /// whose name, source and logic (`carried`, given only for the same data
    /// module) are unchanged and that is not `transient`; admission still
    /// checks the entry's identity length and shape. A private session or a
    /// launch with a presence secret locked keeps none (LLP 1027.005 D3).
    pub(super) fn carry_waiting_in(&mut self, carried: Option<&super::Carried>, private: bool) {
        let Some(carried) = carried.filter(|_| !private && !self.store.opened_locked()) else {
            return;
        };
        for i in 0..self.plan.resources.len() {
            let row = &self.plan.resources[i];
            let eligible = row.reader
                && super::kept::waits_for_module(&self.plan, self.plan.str(row.source))
                || super::kept::kept_module(&self.plan, i);
            if !eligible
                || row.transient
                || self.resources[i]
                    .state
                    .as_ref()
                    .is_some_and(|s| s.kept_seed)
            {
                continue;
            }
            let (name, source) = (self.plan.str(row.name), self.plan.str(row.source));
            if let Some((_, _, args, digest)) =
                (carried.waiting.iter()).find(|(n, s, _, _)| n == name && s == source)
            {
                self.resources[i].waiting = Some(Waiting {
                    args: args.clone(),
                    digest: *digest,
                    slept: false,
                });
            }
        }
    }

    /// Journal resource `i`'s kept answer, kept for `kept`, consumed by a
    /// known mismatch with `now`: each differing identifying argument with
    /// its value, and the states and resources it reads with theirs. Once
    /// per resource and commit.
    fn refuse_kept(
        &mut self,
        i: usize,
        kept: &[Value],
        now: &[Value],
        states: &[Option<ResourceState>],
    ) {
        if self.keeping.refused.iter().any(|(r, _)| *r == i) {
            return;
        }
        let json = |v: &Value| {
            let mut out = String::new();
            crate::agent::untyped_json(v, &mut out);
            out
        };
        let list = |args: &[Value]| json(&Value::list(args.to_vec()));
        let differing: Vec<String> = (0..now.len())
            .filter(|&k| kept.get(k) != Some(&now[k]))
            .map(|k| {
                let inputs = self.arg_inputs(i, k, states, &json);
                super::lines::kept_argument(k + 1, &json(&now[k]), &inputs)
            })
            .collect();
        let mut why = format!(
            "this launch asks for {}",
            super::lines::shortened(&list(now))
        );
        if !differing.is_empty() {
            why = format!("{why}: {}", differing.join("; "));
        }
        let name = self.plan.str(self.resource_row(i).name);
        let line = super::lines::kept_refused(name, Some(&list(kept)), &why);
        self.keeping.refused.push((i, line));
    }

    /// The root states and resources resource `i`'s identifying argument
    /// `k` reads, directly or through derives, each with its value. A
    /// resource says whether it holds an admitted kept answer or is a store
    /// reader without one.
    fn arg_inputs(
        &self,
        i: usize,
        k: usize,
        states: &[Option<ResourceState>],
        json: &dyn Fn(&Value) -> String,
    ) -> Vec<String> {
        use crate::instance::Input;
        let mut named = Vec::new();
        self.identity_inputs(
            i,
            |j| j == k,
            |input| {
                let name = match input {
                    Input::Slot(s) => (self.plan.slots.get(s).zip(self.slots.get(s)))
                        .filter(|(slot, _)| slot.owner.is_none())
                        .map(|(slot, v)| {
                            let value = super::lines::shortened(&json(v));
                            format!("state {} = {value}", self.plan.str(slot.name))
                        }),
                    Input::Resource(r) | Input::PendingResource(r) | Input::FailedResource(r) => {
                        states.get(r).and_then(Option::as_ref).map(|s| {
                            let name = self.plan.str(self.resource_row(r).name);
                            let value = super::lines::shortened(&json(s.value.get(&self.plan)));
                            let what = if s.kept_seed {
                                " (holding an admitted kept answer)"
                            } else if self.resources[r].store_reader {
                                " (a store reader with no admitted device answer)"
                            } else {
                                ""
                            };
                            format!("resource {name} = {value}{what}")
                        })
                    }
                    _ => None,
                };
                if let Some(name) = name.filter(|n| !named.contains(n)) {
                    named.push(name);
                }
                false
            },
        );
        named
    }
}
