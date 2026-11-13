//! A resource over a list (LLP 1017.000 P4c, amended 2026-11-04): `resource
//! r = source(x) for x in list [key=k] as shape S`. The runner keeps one
//! request for each element, keyed as a `for` row is: each element is a
//! member, a runtime row of its own after the declared resources, that asks
//! the source with the element as its one parameter and settles as any
//! resource does. An element that joins adds a member; one that leaves lets
//! only its member's request go; the others go on. `r` itself asks nothing:
//! it reads its members' answers as `S[]` in the list's order, pending or
//! failed when any member is.

use super::{DataSource, Runner, RunnerError, Target};
use crate::held::Held;
use crate::vm::{self, Env, Trap};
use exact_plan::Value;

/// One element's request.
#[derive(Clone)]
pub(super) struct Member {
    pub(super) row: exact_plan::ResourcesRow,
    /// The resource over the list.
    pub(super) family: usize,
    /// The element's key, as [`crate::instance::key_text`] writes it.
    pub(super) key: String,
    /// The element, the one parameter its arguments read.
    pub(super) item: Value,
    /// Its element left the list: its request was let go, and it settles
    /// to nothing until an element that joins the family takes its row.
    pub(super) gone: bool,
}

/// A rendered page's answer for one element of a list.
#[derive(Clone)]
pub(super) struct PageAnswer {
    pub(super) args: Vec<Value>,
    pub(super) value: Value,
    /// It is asked again: its stream was open, or it read the store and the
    /// device's store holds the app's own entries.
    pub(super) again: bool,
    /// It read the store, and the render never had the device's store.
    pub(super) reads: bool,
}

/// A rendered page's answers for one list's elements.
#[derive(Clone)]
pub(super) struct PageAnswers {
    answers: Vec<PageAnswer>,
    /// The answer after the last one taken.
    next: usize,
    /// Which answers an element took.
    taken: Vec<bool>,
    /// The store's revision when the boot took the page's answers, which an
    /// element's answer stands for, as a declared seed's does: a store write
    /// in the boot's own settlement asks a store reader again.
    store_revision: u64,
}

impl PageAnswers {
    pub(super) fn new(answers: Vec<PageAnswer>, store_revision: u64) -> PageAnswers {
        let taken = vec![false; answers.len()];
        PageAnswers {
            answers,
            next: 0,
            taken,
            store_revision,
        }
    }

    /// An answer at `args`. Elements with equal arguments make the same
    /// request, so each takes an answer at those arguments, in the page's
    /// order. Look first at the answer after the last one taken: the boot
    /// makes the elements in the list's order, and the page wrote their
    /// answers in the order of their rows, which is the list's unless the
    /// list changed during the render. An element the list moved is found by
    /// a scan; one the render did not answer costs a scan that finds
    /// nothing.
    fn take(&mut self, args: &[Value]) -> Option<&PageAnswer> {
        let count = self.answers.len();
        let at = (0..count)
            .map(|step| (self.next + step) % count)
            .find(|&at| self.answers[at].args == args)?;
        self.next = at + 1;
        self.taken[at] = true;
        Some(&self.answers[at])
    }

    /// How many of the answers an element took.
    pub(super) fn taken(&self) -> usize {
        self.taken.iter().filter(|taken| **taken).count()
    }
}

impl<D: DataSource> Runner<D> {
    /// The resource rows the plan declares; the members follow them.
    pub fn declared_resources(&self) -> &[exact_plan::ResourcesRow] {
        &self.plan.resources
    }

    /// Whether row `i` is a resource over a list.
    pub(super) fn is_family(&self, i: usize) -> bool {
        self.resource_row(i).each.len != 0
    }

    /// Row `i`'s element, when it is a member.
    pub(super) fn member(&self, i: usize) -> Option<&Member> {
        self.resources.get(i).and_then(|row| row.member.as_ref())
    }

    /// Member `i`'s state at `args` from the page the runner boots from,
    /// while its list's answers are held. Make it a store reader when its
    /// answer read the store, and ask it again as a seeded resource is, when
    /// the page says so: now when its source can answer (`force`), as the
    /// launch's own ask, not a refresh (`ask_at_launch`, LLP 1162.000 D1),
    /// else at `data_ready`.
    pub(super) fn seed_member(
        &mut self,
        i: usize,
        args: &[Value],
        force: &mut Vec<usize>,
    ) -> Option<super::ResourceState> {
        let family = self.member(i)?.family;
        let page = self.resources[family].page_answers.as_mut()?;
        let store_revision = page.store_revision;
        let answer = page.take(args)?;
        let (value, again, reads) = (answer.value.clone(), answer.again, answer.reads);
        self.seed_reads(i, reads);
        if again && self.source_ready(self.plan.str(self.resource_row(i).source)) {
            self.launch.push(i);
            force.push(i);
        } else if again {
            self.resources[i].stale = true;
        }
        Some(super::ResourceState {
            inspection_generation: 0,
            args: args.to_vec(),
            value: Held::new(value),
            store_revision,
            placeholder: false,
            kept_seed: false,
            answered_for: None,
            asleep: false,
            slept: false,
            origin: 0,
        })
    }

    /// What row `i`'s arguments read as parameters: a member's element.
    pub(super) fn member_params(&self, i: usize) -> &[Value] {
        self.member(i)
            .map_or(&[], |m| std::slice::from_ref(&m.item))
    }

    /// Family `i`'s elements in the list's order: each element's key and
    /// the element, read against this pass's values.
    pub(super) fn elements(
        &self,
        i: usize,
        env: &Env<'_>,
    ) -> Result<Vec<(String, Value)>, RunnerError> {
        let (list, key) = self
            .plan
            .resource_each(self.resource_row(i))
            .expect("a family");
        let Value::List(items) = vm::eval(self.plan.code(list), env, &[])
            .map_err(|t| {
                RunnerError::from(t).evaluated(|| {
                    format!(
                        "resource {:?} each subject",
                        self.plan.str(self.resource_row(i).name)
                    )
                })
            })?
            .value
        else {
            return Err(RunnerError::from(Trap::TypeMismatch {
                pc: 0,
                op: exact_plan::Opcode::LoadResource,
            })
            .evaluated(|| {
                format!(
                    "resource {:?} each subject",
                    self.plan.str(self.resource_row(i).name)
                )
            }));
        };
        let mut out = Vec::with_capacity(items.len());
        for item in items.iter() {
            let params = std::slice::from_ref(item);
            let key = vm::eval(self.plan.code(key), &Env { params, ..*env }, &[])
                .map_err(|t| {
                    RunnerError::from(t).evaluated(|| {
                        format!(
                            "resource {:?} each key",
                            self.plan.str(self.resource_row(i).name)
                        )
                    })
                })?
                .value;
            let Some(key) = crate::instance::key_text(&key) else {
                let resource = self.plan.str(self.resource_row(i).name).to_string();
                return Err(RunnerError::ResourceKey { resource });
            };
            out.push((key, item.clone()));
        }
        Ok(out)
    }

    /// The members of family `i` for `elements`, in their order (an element
    /// whose key repeats an earlier one's reads that one's answer), adding a
    /// member for each new key; the family's live members no element keys,
    /// which leave; and the members added.
    pub(super) fn join(
        &mut self,
        i: usize,
        elements: Vec<(String, Value)>,
    ) -> (Vec<usize>, Vec<usize>, Vec<usize>) {
        let mut order = Vec::with_capacity(elements.len());
        let mut added = Vec::new();
        for (key, item) in elements {
            let found =
                (self.resources.iter().map(|row| &row.member).enumerate()).find_map(|(m, row)| {
                    row.as_ref()
                        .filter(|row| row.family == i && !row.gone && row.key == key)
                        .map(|_| m)
                });
            let m = match found {
                Some(m) => m,
                None => {
                    let m = self.add_member(i, key, item.clone());
                    added.push(m);
                    m
                }
            };
            if self.resources[m].state.is_none() {
                self.join_member_writes(i, m);
            }
            // A repeated key reads the first element's request.
            if !order.contains(&m) {
                if let Some(Some(row)) = self.resources.get_mut(m).map(|row| &mut row.member) {
                    row.item = item;
                }
            }
            order.push(m);
        }
        let leaving = (self.resources.iter().map(|row| &row.member).enumerate())
            .filter(|(m, row)| {
                row.as_ref()
                    .is_some_and(|row| row.family == i && !row.gone && !order.contains(m))
            })
            .map(|(m, _)| m)
            .collect();
        (order, leaving, added)
    }

    /// A member of family `i` for the element `item` keyed `key`: a copy of
    /// the family's row that asks for one element and answers its type. It
    /// takes the row of a member whose element left, when one has no
    /// request in flight (a child a tool started can outlive its element,
    /// LLP 1133.007 D13), so a family keeps no more rows than the elements
    /// one settlement holds: those that stay, leave and join.
    fn add_member(&mut self, i: usize, key: String, item: Value) -> usize {
        let free =
            (self.resources.iter().map(|row| &row.member).enumerate()).position(|(m, row)| {
                row.as_ref().is_some_and(|row| row.family == i && row.gone)
                    && !self.pending.iter().any(|p| p.target == Target::Resource(m))
            });
        let mut row = self.resource_row(i).clone();
        row.ty = self.plan.types[row.ty.0 as usize]
            .elem
            .expect("a list type");
        row.each = Default::default();
        let resource = super::resources::ResourceRow {
            member: Some(Member {
                row,
                family: i,
                key,
                item,
                gone: false,
            }),
            ..Default::default()
        };
        if let Some(m) = free {
            self.refresh_next.retain(|r| *r != m);
            self.forget_resource_row(m);
            self.resources[m] = resource;
            m
        } else {
            let m = self.resources.len();
            self.resources.push(resource);
            m
        }
    }

    /// Family `i`'s value from its members' this pass: their shown values as a
    /// list, pending when any is, failed (with the first failure) when any
    /// is.
    pub(super) fn gather(
        &mut self,
        i: usize,
        order: &[usize],
        resources: &[Option<Held>],
        pending: &[bool],
    ) -> (Value, bool) {
        let value = Value::list(
            (order.iter())
                .map(|&m| {
                    resources[m]
                        .as_ref()
                        .expect("settled")
                        .get(&self.plan)
                        .clone()
                })
                .collect(),
        );
        let failed = order
            .iter()
            .find(|&&m| self.resources[m].failed_args.is_some());
        self.resources[i].failed_args = failed.map(|_| Vec::new());
        self.resources[i].failed_why = failed.and_then(|&m| self.resources[m].failed_why.clone());
        (value, order.iter().any(|&m| pending[m]))
    }

    /// Let go of each member in `leaving`: its request goes, and it holds
    /// nothing from now on.
    pub(super) fn let_go_members(&mut self, leaving: &[usize]) {
        for &m in leaving {
            self.forget(Target::Resource(m));
            let mut member = self.resources[m].member.take().expect("a member");
            member.gone = true;
            self.resources[m] = super::resources::ResourceRow {
                member: Some(member),
                ..Default::default()
            };
            self.refresh_next.retain(|r| *r != m);
        }
        self.forget_member_writes(leaving);
        self.sync_pending_flags();
    }

    /// A family is pending while any of its live members is.
    pub(super) fn pending_families(&mut self) {
        for m in 0..self.resources.len() {
            if let Some(row) = self.resources[m].member.as_ref().filter(|row| !row.gone) {
                let family = row.family;
                self.resources[family].pending |= self.resources[m].pending;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Answer, DataError, Store};
    use exact_kernel::Kernel;
    /// `look(u)` answers at once, with `u` as the phase.
    struct Look;

    impl DataSource for Look {
        fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
            Err(DataError::UnknownSource(source.into()))
        }

        fn answer(&mut self, _: &mut Store, _: &str, args: &[Value]) -> Result<Answer, DataError> {
            Ok(Answer::Now(Value::record(vec![args[0].clone()])))
        }
    }

    /// A member whose element left is a row a later element takes: a window
    /// moved half a window at a time, many times over and back again, keeps as
    /// many rows as one move has elements (those that stay, leave and join). The
    /// list reads a resource declared after it, so its elements are read in a
    /// later sweep than the members whose elements left.
    #[test]
    fn a_list_whose_elements_change_keeps_rows_for_the_elements_live() {
        let plan = contract::compile(
        "shape Got\n  phase: string\ncomponent App\n  state ids: string[] = []\n  resource got = look(u) for u in ids.filter(x => x != skip.phase) as shape Got\n  resource skip = look(\"none\") as shape Got\n  action show(xs: string[])\n    ids = xs\n  derive phases = got.map(g => g.phase).join(\",\")\n  view\n    text phases\n",
    )
    .unwrap();
        let mut r = Runner::boot(
            plan,
            Look,
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        let declared = r.declared_resources().len();
        let mut most = 0;
        for round in 0..300 {
            // Pages 0, 1, 0, 1, 2, 1, 2, 3, 2…: keys leave and return.
            let page = round / 3 + usize::from(round % 3 == 1);
            let ids: Vec<String> = (page * 25..page * 25 + 50).map(|n| n.to_string()).collect();
            let list = Value::list(ids.iter().map(|s| Value::str(s)).collect());
            r.act("show", vec![list]).unwrap();
            let phases = r.derive("phases").unwrap().as_str().unwrap().to_string();
            assert_eq!(phases, ids.join(","), "round {round}");
            most = most.max(r.resources.len());
        }
        assert_eq!(r.declared_resources().len(), declared);
        assert!(
            most <= declared + 75,
            "{most} rows for 50 elements live and 25 joining"
        );
    }
}
