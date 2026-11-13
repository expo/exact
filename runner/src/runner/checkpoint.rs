//! A rendered document's checkpoint (LLP 1048.000 D6): the answers the
//! document was projected from, handed to the runtime as an input of their
//! own — not the store, not kept answers, not a reload's carried state.
use super::each::{PageAnswer, PageAnswers};
use super::{DataSource, ResourceState, Runner, RunnerError, Target};
use crate::delivery::Delivery;
use exact_kernel::Kernel;
use exact_plan::{Plan, Value};
use std::collections::HashSet;

/// The state a document was projected from. Never a store.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Checkpoint {
    /// The router location the document was rendered at.
    pub location: String,
    /// The render time: the clock the document was projected at (LLP
    /// 1048.000 D5), and every answer's freshness.
    pub now_ms: f64,
    /// The identity of the logic that answered (`DataSource::revision`).
    pub logic: Option<String>,
    /// (resource name, source name, arguments, answer) for every answer the
    /// document was projected from: the answer of each resource that had
    /// answered and, for a resource over a list, of each element that had
    /// answered, under the list's name (LLP 1017.000).
    pub answers: Vec<(String, String, Vec<Value>, Value)>,
    /// What was still pending then, and rendered its placeholder.
    pub pending: Vec<String>,
    /// The indices in `answers` of those whose stream was still open (LLP
    /// 1016.000 D5): the runtime shows each and asks it again, which
    /// subscribes the runtime to it.
    pub open: Vec<usize>,
    /// The indices in `answers` of those whose own ask read what the store
    /// holds, not only randomness: the runtime takes each as a store
    /// reader's, since the render never had the device's store. One that
    /// read the store only through its arguments is not: it is asked when
    /// they change (`Runner::asks_on_store_change`).
    pub readers: Vec<usize>,
}

impl<D: DataSource> Runner<D> {
    /// Boot from a rendered document's checkpoint (LLP 1048.000 D6). Each
    /// answer seeds its resource — the same name and source, answered by
    /// the same logic, a value that still fits — as an answer this runner
    /// already has: it isn't asked again at boot or at `data_ready`, only
    /// when its arguments or inputs change, as any answer is. The clock
    /// starts at the render time; the host's clock and store then apply as
    /// ordinary updates. An answer whose own ask read the store is a store
    /// reader's, since the render never had the device's store: it is asked
    /// again when the device's store holds the app's own entries, showing the
    /// rendered answer meanwhile, and whenever the store changes. An open
    /// stream's answer is asked again too: the render's stream ended with
    /// the render. An element of a resource over a list that the boot makes
    /// takes the answer under its list's name at its arguments; one that
    /// joins later asks, as any element that joins does.
    #[allow(clippy::too_many_arguments)] // the host boot facts
    pub fn boot_checkpoint(
        plan: Plan,
        data: D,
        kernel: Kernel,
        checkpoint: &Checkpoint,
        snapshot: Vec<(String, Vec<u8>)>,
        delivery: Delivery,
        viewport: crate::Viewport,
        launch: &str,
    ) -> Result<Runner<D>, RunnerError> {
        Runner::boot_checkpoint_linked(
            super::RunnerLinks::ALL,
            plan,
            data,
            kernel,
            checkpoint,
            snapshot,
            delivery,
            viewport,
            launch,
        )
        .map(Runner::linking_every_device)
    }

    /// [`Runner::boot_checkpoint`], with what the host links (LLP 1047 D3).
    #[allow(clippy::too_many_arguments)] // the host boot facts
    pub fn boot_checkpoint_linked(
        links: super::RunnerLinks,
        plan: Plan,
        data: D,
        kernel: Kernel,
        checkpoint: &Checkpoint,
        snapshot: Vec<(String, Vec<u8>)>,
        delivery: Delivery,
        viewport: crate::Viewport,
        launch: &str,
    ) -> Result<Runner<D>, RunnerError> {
        Runner::boot_inner(
            links,
            plan,
            data,
            kernel,
            super::Seed::Checkpoint(checkpoint),
            snapshot,
            delivery,
            viewport,
            launch,
        )
    }

    /// What a document projected from this runner now was projected from
    /// (LLP 1048.000 D6): every answer of the app's sources, the location,
    /// the clock, what is still pending, the open streams, and the answers
    /// whose own ask read the store. The runner's own facts (delivery, the
    /// viewport, surface records) are the runtime's to answer, never a
    /// checkpoint's, and so are a module's names: a render answers them from
    /// the web's declaration, never the reader's device (a battery, a
    /// permission's status), and a seeded answer is not asked again, so the
    /// device would never be read.
    pub fn document_checkpoint(&self, location: &str) -> Checkpoint {
        let mut pending: Vec<String> = (self.pending().into_iter())
            .filter(|(_, ticket)| !self.is_open_stream(*ticket))
            .map(|(name, _)| name)
            .collect();
        let mut open_rows = vec![false; self.resources.len()];
        for p in self.pending.iter().filter(|p| !p.in_flight()) {
            if let Target::Resource(i) = p.target {
                open_rows[i] = true;
            }
        }
        let (mut answers, mut open, mut readers) = (Vec::new(), Vec::new(), Vec::new());
        // Write each row's answer but a list's own, which no request gave: a
        // list's answers are its elements', whose rows follow the declared
        // ones under the list's name.
        for (i, resource) in self.resources.iter().enumerate() {
            if self.is_family(i) {
                continue;
            }
            let row = self.resource_row(i);
            let name = self.plan.str(row.name);
            let source = self.plan.str(row.source);
            if self.device_answers(source) {
                continue;
            }
            if resource.stale {
                if !pending.iter().any(|p| p == name) {
                    pending.push(name.to_string());
                }
                continue;
            }
            if let Some(state) =
                (resource.state.as_ref()).filter(|state| !resource.pending && !state.placeholder)
            {
                answers.push((
                    name.to_string(),
                    source.to_string(),
                    state.args.clone(),
                    state.value.get(&self.plan).clone(),
                ));
                if open_rows[i] {
                    open.push(answers.len() - 1);
                }
                if self.asks_on_store_change(i) {
                    readers.push(answers.len() - 1);
                }
            }
        }
        Checkpoint {
            location: location.to_string(),
            now_ms: self.now_ms,
            logic: self.data.revision().map(str::to_owned),
            answers,
            pending,
            open,
            readers,
        }
    }

    /// Seed each resource `checkpoint` answered (see [`Runner::boot_checkpoint`]),
    /// and hold each list's answers for the elements the boot makes. Returns
    /// which rows it seeded, and those of them it asks again.
    pub(super) fn seed_checkpoint(&mut self, checkpoint: &Checkpoint) -> (Vec<bool>, Vec<usize>) {
        let mut seeded = vec![false; self.resources.len()];
        let mut again = Vec::new();
        if checkpoint.logic.as_deref() != self.data.revision() {
            return (seeded, again);
        }
        // Ask an answer again when its stream was open, or when it read the
        // store and the device's store opened with an entry a source can
        // read, which the render never had (LLP 1027 D4)
        let device_state = self.store.opened_with_secrets();
        let set = |indices: &[usize]| indices.iter().copied().collect::<HashSet<usize>>();
        let (open, readers) = (set(&checkpoint.open), set(&checkpoint.readers));
        let reads = |k: usize| readers.contains(&k);
        let asks = |k: usize| open.contains(&k) || device_state && reads(k);
        for (i, seed) in seeded.iter_mut().enumerate() {
            let row = self.resource_row(i);
            let (name, source) = (self.plan.str(row.name), self.plan.str(row.source));
            if self.resources[i].state.is_some() || self.device_answers(source) {
                continue;
            }
            let mut answers =
                (checkpoint.answers.iter().enumerate()).filter(|(_, (n, s, args, _))| {
                    n == name && s == source && args.len() == row.args.len as usize
                });
            if self.is_family(i) {
                let element_type = self.plan.types[row.ty.0 as usize]
                    .elem
                    .expect("a list type");
                let elements = (answers
                    .filter(|(_, (.., value))| self.conforms(value, element_type)))
                .map(|(k, (_, _, args, value))| PageAnswer {
                    args: args.clone(),
                    value: value.clone(),
                    again: asks(k),
                    reads: reads(k),
                })
                .collect();
                self.resources[i].page_answers =
                    Some(Box::new(PageAnswers::new(elements, self.store.revision())));
                continue;
            }
            let Some((k, (_, _, args, value))) = answers.next() else {
                continue;
            };
            if self.check_shape(i, value).is_err() {
                continue;
            }
            self.resources[i].state = Some(ResourceState {
                inspection_generation: 0,
                args: args.clone(),
                value: crate::held::Held::new(value.clone()),
                store_revision: self.store.revision(),
                placeholder: false,
                kept_seed: false,
                answered_for: None,
                asleep: false,
                slept: false,
                origin: 0,
            });
            *seed = true;
            self.seed_reads(i, reads(k));
            if asks(k) {
                again.push(i);
            }
        }
        (seeded, again)
    }

    /// End the checkpoint's seeding once the boot has settled: the held
    /// answers are dropped, so an element that joins later asks.
    /// Returns the journal's line for it, which boot writes after its own.
    pub(super) fn end_checkpoint_seeding(
        &mut self,
        checkpoint: &Checkpoint,
        seeded: &[bool],
    ) -> String {
        if checkpoint.logic.as_deref() != self.data.revision() {
            return "checkpoint: other logic answered it; its answers are asked again".into();
        }
        let elements: usize = (self.resources.iter_mut())
            .filter_map(|row| row.page_answers.take())
            .map(|page| page.taken())
            .sum();
        let taken = seeded.iter().filter(|s| **s).count() + elements;
        super::lines::seeded(taken, checkpoint.answers.len())
    }

    /// Whether the runtime answers `source`, never a checkpoint: the
    /// runner's own facts and a module's names.
    fn device_answers(&self, source: &str) -> bool {
        exact_plan::runner_owned_source(source) || self.plan.module_named(source).is_some()
    }
}
