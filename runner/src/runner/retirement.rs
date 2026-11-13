//! A module generation fails all its outstanding work, including work restored by rollback.
use super::{CommitReceipt, DataSource, Runner, RunnerError};
use std::sync::{Arc, Mutex, OnceLock};

/// One module's identity, shared with its worker proxy.
#[derive(Clone, Default)]
pub struct ModuleIdentity(Arc<Mutex<ModuleInstance>>);

/// One generation of a module. Outstanding work retains it across retirement.
#[derive(Clone, Default, Debug)]
pub struct ModuleInstance(Arc<OnceLock<String>>);

impl ModuleIdentity {
    /// The generation a new call belongs to.
    pub fn instance(&self) -> ModuleInstance {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Fail this generation before dropping its runtime. Later calls belong to a fresh one.
    pub fn retire(&self, message: String) {
        let mut instance = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let _ = instance.0.set(message);
        *instance = ModuleInstance::default();
    }
}

impl PartialEq for ModuleInstance {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for ModuleInstance {}

impl ModuleInstance {
    /// Why this generation can no longer execute.
    pub fn failure(&self) -> Option<&str> {
        self.0.get().map(String::as_str)
    }
}

impl<D: DataSource> Runner<D> {
    pub(super) fn match_failed_requests(&mut self) {
        let mut failed: Vec<_> = self
            .pending
            .iter()
            .filter_map(|p| Some((p.ticket, p.module.as_ref()?.failure()?.to_owned())))
            .collect();
        failed.extend(self.failed_module_work());
        failed.extend(self.failed_tool_tickets());
        self.discard_failed_tools();
        failed.extend(self.failed_background());
        for (ticket, message) in failed {
            if let Some(token) = self
                .pending
                .iter()
                .find(|p| p.ticket == ticket)
                .and_then(|p| p.continuation)
            {
                self.data.discard(token);
            }
            self.fail_mapper_turn(ticket, &message);
            self.failed_tickets.insert(ticket, message);
            if !self.module_aborts.contains(&ticket) {
                self.module_aborts.push(ticket);
            }
        }
    }

    pub(super) fn executor_failed(
        &mut self,
        ticket: u64,
        message: String,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        self.explicit_aborts.push(ticket);
        if let Some(p) = self.pending.iter().find(|p| p.ticket == ticket) {
            return self.release_failed_as_asked(
                ticket,
                p.target,
                crate::failure::Failure::error(message),
            );
        }
        if self.is_background(ticket) {
            self.background = Default::default();
        }
        self.fail_tool(ticket, &message);
        self.drop_module_work(ticket);
        self.log(format!("request {ticket}: executor retired: {message}"));
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Answer, DataError, Outcome, Request, Store, Value};

    struct Source([ModuleIdentity; 2]);
    impl DataSource for Source {
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
            unreachable!()
        }
        fn module_identity(&self, source: &str) -> Option<ModuleIdentity> {
            Some(self.0[usize::from(source == "healthy")].clone())
        }
        fn answer(
            &mut self,
            _: &mut Store,
            source: &str,
            _: &[Value],
        ) -> Result<Answer, DataError> {
            let mut request = Request::get("https://test.invalid/");
            request.fetch = Some(1);
            if source == "racing" {
                request.module = Some(self.0[0].instance());
                self.0[0].retire("retired before enqueue".into());
            }
            Ok(Answer::Later(request))
        }
        fn parse(
            &mut self,
            _: &mut Store,
            _: &str,
            _: &[Value],
            _: Outcome,
        ) -> Result<Answer, DataError> {
            Ok(Answer::Now(Value::str("done")))
        }
    }

    #[test]
    fn retirement_between_producing_and_enqueuing_a_request_keeps_its_generation() {
        let plan = contract::compile("component App\n  resource result = racing() as shape string\n  view\n    text result\n").unwrap();
        let mut r = Runner::boot(
            plan,
            Source(Default::default()),
            exact_kernel::Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        assert!(
            r.take_requests().is_empty(),
            "retired work must never go out"
        );
        let (ticket, outcome) = r.take_aborted_answer().unwrap();
        assert!(outcome.summary().contains("retired before enqueue"));
        r.fulfill(ticket, outcome).unwrap();
        assert!(r.pending().is_empty());
    }

    #[test]
    fn retirement_is_scoped_to_the_module_generation_and_success_releases_ownership() {
        let identities = [ModuleIdentity::default(), ModuleIdentity::default()];
        let plan = contract::compile("component App\n  resource broken = broken() as shape string\n  resource healthy = healthy() as shape string\n  action again\n    refresh broken\n  view\n    text broken\n").unwrap();
        let mut r = Runner::boot(
            plan,
            Source(identities.clone()),
            exact_kernel::Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        let requests = r.take_requests();
        let broken = requests
            .iter()
            .find(|r| r.target == "broken")
            .unwrap()
            .ticket;
        let healthy = requests
            .iter()
            .find(|r| r.target == "healthy")
            .unwrap()
            .ticket;
        identities[0].retire("heap exhausted".into());
        r.collect_aborts();
        let (ticket, outcome) = r.take_aborted_answer().unwrap();
        assert_eq!(ticket, broken);
        r.fulfill(ticket, outcome).unwrap();
        assert!(r.take_aborted_answer().is_none());
        assert!(
            r.holds(healthy),
            "a matching fetch id in another module stays live"
        );
        let instance = identities[0].instance();
        for _ in 0..32 {
            r.act("again", vec![]).unwrap();
            let request = r.take_requests().pop().unwrap();
            r.fulfill(broken, crate::Dispatch::again_outcome()).unwrap();
            assert!(
                r.holds(request.ticket),
                "an old reply cannot settle the replacement"
            );
            r.fulfill(request.ticket, crate::Dispatch::again_outcome())
                .unwrap();
            assert_eq!(
                Arc::strong_count(&instance.0),
                2,
                "a successful call leaves no ownership record"
            );
            assert!(r.failed_tickets.is_empty());
        }
        assert!(instance.failure().is_none());
    }
}
