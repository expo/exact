//! Development-only resource provenance (LLP 1142 D9). These identifiers
//! describe requests; settlement never consults them to decide what to show.
use super::*;
use crate::agent::{num, quote};
use crate::perf::ResourceWork;
use std::fmt::Write as _;

impl<D: DataSource> Runner<D> {
    /// Same immutable plan as a targeted node read. No work until an inspector
    /// requests identity; a replacement runner owns a fresh cache.
    pub fn inspection_digest(&self) -> &str {
        self.inspection_digest.get_or_init(|| {
            use sha2::{Digest, Sha256};
            let mut out = String::with_capacity(64);
            for byte in Sha256::digest(self.plan.encode()) {
                write!(out, "{byte:02x}").unwrap();
            }
            out
        })
    }

    fn inspection_id(&mut self) -> u64 {
        self.ids.work.resource_serial += 1;
        self.ids.work.resource_serial
    }

    pub(super) fn begin_resource_inspection(&mut self) {
        if self.ids.work.sites.is_empty() || self.ids.work.resource_inspection {
            return;
        }
        self.ids.work.resources_since = self.now_ms;
        self.ids.work.resource_inspection = true;
        for row in &mut self.resources {
            row.inspection = ResourceWork::default();
        }
        // Existing live answers and tickets have provenance even when observation
        // starts after boot. Seeds have no request in this runner and stay unknown.
        for i in 0..self.resources.len() {
            if self.resources[i]
                .state
                .as_ref()
                .is_some_and(|s| s.origin > 0 && !s.placeholder)
            {
                let generation = self.inspection_id();
                self.resources[i]
                    .state
                    .as_mut()
                    .unwrap()
                    .inspection_generation = generation;
                self.resources[i].inspection = ResourceWork {
                    request: generation,
                    successful: generation,
                };
            }
            if let Some(at) = self
                .pending
                .iter()
                .position(|p| p.target == Target::Resource(i))
            {
                let same = self.resources[i]
                    .state
                    .as_ref()
                    .is_some_and(|s| s.origin > 0 && s.origin == self.pending[at].origin);
                let generation = if same {
                    self.resources[i]
                        .state
                        .as_ref()
                        .unwrap()
                        .inspection_generation
                } else {
                    self.inspection_id()
                };
                for p in self
                    .pending
                    .iter_mut()
                    .filter(|p| p.target == Target::Resource(i))
                {
                    p.inspection_generation = generation;
                }
                self.resources[i].inspection.request = generation;
            } else if self.resources[i].failed_args.is_some() {
                self.resources[i].inspection.request = self.inspection_id();
            }
        }
    }

    pub(super) fn inspect_request(&mut self, i: usize) {
        if self.ids.work.resource_inspection {
            self.resources[i].inspection.request = self.inspection_id();
        }
    }

    pub(super) fn inspected_request(&self, i: usize) -> u64 {
        self.resources
            .get(i)
            .map(|row| &row.inspection)
            .map_or(0, |w| w.request)
    }

    pub(super) fn inspected_success(&mut self, i: usize, generation: u64) {
        if generation > 0 {
            if let Some(w) = self.resources.get_mut(i).map(|row| &mut row.inspection) {
                w.successful = generation;
            }
        }
    }

    pub(crate) fn resource_inspection(&self, out: &mut String) {
        if !self.ids.work.resource_inspection {
            return;
        }
        out.push_str(",\"planDigest\":");
        quote(self.inspection_digest(), out);
        let generation = |n: u64| if n == 0 { "null".into() } else { n.to_string() };
        let _ = write!(
            out,
            ",\"resourcesMeasuredFrom\":{},\"resourceStatus\":{{",
            num(self.ids.work.resources_since)
        );
        for (i, w) in self
            .resources
            .iter()
            .take(self.plan.resources.len())
            .map(|row| &row.inspection)
            .enumerate()
        {
            if i > 0 {
                out.push(',');
            }
            let row = self.resource_row(i);
            quote(self.plan.str(row.name), out);
            let _ = write!(out, ":{{\"resource\":{i},\"source\":");
            quote(self.plan.str(row.source), out);
            let state = self.resources[i].state.as_ref();
            let displayed = state.map_or(0, |s| s.inspection_generation);
            let fallback =
                state.is_none_or(|s| s.placeholder || s.origin == 0 && s.value.is_compiled());
            let status = if fallback {
                "fallback"
            } else if displayed > 0 && displayed == w.request {
                "current"
            } else {
                "retained"
            };
            let equal = state
                .filter(|_| !fallback)
                .and_then(|s| {
                    self.settled
                        .as_ref()
                        .and_then(|settled| settled.resource_args.get(i))
                        .and_then(|args| {
                            (s.answered_args().len() == args.len())
                                .then(|| s.answered_args() == args)
                        })
                })
                .map_or("null", |equal| if equal { "true" } else { "false" });
            let _ = write!(out, ",\"requestGeneration\":{},\"lastSuccessfulGeneration\":{},\"displayedGeneration\":{},\"pending\":{},\"displayed\":\"{status}\",\"argumentsEqual\":{equal},\"placeholder\":{},\"keptSeed\":{},\"keptSeedSupported\":true}}",
                generation(w.request), generation(w.successful), generation(displayed), self.resources[i].pending, state.is_some_and(|s| s.placeholder), state.is_some_and(|s| s.kept_seed));
        }
        out.push('}');
    }
}
