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
    /// It was kept across idleness, as a seed or while it waited: admitted,
    /// it is no answer until a fresh one after the wake (LLP 1161.001 D1).
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
    /// after readiness, whose ask must still go out. Admitted at a ready
    /// boot, it stands over the bake's answer while it is asked.
    pub(super) hold: bool,
    /// A waiting entry was admitted before readiness: it shows as a boot
    /// seed does, neither pending nor awaiting.
    pub(super) admitted_early: bool,
    /// The seed this pass let go was kept across idleness: the fallback
    /// that replaces it is no answer since the wake either (LLP 1161.001 D1).
    pub(super) slept: bool,
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
        let now = &args[..self.identity_len(i)];
        let mut out = Admission {
            kept_seed: false,
            hold: false,
            admitted_early: false,
            slept: false,
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
                    self.refuse_kept(i, &kept, now, valid, states);
                }
            } else if !ready {
                match matched {
                    Match::Admit => {
                        out.kept_seed = true;
                        self.resources[i].kept_admitted = true;
                        // `data_ready`'s ask of it is this commit's.
                        self.note_ask(i);
                    }
                    Match::Wait => {
                        out.hold = true;
                        // A seed woken from idleness keeps that history: the
                        // placeholder that replaces it meanwhile does not.
                        let slept = states[i].as_ref().is_some_and(|s| s.slept);
                        self.start_waiting(i, kept, slept);
                    }
                    Match::Mismatch => self.refuse_kept(i, &kept, now, valid, states),
                }
            } else if let Some(slept) = (states[i].as_ref())
                .map(|s| s.slept)
                .filter(|&slept| slept || !self.resources[i].kept_admitted)
            {
                // Never compared before readiness: a kept module's seed at a
                // boot whose source is ready (D5), or a seed idle through
                // readiness until its gate opened (LLP 1161.002 §7), which
                // may have been. Admitted, it stands while its ask is
                // pending; otherwise the fallback shows.
                out.hold = true;
                out.slept = slept && matched != Match::Admit;
                match matched {
                    Match::Admit => {
                        self.resources[i].kept_admitted = true;
                        self.journal_shown(i, &kept);
                    }
                    Match::Wait => {
                        self.start_waiting(i, kept, slept);
                        states[i] = None;
                    }
                    Match::Mismatch => {
                        self.refuse_kept(i, &kept, now, valid, states);
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
                            from_page: false,
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
                        self.journal_shown(i, &waiting.args);
                    }
                }
                Match::Wait => {}
                Match::Mismatch => {
                    self.resources[i].waiting = None;
                    self.refuse_kept(i, &waiting.args, now, valid, states);
                }
            }
        }
        if let Some(state) = &mut states[i] {
            state.kept_seed = out.kept_seed;
        }
        out
    }

    /// Journal that resource `i` shows its kept answer, kept for `args`, at
    /// a commit after boot's (`journal_kept_seeds` names boot's seeds). The
    /// line names no identifying value.
    fn journal_shown(&mut self, i: usize, args: &[Value]) {
        let name = self.plan.str(self.resource_row(i).name);
        let line = super::lines::kept_shown(name);
        self.keeping.refused.push((i, line, Some(args.to_vec())));
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
                    self.on_fallback(r, states)
                }
                _ => false,
            },
        )
    }

    /// Whether resource `r` is a store reader with no admitted device
    /// answer: on its placeholder, or on its compiled answer while it waits
    /// for its source (`stale`) or for its first answer (`origin` 0; a fresh
    /// answer equal to the compiled one keeps the compiled object with its
    /// request's origin).
    fn on_fallback(&self, r: usize, states: &[Option<ResourceState>]) -> bool {
        self.resources[r].store_reader
            && states[r].as_ref().is_some_and(|s| {
                s.placeholder || s.value.is_compiled() && (self.resources[r].stale || s.origin == 0)
            })
    }

    /// Whether resource `r` shows a kept answer no fresh one has replaced:
    /// its boot seed, or one admitted since (`origin` 0, neither compiled
    /// nor a placeholder). It is provisional (D4), so not an answer to carry.
    pub(super) fn holds_kept(&self, r: usize, state: &ResourceState) -> bool {
        state.kept_seed
            || self.resources[r].kept_admitted
                && state.origin == 0
                && !state.placeholder
                && !state.value.is_compiled()
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
        self.keeping.refused.push((i, line, None));
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
        let identity = self.identity_len(i);
        let ours = (self.resources[i].waiting.as_ref())
            .is_some_and(|w| args.get(..identity) == Some(&w.args[..]));
        if replaced || ours {
            self.resources[i].waiting = None;
        }
    }

    /// The declared resources' waiting entries, for a reload to carry, and
    /// as one each admitted kept answer that slept and no fresh answer has
    /// replaced: the replacement admits it again at once, keeping that
    /// history (LLP 1161.001 D1), where it would otherwise seed it afresh.
    pub(super) fn carry_waiting(&self) -> Vec<(String, String, Vec<Value>, u64, bool)> {
        (0..self.declared_resources().len())
            .filter_map(|i| {
                let row = self.resource_row(i);
                let name = self.plan.str(row.name);
                let (args, digest, slept) = match &self.resources[i].waiting {
                    Some(w) => (w.args.clone(), w.digest, w.slept),
                    None => {
                        let s = (self.resources[i].state.as_ref())
                            .filter(|s| s.slept && self.holds_kept(i, s))?;
                        let stored = self.store.kept(&super::kept::kept_name(name))?;
                        (s.args.clone(), digest(stored), true)
                    }
                };
                Some((
                    name.to_string(),
                    self.plan.str(row.source).to_string(),
                    args,
                    digest,
                    slept,
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
            if !eligible || row.transient {
                continue;
            }
            let (name, source) = (self.plan.str(row.name), self.plan.str(row.source));
            let Some((_, _, args, digest, slept)) =
                (carried.waiting.iter()).find(|(n, s, ..)| n == name && s == source)
            else {
                continue;
            };
            let stored = self
                .store
                .kept(&super::kept::kept_name(name))
                .map(self::digest);
            if let Some(seed) = self.resources[i].state.as_mut().filter(|s| s.kept_seed) {
                // This boot planted the same entry as a seed: it is compared
                // again, and keeps the sleep history it waited with.
                if seed.args == *args && stored == Some(*digest) {
                    seed.slept |= *slept;
                }
            } else {
                self.resources[i].waiting = Some(Waiting {
                    args: args.clone(),
                    digest: *digest,
                    slept: *slept,
                });
            }
        }
    }

    /// Journal resource `i`'s kept answer, kept for `kept`, consumed by a
    /// known mismatch with `now`: each differing identifying argument and
    /// the states and resources it reads. Once per resource and commit.
    fn refuse_kept(
        &mut self,
        i: usize,
        kept: &[Value],
        now: &[Value],
        valid: bool,
        states: &[Option<ResourceState>],
    ) {
        if self.keeping.refused.iter().any(|(r, ..)| *r == i) {
            return;
        }
        let differing: Vec<String> = (0..now.len())
            .filter(|&k| kept.get(k) != Some(&now[k]))
            .map(|k| {
                let inputs = self.arg_inputs(i, k, states);
                super::lines::kept_argument(k + 1, &inputs)
            })
            .collect();
        let unknown: Vec<String> = (0..now.len())
            .filter(|&k| self.arg_unknown(i, k, states))
            .map(|k| {
                let inputs = self.arg_inputs(i, k, states);
                super::lines::kept_argument_unknown(k + 1, &inputs)
            })
            .collect();
        let why = if kept.len() != now.len() {
            "its identifying arguments differ in count".to_string()
        } else if !differing.is_empty() {
            differing.join("; ")
        } else if super::kept::kept_surface(&self.plan, i) && !self.surface_seed_waiting(i) {
            "a live surface publication replaced it".to_string()
        } else if !valid {
            "its request arguments are invalid".to_string()
        } else {
            debug_assert!(!unknown.is_empty());
            unknown.join("; ")
        };
        let name = self.plan.str(self.resource_row(i).name);
        let line = super::lines::kept_refused(name, &why);
        self.keeping.refused.push((i, line, None));
    }

    /// The root states and resources resource `i`'s identifying argument
    /// `k` reads, directly or through derives. A store reader says whether
    /// it holds an admitted kept answer, a fresh one, or no admitted device
    /// answer (the build's or its placeholder).
    fn arg_inputs(&self, i: usize, k: usize, states: &[Option<ResourceState>]) -> Vec<String> {
        use crate::instance::Input;
        let mut named = Vec::new();
        self.identity_inputs(
            i,
            |j| j == k,
            |input| {
                let name = match input {
                    Input::Slot(s) => (self.plan.slots.get(s).zip(self.slots.get(s)))
                        .filter(|(slot, _)| slot.owner.is_none())
                        .map(|(slot, _)| format!("state {}", self.plan.str(slot.name))),
                    Input::Resource(r) | Input::PendingResource(r) | Input::FailedResource(r) => {
                        states.get(r).and_then(Option::as_ref).map(|s| {
                            let name = self.plan.str(self.resource_row(r).name);
                            let what = if self.holds_kept(r, s) {
                                " (holding an admitted kept answer)"
                            } else if self.on_fallback(r, states) {
                                " (a store reader with no admitted device answer)"
                            } else if self.resources[r].store_reader {
                                " (holding a fresh answer)"
                            } else {
                                ""
                            };
                            format!("resource {name}{what}")
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
