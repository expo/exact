//! State carried across a restart (LLP 1007 §6, LLP 1027 D5).
use super::{Runner, RunnerError, RunnerLinks, Seed};
use exact_kernel::Kernel;
use exact_plan::{Plan, Value};

/// What survives a reload: slots whose names/types still fit, settled
/// resources with matching sources, arguments and logic identity, app secrets, and
/// the clock. A logic edit re-asks resources rather than preserving old answers.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Carried {
    /// The session plan number (LLP 1029.000.002 D10).
    pub generation: u64,
    /// Highest view id issued in the session, including views since destroyed.
    pub last_view_id: exact_kernel::ViewId,
    /// Views the candidate can keep at matching source places (LLP 1029.000.002 D3).
    pub views: crate::instance::RetainedViews,
    /// Reading positions of virtualized lists, by their authored place.
    pub lists: crate::instance::RetainedLists,
    /// The last published kernel epoch.
    pub epoch: u64,
    /// A fresh reboot keeps the sequence but initializes state and reads the host store anew.
    pub fresh_state: bool,
    /// Picked paths survive reloads, including copies still being imported.
    pub picked_count: u64,
    /// Router slot name and its value decoded through the old plan's shapes.
    /// @ref LLP 1038 D5 — preserves named params when their field order changes.
    pub router: Option<(String, exact_route::Router)>,
    /// The replaceable data module that produced these resource answers.
    pub data_revision: Option<String>,
    /// Preserve kept-answer persistence when a live candidate is already loaded.
    pub keeps_answers: bool,
    /// Slot name → value.
    pub slots: Vec<(String, Value)>,
    /// Tab panels' state (LLP 1075.003 §9.14): (panel `id`, slot name, value).
    pub panels: crate::instance::tabs::CarriedPanels,
    /// Resource name → (source name, arguments, value).
    pub resources: Vec<(String, String, Vec<Value>, Value)>,
    /// Kept entries waiting for their identifying arguments (LLP 1027.005
    /// D3, 2026-11-13): (resource name, source name, identifying arguments,
    /// digest of the stored entry, whether it was kept across idleness).
    pub waiting: Vec<(String, String, Vec<Value>, u64, bool)>,
    /// The resources whose answer came from a stream still open: a
    /// replacement that takes the answer shows it and asks again, opening
    /// the stream anew, since the old one ends with the runner that had it.
    pub streams: Vec<String>,
    /// (Resource name, source name) pairs whose answer depends on the store.
    pub store_readers: Vec<(String, String)>,
    /// (Resource name, source name) pairs whose answers are kept (LLP 1027
    /// D4): the store readers and what the plan keeps whatever it reads
    /// (`kept::keeps_answers`). A reload keeps only their kept entries.
    pub keepers: Vec<(String, String)>,
    /// Of `store_readers`, those asked in the run that carried them, and
    /// whether each read what the store holds: one that only drew randomness
    /// is not asked again by a store change after the reload either
    /// (LLP 1069.005 D2).
    pub read_contents: Vec<(String, String, bool)>,
    /// The clock, milliseconds.
    pub now_ms: f64,
    /// The store's kept values (LLP 1018): what the host has persisted.
    pub store: Vec<(String, Vec<u8>)>,
    /// Each queue's waiting sends, by mutation name: never carried, only
    /// said once the new runner boots (LLP 1092 D4).
    pub forgot_waiting: Vec<(String, usize)>,
    /// The driver's fetch faults as they are (LLP 1103 D3, `Faults::spec`),
    /// so a replacement neither revives a spent fault nor loses an armed one.
    /// A native host's replacement then shares the live table at its commit
    /// (`Runner::share_faults`).
    pub faults: Option<String>,
    /// The window's app modes, (axis, value) (LLP 1121 D5): an app's
    /// `setMode` or the agent's `prefer mode` outlives a reload.
    pub modes: Vec<(String, String)>,
    /// The uploads asked and how each stands (LLP 1016.000 D10), whatever
    /// the data module: none is sent again at its resource and arguments,
    /// and one still on its way is cut off.
    pub uploads: Vec<crate::UploadMark>,
}

impl Carried {
    /// Take a disposable validation runner's answers, keeping the live session's
    /// sequence, state and router for the committed replacement.
    pub fn with_validated_answers(mut self, validated: Self) -> Self {
        self.data_revision = validated.data_revision;
        self.keeps_answers = validated.keeps_answers;
        self.resources = validated.resources;
        self.waiting = validated.waiting;
        self.streams = validated.streams;
        self.store_readers = validated.store_readers;
        self.keepers = validated.keepers;
        self.read_contents = validated.read_contents;
        self
    }

    pub(super) fn seed(&self) -> Seed<'_> {
        if self.fresh_state {
            Seed::Reboot(self)
        } else {
            Seed::Carried(self)
        }
    }
}

impl<D: super::DataSource> super::Runner<D> {
    /// Boot a new plan with the state of an old runner (a dev reload that
    /// keeps its place — LLP 1007 §6). The new tree keeps compatible views'
    /// ids. Timers and derives start fresh, and matching slots and settled
    /// resources take their carried values.
    pub fn boot_carrying(
        plan: Plan,
        data: D,
        kernel: Kernel,
        carried: &Carried,
        viewport: crate::Viewport,
        launch: &str,
    ) -> Result<Runner<D>, RunnerError> {
        Runner::boot_inner(
            RunnerLinks::ALL,
            super::ModuleLinks::ALL,
            plan,
            data,
            kernel,
            carried.seed(),
            carried.store.clone(),
            Default::default(),
            viewport,
            launch,
        )
        .map(Runner::linking_every_device)
    }

    /// (Resource name, source name) of each declared resource `which` names.
    fn declared_where(&self, which: impl Fn(usize) -> bool) -> Vec<(String, String)> {
        (0..self.declared_resources().len())
            .filter(|&i| which(i))
            .map(|i| {
                let row = &self.plan.resources[i];
                let (name, source) = (self.plan.str(row.name), self.plan.str(row.source));
                (name.to_string(), source.to_string())
            })
            .collect()
    }

    /// Session identity for a fresh reboot, without carrying application state.
    pub fn reboot_sequence(&self) -> Carried {
        Carried {
            generation: self.generation,
            last_view_id: self.ids.last_issued(),
            epoch: self.kernel.epoch(),
            fresh_state: true,
            ..Carried::default()
        }
    }

    /// Everything a reload keeps.
    pub fn carry(&self) -> Carried {
        Carried {
            generation: self.generation,
            last_view_id: self.ids.last_issued(),
            epoch: self.kernel.epoch(),
            fresh_state: false,
            views: self
                .tree
                .as_ref()
                .map(|tree| tree.retainable_views(&self.plan, &self.kernel, &self.ids))
                .unwrap_or_default(),
            lists: self
                .tree
                .as_ref()
                .map(|tree| tree.carried_lists(&self.plan, &self.kernel))
                .unwrap_or_default(),
            picked_count: self.picked_count,
            router: self.carry_router(),
            data_revision: self.data.revision().map(str::to_owned),
            keeps_answers: self.keeping.on,
            slots: self
                .plan
                .slots
                .iter()
                .zip(&self.slots)
                .filter(|(s, _)| s.owner.is_none())
                .map(|(s, v)| (self.plan.str(s.name).to_string(), v.clone()))
                .collect(),
            panels: self.carry_panels(),
            resources: self
                .resource_rows()
                .zip(self.resources.iter().map(|row| &row.state))
                .take(self.declared_resources().len())
                .enumerate()
                // A request in flight or a placeholder shown until the
                // source is ready is not an answer to carry, nor one a
                // write affects: the replacement runner asks it.
                .filter(|(i, _)| {
                    !self.resources[*i].pending && !self.resources[*i].stale && !self.written(*i)
                })
                // Nor is a kept answer no fresh one has replaced (D5): the
                // replacement shows it from the store and asks again.
                .filter(|(i, (_, s))| {
                    s.as_ref()
                        .is_none_or(|s| !s.placeholder && !self.holds_kept(*i, s))
                })
                .filter_map(|(_, (r, s))| {
                    s.as_ref().map(|s| {
                        (
                            self.plan.str(r.name).to_string(),
                            self.plan.str(r.source).to_string(),
                            s.args.clone(),
                            s.value.get(&self.plan).clone(),
                        )
                    })
                })
                .collect(),
            waiting: self.carry_waiting(),
            streams: (self.pending.iter())
                .filter(|p| !p.in_flight())
                .filter_map(|p| match p.target {
                    super::Target::Resource(i) if i < self.declared_resources().len() => {
                        Some(self.target_name(p.target))
                    }
                    _ => None,
                })
                .collect(),
            store_readers: self.declared_where(|i| self.resources[i].store_reader),
            keepers: self.declared_where(|i| {
                self.resources[i].store_reader || super::kept::keeps_answers(&self.plan, i)
            }),
            read_contents: self
                .resource_rows()
                .take(self.declared_resources().len())
                .enumerate()
                .filter(|(i, _)| self.resources[*i].store_reader && self.resources[*i].reads_known)
                .map(|(i, resource)| {
                    (
                        self.plan.str(resource.name).to_string(),
                        self.plan.str(resource.source).to_string(),
                        self.resources[i].contents_reader,
                    )
                })
                .collect(),
            now_ms: self.now_ms,
            store: self.store.snapshot(),
            forgot_waiting: self.queued(),
            faults: (!self.faults.is_empty()).then(|| self.faults.spec()),
            modes: self.modes(),
            uploads: self.carried_uploads(),
        }
    }
}

/// The root-slot part of the replacement report (LLP 1029.000.002 piece 1).
#[derive(Debug)]
pub(super) struct Replacement {
    at: f64,
    dropped: Vec<(String, &'static str)>,
}

impl<D: super::DataSource> Runner<D> {
    /// The committed plan number within this session; a candidate cannot advance its predecessor.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub(super) fn record_replacement(&mut self, carried: Option<&Carried>) {
        let Some(carried) = carried.filter(|c| c.generation > 0) else {
            return;
        };
        let dropped =
            carried
                .slots
                .iter()
                .filter_map(|(name, value)| {
                    let row =
                        self.plan.slots.iter().enumerate().find(|(_, row)| {
                            row.owner.is_none() && self.plan.str(row.name) == name
                        });
                    let why = match row {
                        None => "no place",
                        Some((i, _)) if self.plan.router == Some(exact_plan::SlotsId(i as u32)) => {
                            return None
                        }
                        Some((_, row)) if !value.conforms(&self.plan, row.ty) => "type changed",
                        _ => return None,
                    };
                    Some((name.clone(), why))
                })
                .collect();
        self.replacement = Some(Replacement {
            at: self.now_ms,
            dropped,
        });
        let mut report = String::new();
        self.replacement_json(&mut report);
        self.log(format!(
            "replacement generation {}: {report}",
            self.generation
        ));
    }

    /// The host's word on what a replacement waits for (LLP 1029.000.002
    /// D9), which `state.replacement.pending` reports.
    pub fn set_replacement_pending(&mut self, pending: Option<&'static str>) {
        self.replacement_pending = pending;
    }

    pub(crate) fn view_place(
        &self,
        view: exact_kernel::ViewId,
    ) -> Option<Vec<crate::instance::place::Part>> {
        let mut found = None;
        self.tree
            .as_ref()?
            .visit_places(&self.plan, &self.kernel, |node, path, _| {
                if node.view == view {
                    found = Some(path.to_vec());
                }
            });
        found
    }

    /// `state.replacement`: the last replacement's report and what a waiting
    /// one waits for; null before any replacement while none waits.
    pub(crate) fn replacement_state_json(&self, out: &mut String) {
        if self.replacement.is_none() && self.replacement_pending.is_none() {
            out.push_str("null");
            return;
        }
        if self.replacement.is_some() {
            self.replacement_json(out);
            out.pop();
            out.push(',');
        } else {
            out.push('{');
        }
        out.push_str("\"pending\":");
        match self.replacement_pending {
            Some(pending) => crate::json::quote(pending, out),
            None => out.push_str("null"),
        }
        out.push('}');
    }

    pub(crate) fn replacement_json(&self, out: &mut String) {
        use std::fmt::Write;
        let Some(report) = &self.replacement else {
            out.push_str("null");
            return;
        };
        let _ = write!(
            out,
            "{{\"at\":{},\"dropped\":[",
            crate::json::num(report.at)
        );
        for (i, (name, why)) in report.dropped.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str("{\"what\":\"state\",\"name\":");
            crate::json::quote(name, out);
            out.push_str(",\"why\":");
            crate::json::quote(why, out);
            out.push('}');
        }
        out.push_str("]}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Answer, DataError, DataSource, Runner, Store};
    use exact_kernel::{Kernel, NodeType};
    use exact_plan::{builder::PlanBuilder, Plan, TypeKind};

    #[derive(Default)]
    struct Source {
        queries: usize,
        deferred: bool,
        pending: bool,
    }
    impl DataSource for Source {
        fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
            Err(DataError::UnknownSource(source.into()))
        }
        fn answer(
            &mut self,
            store: &mut Store,
            source: &str,
            _: &[Value],
        ) -> Result<Answer, DataError> {
            self.queries += 1;
            if self.pending && source == "fresh" {
                return Ok(Answer::Later(crate::Request::get(
                    "https://example.test/answer",
                )));
            }
            Ok(Answer::Now(Value::str(if source == "remember" {
                store.get("token").unwrap_or("")
            } else {
                source
            })))
        }
        fn grants(&self) -> &str {
            "secret.keep token\n"
        }
        fn ready(&self) -> bool {
            !self.deferred
        }
        fn revision(&self) -> Option<&str> {
            Some("unchanged-module")
        }
    }

    fn plan(source: &str, padding: bool) -> Plan {
        let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
        if padding {
            b.str("unrelated new declaration");
        }
        let string = b.primitive(TypeKind::String);
        b.resource("answer", source, &[], string, None);
        b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
        b.finish().unwrap()
    }

    #[test]
    fn replacements_count_generations_continue_epochs_and_report_root_drops() {
        let plan = |second: bool| {
            contract::compile(&format!("component App\n  state kept = 1\n  state retyped = {}\n{}  action bump\n    kept = kept + 1\n  view\n    text `${{kept}}`\n", if second { "\"new\"" } else { "1" }, if second { "" } else { "  state removed = 1\n" })).unwrap()
        };
        let mut runner = Runner::boot(
            plan(false),
            (),
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        assert_eq!(
            crate::agent::tagged(&runner, "{}".into()),
            crate::agent::tags(&runner)
        );
        for generation in 1..=4 {
            for op in ["tags", "tree", "state", "logs", "unknown"] {
                let reply = crate::agent::handle(&runner, &format!("{{\"op\":\"{op}\"}}"));
                assert_eq!(
                    crate::json::field_num(&reply, "generation"),
                    Some(generation as f64),
                    "{reply}"
                );
            }
            let state = crate::agent::handle(&runner, "{\"op\":\"state\"}");
            assert_eq!(
                crate::json::field_num(&state, "generation"),
                Some(generation as f64)
            );
            assert_eq!(runner.kernel().incarnation(), 1);
            if generation == 1 {
                assert!(state.contains("\"replacement\":null"));
                // What the host says a waiting replacement waits for (LLP 1029.000.002 D9).
                runner.set_replacement_pending(Some("pointer"));
                let held = crate::agent::handle(&runner, "{\"op\":\"state\"}");
                assert!(
                    held.contains("\"replacement\":{\"pending\":\"pointer\"}"),
                    "{held}"
                );
                runner.set_replacement_pending(None);
            }
            if generation == 2 {
                assert!(state.contains("\"replacement\":{\"at\":50,"));
                assert!(
                    state.contains("\"why\":\"no place\"}],\"pending\":null}"),
                    "{state}"
                );
                assert!(runner
                    .journal()
                    .next()
                    .unwrap()
                    .contains("replacement generation 2:"));
                assert!(
                    state.contains(
                        "{\"what\":\"state\",\"name\":\"retyped\",\"why\":\"type changed\"}"
                    ),
                    "{state}"
                );
                assert!(
                    state
                        .contains("{\"what\":\"state\",\"name\":\"removed\",\"why\":\"no place\"}"),
                    "{state}"
                );
                assert!(!state.contains("\"name\":\"kept\""));
            }
            if generation == 4 {
                break;
            }
            runner.act("bump", vec![]).unwrap();
            runner.advance(50.0 * generation as f64).unwrap();
            let epoch = runner.kernel().epoch();
            let carried = runner.carry();
            let mut invalid = plan(false);
            invalid.kernel_schema_digest ^= 1;
            assert!(Runner::boot_carrying(
                invalid,
                (),
                Kernel::with_monospace(),
                &carried,
                Default::default(),
                "/"
            )
            .is_err());
            assert_eq!(
                runner.carry(),
                carried,
                "a refused candidate changes nothing"
            );
            runner = Runner::boot_carrying(
                plan(generation == 1),
                (),
                Kernel::with_monospace(),
                &carried,
                Default::default(),
                "/",
            )
            .unwrap();
            assert!(runner.kernel().epoch() > epoch);
        }
        let previous = runner.reboot_sequence();
        let rebooted = Runner::boot_with_delivery(
            plan(false),
            (),
            Kernel::with_monospace(),
            Some(&previous),
            vec![],
            Default::default(),
            Default::default(),
            "/",
        )
        .unwrap();
        assert_eq!(rebooted.generation(), 5);
        assert!(rebooted.kernel().epoch() > runner.kernel().epoch());
        assert_eq!(rebooted.slot("kept"), Some(&Value::Number(1.0)));
    }

    #[test]
    fn inspection_identity_is_lazy_and_owned_by_one_immutable_plan() {
        let runner = Runner::boot(
            plan("fresh", false),
            Source::default(),
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        let id = runner.roots()[0];
        assert!(runner.inspection_digest.get().is_none());
        crate::agent::handle(&runner, &format!("{{\"op\":\"node\",\"id\":{id}}}"));
        crate::agent::handle(&runner, "{\"op\":\"node\",\"id\":4294967295,\"plan\":true}");
        assert!(
            runner.inspection_digest.get().is_none(),
            "ordinary and stale node reads must not encode the plan"
        );
        let first = crate::agent::handle(
            &runner,
            &format!("{{\"op\":\"node\",\"id\":{id},\"plan\":true}}"),
        );
        let cached = runner.inspection_digest.get().unwrap();
        assert_eq!(
            crate::json::field_str(&first, "planDigest").as_deref(),
            Some(cached.as_str())
        );
        let address = cached.as_ptr();
        crate::agent::handle(
            &runner,
            &format!("{{\"op\":\"node\",\"id\":{id},\"plan\":true}}"),
        );
        assert_eq!(runner.inspection_digest.get().unwrap().as_ptr(), address);
        let replacement = Runner::boot(
            plan("fresh", true),
            Source::default(),
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        assert!(replacement.inspection_digest.get().is_none());
        assert_ne!(replacement.inspection_digest(), cached);
    }

    #[test]
    fn carried_answers_and_store_dependencies_follow_source_names_not_string_ids() {
        let original = Runner::boot_stored(
            plan("remember", false),
            Source::default(),
            Kernel::with_monospace(),
            vec![("token".into(), "stored answer".into())],
            Default::default(),
            "/",
        )
        .unwrap();
        assert_eq!(
            original.resource("answer"),
            Some(&Value::str("stored answer"))
        );
        assert!(original.resource_reads_store("answer"));
        let carried = original.carry();
        for (source, padding, reused) in [("remember", true, true), ("fresh", false, false)] {
            let next = plan(source, padding);
            assert_eq!(
                original.plan().resources[0].source == next.resources[0].source,
                !reused
            );
            let mut reloaded = Runner::boot_carrying(
                next,
                Source::default(),
                Kernel::with_monospace(),
                &carried,
                Default::default(),
                "/",
            )
            .unwrap();
            assert_eq!(
                reloaded.resource("answer"),
                Some(&Value::str(if reused { "stored answer" } else { "fresh" }))
            );
            assert_eq!(reloaded.data().queries, usize::from(!reused));
            assert_eq!(reloaded.resource_reads_store("answer"), reused);
        }
    }

    /// A reload that makes a kept reader `transient` forgets what it kept
    /// (LLP 1156.005 D4), though its source and logic are unchanged.
    #[test]
    fn a_reload_into_a_transient_resource_forgets_its_kept_answer() {
        let mut original_plan = plan("remember", false);
        original_plan.resources[0].reader = true;
        let mut original = Runner::boot_stored(
            original_plan,
            Source {
                deferred: true,
                ..Default::default()
            },
            Kernel::with_monospace(),
            vec![("token".into(), "stored answer".into())],
            Default::default(),
            "/",
        )
        .unwrap();
        original.data.deferred = false;
        original.data_ready().unwrap();
        let carried = original.carry();
        assert!(carried.store.iter().any(|(n, _)| n == "exact.kept.answer"));
        let mut next = plan("remember", true);
        next.resources[0].reader = true;
        next.resources[0].transient = true;
        let reloaded = Runner::boot_carrying(
            next,
            Source::default(),
            Kernel::with_monospace(),
            &carried,
            Default::default(),
            "/",
        )
        .unwrap();
        assert_eq!(reloaded.store.kept("exact.kept.answer"), None);
    }

    #[test]
    fn a_carried_kept_seed_cannot_restore_an_answer_from_another_source() {
        let mut original_plan = plan("remember", false);
        original_plan.resources[0].reader = true;
        // A module not ready at boot shows the resource's zero, and its answer
        // at `data_ready` is kept (LLP 1054.000.002 D1).
        let mut original = Runner::boot_stored(
            original_plan,
            Source {
                deferred: true,
                ..Default::default()
            },
            Kernel::with_monospace(),
            vec![("token".into(), "stored answer".into())],
            Default::default(),
            "/",
        )
        .unwrap();
        original.data.deferred = false;
        original.data_ready().unwrap();
        let mut carried = original.carry();
        assert!(carried
            .store
            .iter()
            .any(|(name, _)| name == "exact.kept.answer"));
        // Pending resources are omitted from direct carry, but their last kept
        // answer and source-qualified store dependency still survive.
        carried.resources.clear();
        for (source, reused) in [("remember", true), ("fresh", false)] {
            let mut next = plan(source, true);
            next.resources[0].reader = true;
            let mut reloaded = Runner::boot_carrying(
                next,
                Source {
                    deferred: true,
                    ..Default::default()
                },
                Kernel::with_monospace(),
                &carried,
                Default::default(),
                "/",
            )
            .unwrap();
            // Another source's kept answer is never restored: until its
            // module is ready, the resource shows its zero.
            assert_eq!(
                reloaded.resource("answer"),
                Some(&Value::str(if reused { "stored answer" } else { "" }))
            );
            assert_eq!(reloaded.data().queries, 0);
        }
    }

    #[test]
    fn repeated_reload_cannot_relabel_an_old_kept_seed_as_the_pending_source() {
        let mut old_plan = plan("remember", false);
        old_plan.resources[0].reader = true;
        let mut original = Runner::boot_stored(
            old_plan,
            Source {
                deferred: true,
                ..Default::default()
            },
            Kernel::with_monospace(),
            vec![("token".into(), "stored answer".into())],
            Default::default(),
            "/",
        )
        .unwrap();
        let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
        let string = b.primitive(TypeKind::String);
        b.resource("answer", "fresh", &[], string, Some(&Value::str("loading")));
        b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
        let mut next = b.finish().unwrap();
        next.resources[0].reader = true;
        original.data.deferred = false;
        original.data_ready().unwrap();
        let mut carried = original.carry();
        for reload in 0..2 {
            let mut reloaded = Runner::boot_carrying(
                next.clone(),
                Source {
                    deferred: true,
                    pending: true,
                    ..Default::default()
                },
                Kernel::with_monospace(),
                &carried,
                Default::default(),
                "/",
            )
            .unwrap();
            assert_eq!(reloaded.resource("answer"), Some(&Value::str("loading")));
            assert_eq!(reloaded.data().queries, 0);
            assert_eq!(reloaded.store().revision(), 0);
            assert_eq!(
                reloaded
                    .store()
                    .writes()
                    .iter()
                    .filter(|write| { write.name == "exact.kept.answer" && write.value.is_none() })
                    .count(),
                usize::from(reload == 0)
            );
            carried = reloaded.carry();
            assert!(carried.resources.is_empty());
            assert!(!carried
                .store
                .iter()
                .any(|(name, _)| name == "exact.kept.answer"));
            assert!(carried
                .store
                .iter()
                .any(|(name, value)| name == "token" && value == b"stored answer"));
        }
    }

    /// `events()` streams; each message is its answer, and the stream's end
    /// answers "ended".
    #[derive(Default)]
    struct Events {
        asks: usize,
    }
    impl DataSource for Events {
        fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
            Err(DataError::UnknownSource(source.into()))
        }
        fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
            self.asks += 1;
            Ok(Answer::stream(crate::Request::get(
                "https://example.test/events",
            )))
        }
        fn parse(
            &mut self,
            _: &mut Store,
            _: &str,
            _: &[Value],
            outcome: crate::Outcome,
        ) -> Result<Answer, DataError> {
            Ok(Answer::Now(Value::str(match &outcome {
                crate::Outcome::Message(m) => &m.data,
                _ => "ended",
            })))
        }
    }

    fn message(data: &str) -> crate::Outcome {
        crate::Outcome::Message(crate::Message {
            data: data.into(),
            ..crate::Message::default()
        })
    }

    fn reload(carried: &Carried) -> Runner<Events> {
        Runner::boot_carrying(
            plan("events", false),
            Events::default(),
            Kernel::with_monospace(),
            carried,
            Default::default(),
            "/",
        )
        .unwrap()
    }

    /// A stream open at a reload is opened anew, its last answer shown
    /// meanwhile; one still awaiting its first message is asked again as any
    /// request in flight is; one that ended is not.
    #[test]
    fn a_reload_opens_a_carried_stream_again() {
        let boot = || {
            Runner::boot(
                plan("events", false),
                Events::default(),
                Kernel::with_monospace(),
                Default::default(),
                "/",
            )
            .unwrap()
        };
        let mut open = boot();
        let ticket = open.take_requests().remove(0).ticket;
        open.fulfill(ticket, message("first")).unwrap();
        let carried = open.carry();
        assert_eq!(carried.streams, ["answer"]);
        let mut reloaded = reload(&carried);
        assert_eq!(reloaded.resource("answer"), Some(&Value::str("first")));
        let asked = reloaded.take_requests();
        assert_eq!(asked.len(), 1, "the stream is opened again");
        assert!(asked[0].request.stream);
        assert_eq!(reloaded.data().asks, 1);
        reloaded
            .fulfill(asked[0].ticket, message("second"))
            .unwrap();
        assert_eq!(reloaded.resource("answer"), Some(&Value::str("second")));
        assert_eq!(reloaded.carry().streams, ["answer"]);

        let mut waiting = boot();
        assert_eq!(waiting.take_requests().len(), 1);
        let carried = waiting.carry();
        assert!(carried.streams.is_empty() && carried.resources.is_empty());
        assert_eq!(reload(&carried).take_requests().len(), 1);

        let mut ended = boot();
        let ticket = ended.take_requests().remove(0).ticket;
        ended.fulfill(ticket, message("first")).unwrap();
        let end = crate::Outcome::Failed {
            kind: crate::FailureKind::Network,
            message: "the event stream ended".into(),
        };
        ended.fulfill(ticket, end).unwrap();
        let carried = ended.carry();
        assert!(carried.streams.is_empty());
        let mut reloaded = reload(&carried);
        assert_eq!(reloaded.resource("answer"), Some(&Value::str("ended")));
        assert!(reloaded.take_requests().is_empty());
        assert_eq!(reloaded.data().asks, 0);
    }
}
