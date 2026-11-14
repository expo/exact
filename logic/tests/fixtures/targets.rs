use exact_runner::{
    Answer, CacheMode, DataError, DataSource, FailureKind, Outcome, Request, Store, Target, Value,
};
use std::collections::HashMap;

#[derive(Default)]
pub struct Targets {
    next: usize,
    pending: HashMap<Target, (usize, bool)>,
}

impl DataSource for Targets {
    fn app_id(&self) -> &str {
        "test.targets"
    }

    fn grants(&self) -> &str {
        "net.fetch local-network development"
    }

    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        Ok(Value::str("bake placeholder"))
    }

    fn answer_for(
        &mut self,
        target: Target,
        _: &mut Store,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, DataError> {
        self.next += 1;
        self.pending.insert(target, (self.next, false));
        let request = Request::get(args[0].as_str().unwrap()).cache(CacheMode::NoStore);
        Ok(Answer::Later(if source == "prefetch" {
            request.prefetch()
        } else {
            request
        }))
    }

    fn parse_for(
        &mut self,
        target: Target,
        _: &mut Store,
        source: &str,
        args: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        let Some((call, continued)) = self.pending.get_mut(&target) else {
            return Err(DataError::Unavailable("target has no pending call".into()));
        };
        if source == "prefetch" {
            let Outcome::Failed {
                kind: FailureKind::Refused,
                message,
            } = outcome
            else {
                return Err(DataError::Unavailable("expected prefetch refusal".into()));
            };
            let value = Value::str(&format!("{}:{call}:{message}", target.text()));
            self.pending.remove(&target);
            return Ok(Answer::Now(value));
        }
        if !*continued {
            *continued = true;
            return Ok(Answer::Later(
                Request::get(args[0].as_str().unwrap()).cache(CacheMode::NoStore),
            ));
        }
        let Outcome::Response(response) = outcome else {
            return Err(DataError::Unavailable("expected HTTP reply".into()));
        };
        let value = Value::str(&format!(
            "{}:{call}:{}",
            target.text(),
            String::from_utf8_lossy(&response.body)
        ));
        self.pending.remove(&target);
        Ok(Answer::Now(value))
    }
}

#[cfg(not(test))]
exact_logic_abi::export!(Targets, Targets::default());
