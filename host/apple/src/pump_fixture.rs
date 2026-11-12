use exact_runner::{Answer, DataError, DataSource, Dispatch, Outcome, Request, Store, Value};
use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

pub struct Replies {
    pub clock: Rc<Cell<Instant>>,
    pub advance_after: Option<usize>,
    pub refuse: Option<usize>,
    pub parsed: Vec<usize>,
    pub released_at: Vec<usize>,
}

impl Replies {
    pub fn new() -> Self {
        Self {
            clock: Rc::new(Cell::new(Instant::now())),
            advance_after: None,
            refuse: None,
            parsed: vec![],
            released_at: vec![],
        }
    }
}

fn source(n: usize) -> String {
    let mut source = String::from("component App\n");
    for i in 0..n {
        source.push_str(&format!("  resource r{i} = reply({i}) as shape number\n"));
    }
    source.push_str("  view\n    column\n");
    for i in 0..n {
        source.push_str(&format!("      text r{i}.toString() testId=\"r{i}\"\n"));
    }
    source
}

pub fn plan(n: usize) -> Vec<u8> {
    contract::compile(&source(n)).unwrap().encode()
}

pub fn fault_plan() -> Vec<u8> {
    let source = source(4)
        .replace(
            "  view",
            "  derive unstable = pending(r2) ? 1 : 1 / r3\n  view",
        )
        .replace(
            "    column\n",
            "    column\n      text unstable.toString()\n",
        );
    contract::compile(&source).unwrap().encode()
}

impl DataSource for Replies {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        Ok(Value::Number(0.))
    }
    fn answer(&mut self, _: &mut Store, _: &str, args: &[Value]) -> Result<Answer, DataError> {
        let index = args[0].as_number().unwrap() as usize;
        let mut request = Request::continuation(index as u64 + 1);
        if self.refuse == Some(index) {
            request.timeout_ms = Some(1);
        }
        Ok(Answer::Later(request))
    }
    fn dispatch(&mut self, token: u64, _: &Store) -> Dispatch {
        if self.refuse == Some(token as usize - 1) {
            Dispatch::Missing
        } else {
            Dispatch::Again
        }
    }
    fn release(&mut self, _: &Store) -> Vec<(u64, Dispatch)> {
        self.released_at.push(self.parsed.len());
        vec![]
    }
    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        args: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        let index = args[0].as_number().unwrap() as usize;
        if self.refuse == Some(index) {
            assert!(matches!(outcome, Outcome::Failed { .. }));
        }
        self.parsed.push(index);
        if self.advance_after == Some(self.parsed.len()) {
            self.clock.set(self.clock.get() + Duration::from_millis(5));
        }
        Ok(Answer::Now(Value::Number(
            10. + args[0].as_number().unwrap(),
        )))
    }
}
