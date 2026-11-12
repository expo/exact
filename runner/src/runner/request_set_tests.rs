use super::*;
use exact_kernel::NodeType;
use exact_plan::{asm::Asm, builder::PlanBuilder, TypeKind};

#[derive(Default)]
enum Reply {
    #[default]
    Wait,
    Add(usize),
    Settle,
    Fail,
}

struct Sets {
    size: usize,
    extra: Vec<Request>,
    serial: u64,
    tagged: bool,
    post: bool,
    write: bool,
    refuse_guard: bool,
    reply: Reply,
    seen: Vec<(Option<u64>, Outcome)>,
    forgotten: Vec<Vec<(Target, Vec<Value>)>>,
}

impl Sets {
    fn new(size: usize) -> Self {
        Self {
            size,
            extra: Vec::new(),
            serial: 0,
            tagged: true,
            post: false,
            write: false,
            refuse_guard: false,
            reply: Reply::Wait,
            seen: Vec::new(),
            forgotten: Vec::new(),
        }
    }

    fn requests(&mut self, size: usize) -> Answer {
        let mut requests = (0..size).map(|i| {
            self.serial += 1;
            let url = format!("https://example.test/{i}");
            let mut request = if self.post {
                Request::post_json(&url, "{}")
            } else {
                Request::get(&url)
            };
            request.fetch = self.tagged.then_some(self.serial);
            request
        });
        let first = requests.next().unwrap();
        self.extra = requests.collect();
        Answer::Later(first)
    }
}

impl DataSource for Sets {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        unreachable!("the source hands over request sets")
    }

    fn answer(&mut self, _: &mut Store, source: &str, _: &[Value]) -> Result<Answer, DataError> {
        if source == "guard" {
            return if self.refuse_guard {
                Err(DataError::Unavailable("guard refused".into()))
            } else {
                Ok(Answer::Now(Value::Number(0.)))
            };
        }
        Ok(self.requests(self.size))
    }

    fn more_for(&mut self, _: Target) -> Vec<Request> {
        std::mem::take(&mut self.extra)
    }

    fn parse_fetch_for(
        &mut self,
        _: Target,
        fetch: Option<u64>,
        store: &mut Store,
        _: &str,
        _: &[Value],
        outcome: Outcome,
    ) -> Result<Option<Answer>, DataError> {
        self.seen.push((fetch, outcome));
        if self.write {
            store.get("token");
            store.set("token", &fetch.unwrap().to_string())?;
        }
        match std::mem::take(&mut self.reply) {
            Reply::Wait => Ok(None),
            Reply::Add(n) => Ok(Some(self.requests(n))),
            Reply::Settle => Ok(Some(Answer::Now(Value::Number(42.)))),
            Reply::Fail => Err(DataError::Unavailable("reply failed".into())),
        }
    }

    fn forgotten(&mut self, _: &Store, in_flight: &[InFlight<'_>]) {
        self.forgotten.push(
            in_flight
                .iter()
                .map(|p| (p.target, p.args.to_vec()))
                .collect(),
        );
    }

    fn grants(&self) -> &str {
        "secret.keep token"
    }
}

fn boot(source: Sets) -> Runner<Sets> {
    boot_with_guard(source, false)
}

fn boot_with_guard(source: Sets, guard: bool) -> Runner<Sets> {
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let number = b.primitive(TypeKind::Number);
    let zero = b.constant(&Value::Number(0.));
    let revision = b.slot("revision", number, zero);
    let mut arg = Asm::new();
    arg.load_slot(revision);
    let arg = b.code(arg);
    let rows = b.resource("rows", "rows", &[arg], number, None);
    if guard {
        b.resource("guard", "guard", &[arg], number, None);
    }
    let mut body = Asm::new();
    body.load_param(0).store_slot(revision);
    let body = b.code(body);
    b.action("change", &[("value", number)], &[revision], body);
    let mut body = Asm::new();
    body.refresh(rows);
    let body = b.code(body);
    b.action("refresh", &[], &[], body);
    let optional = b.option(number);
    let none = b.constant(&Value::NONE);
    let source_name = b.str("send");
    for (name, mode) in [
        ("ordinary", MutationMode::Newest),
        ("parallel", MutationMode::Parallel),
    ] {
        let slot = b.slot(name, optional, none);
        let mutation = b.mutation(name, slot, number);
        b.set_mutation_mode(mutation, mode);
        let mut body = Asm::new();
        body.send(mutation, source_name, 0);
        let body = b.code(body);
        b.action(name, &[], &[slot], body);
    }
    b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
    Runner::boot(
        b.finish().unwrap(),
        source,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

fn response(status: u16) -> Outcome {
    Outcome::Response(crate::Response {
        status,
        headers: vec![],
        body: vec![],
    })
}

#[test]
fn a_resource_hands_over_its_set_in_order_and_is_pending_once() {
    let mut r = boot(Sets::new(3));
    let requests = r.take_requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests.iter().map(|r| r.request.fetch).collect::<Vec<_>>(),
        [Some(1), Some(2), Some(3)]
    );
    for (i, request) in requests.iter().enumerate() {
        assert_eq!(request.class, RequestClass::Read);
        assert_eq!(request.request.url, format!("https://example.test/{i}"));
        assert!(r.holds(request.ticket));
    }
    assert!(requests.windows(2).all(|w| w[0].ticket < w[1].ticket));
    assert!(r.resources[0].pending);
    assert_eq!(r.outstanding().requests, ["rows"]);
    assert_eq!(r.pending().len(), 3);
    assert_eq!(r.in_flight().len(), 3);
    let state: serde_json::Value = serde_json::from_str(&crate::agent::state(&r)).unwrap();
    assert_eq!(state["pending"].as_array().unwrap().len(), 3);
    assert!(r.take_requests().is_empty());
}

#[test]
fn replies_route_by_fetch_and_empty_turns_commit_before_the_answer_settles() {
    let mut r = boot(Sets::new(3));
    r.data().write = true;
    let requests = r.take_requests();
    for (index, status) in [(2, 203), (0, 201)] {
        assert!(r
            .fulfill(requests[index].ticket, response(status))
            .unwrap()
            .is_some());
        assert_eq!(
            r.store().get("token"),
            Some(if index == 2 { "3" } else { "1" })
        );
        assert!(r.resource_reads_store("rows"));
        assert!(r.resources[0].pending);
        assert!(!r.holds(requests[index].ticket));
    }
    assert_eq!(
        r.data().seen,
        [(Some(3), response(203)), (Some(1), response(201))]
    );
    assert_eq!(r.pending().len(), 1);
    assert!(r.holds(requests[1].ticket));
    r.data().reply = Reply::Settle;
    r.fulfill(requests[1].ticket, response(202)).unwrap();
    assert_eq!(r.resource("rows"), Some(&Value::Number(42.)));
    assert!(!r.resources[0].pending);
}

#[test]
fn a_reply_adds_tickets_and_settlement_lets_the_remaining_request_go() {
    let mut r = boot(Sets::new(3));
    let first = r.take_requests();
    r.data().reply = Reply::Add(2);
    r.fulfill(first[1].ticket, response(200)).unwrap();
    let added = r.take_requests();
    assert_eq!(added.len(), 2);
    assert_eq!(
        added.iter().map(|r| r.request.fetch).collect::<Vec<_>>(),
        [Some(4), Some(5)]
    );
    assert!(added
        .iter()
        .all(|r| r.class == RequestClass::Read && r.ticket > first[2].ticket));
    assert_eq!(r.pending().len(), 4);
    r.fulfill(added[1].ticket, response(205)).unwrap();
    r.fulfill(first[0].ticket, response(201)).unwrap();
    r.data().reply = Reply::Settle;
    r.fulfill(added[0].ticket, response(204)).unwrap();
    assert!(!r.holds(first[2].ticket));
    assert!(!r.has_pending());
    assert!(r.pending().is_empty());
    assert!(!r.resources[0].pending);
    assert!(r.data().forgotten.last().unwrap().is_empty());
    assert!(r.fulfill(first[2].ticket, response(203)).unwrap().is_none());
    assert_eq!(r.data().seen.len(), 4);
}

#[test]
fn newer_arguments_and_refresh_let_go_of_the_whole_set() {
    let mut r = boot(Sets::new(3));
    let first = r.take_requests();
    r.fulfill(first[1].ticket, response(200)).unwrap();
    r.act("change", vec![Value::Number(1.)]).unwrap();
    let next = r.take_requests();
    assert_eq!(next.len(), 3);
    assert!(first.iter().all(|p| !r.holds(p.ticket)));
    assert_eq!(
        r.data().forgotten.last().unwrap(),
        &vec![(Target::Resource(0), vec![Value::Number(1.)]); 3]
    );
    r.act("refresh", vec![]).unwrap();
    let refreshed = r.take_requests();
    assert_eq!(refreshed.len(), 3);
    assert!(refreshed.iter().all(|p| p.forced));
    assert!(next.iter().all(|p| !r.holds(p.ticket)));
    assert_eq!(r.pending().len(), 3);
    assert_eq!(r.data().forgotten.last().unwrap().len(), 3);
}

#[test]
fn a_kept_request_requires_one_old_and_one_new_request_and_no_force() {
    for (old_size, new_size, kept) in [(1, 1, true), (1, 2, false), (2, 1, false), (2, 2, false)] {
        let mut source = Sets::new(old_size);
        source.tagged = false;
        let mut r = boot(source);
        let first = r.take_requests();
        r.data().size = new_size;
        r.act("change", vec![Value::Number(1.)]).unwrap();
        let next = r.take_requests();
        assert_eq!(next.len(), if kept { 0 } else { new_size });
        assert!(first.iter().all(|p| r.holds(p.ticket) == kept));
        assert_eq!(r.pending().len(), new_size);
        r.data().reply = Reply::Settle;
        let ticket = if kept {
            first[0].ticket
        } else {
            next[0].ticket
        };
        r.fulfill(ticket, response(200)).unwrap();
        assert_eq!(
            r.resources[0].state.as_ref().unwrap().args,
            [Value::Number(1.)]
        );
    }
    let mut source = Sets::new(1);
    source.tagged = false;
    let mut r = boot(source);
    let first = r.take_requests();
    r.act("refresh", vec![]).unwrap();
    let refreshed = r.take_requests();
    assert_eq!(refreshed.len(), 1);
    assert!(refreshed[0].forced);
    assert!(!r.holds(first[0].ticket));
}

#[test]
fn mutations_refuse_sets_on_the_first_turn_and_on_a_reply() {
    for action in ["ordinary", "parallel"] {
        let mut r = boot(Sets::new(1));
        r.take_requests();
        r.data().size = 2;
        assert!(
            matches!(r.act(action, vec![]), Err(RunnerError::Data { error: DataError::Unavailable(why), .. })
            if why.contains("one request at a time"))
        );
        assert!(r.take_requests().is_empty());
        r.data().size = 1;
        r.act(action, vec![]).unwrap();
        let sent = r.take_requests();
        assert_eq!(sent.len(), 1);
        r.data().reply = Reply::Add(2);
        r.fulfill(sent[0].ticket, response(200)).unwrap();
        assert!(!r.holds(sent[0].ticket));
        assert!(r.take_requests().is_empty());
        assert!(r
            .failed_mut
            .iter()
            .flatten()
            .any(|failure| failure.message.contains("one request at a time")));
    }
}

#[test]
fn a_failed_reply_releases_the_set_and_keeps_effect_turn_writes() {
    let mut source = Sets::new(3);
    source.post = true;
    source.write = true;
    let mut r = boot(source);
    let first = r.take_requests();
    r.data().reply = Reply::Fail;
    r.fulfill(first[1].ticket, response(200)).unwrap();
    assert_eq!(r.store().get("token"), Some("2"));
    assert!(r.resource_reads_store("rows"));
    assert!(first.iter().all(|p| !r.holds(p.ticket)));
    assert!(!r.resources[0].pending);
    assert_eq!(r.failed_resources(), [("rows", "reply failed")]);
    assert!(r.data().forgotten.last().unwrap().is_empty());
}

#[test]
fn a_refused_reask_restores_every_ticket_of_the_previous_set() {
    let mut r = boot_with_guard(Sets::new(3), true);
    let first = r.take_requests();
    r.data().refuse_guard = true;
    assert!(r.act("change", vec![Value::Number(1.)]).is_err());
    assert!(r.take_requests().is_empty());
    assert!(first.iter().all(|p| r.holds(p.ticket)));
    assert_eq!(r.pending().len(), 3);
    assert_eq!(
        r.resources
            .iter()
            .map(|row| row.pending)
            .collect::<Vec<_>>(),
        [true, false]
    );
    assert_eq!(
        r.data().forgotten.last().unwrap(),
        &vec![(Target::Resource(0), vec![Value::Number(0.)]); 3]
    );
}

#[test]
fn family_request_sets_keep_distinct_targets_and_settle_their_runtime_rows() {
    let plan = contract::compile(
        r#"component App
  resource rows = rows(n) for n in [1, 2] as shape number
  view
    text "Rows"
"#,
    )
    .unwrap();
    let mut r = Runner::boot(
        plan,
        Sets::new(2),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let requests = r.take_requests();
    assert_eq!(requests.len(), 4);
    assert_eq!(r.outstanding().requests, ["rows", "rows"]);
    for request in [&requests[1], &requests[3]] {
        r.data().reply = Reply::Settle;
        r.fulfill(request.ticket, response(200)).unwrap();
    }
    assert!(!r.has_pending());
    assert!(requests.iter().all(|request| !r.holds(request.ticket)));
    assert_eq!(
        r.resource("rows"),
        Some(&Value::list(vec![Value::Number(42.); 2]))
    );
}

#[test]
fn inspection_started_with_a_set_in_flight_tracks_every_ticket() {
    let mut r = boot(Sets::new(3));
    let requests = r.take_requests();
    r.measure(true, false);
    let generation = r.resources[0].inspection.request;
    assert_ne!(generation, 0);
    assert!(r
        .pending
        .iter()
        .all(|p| p.inspection_generation == generation));
    for request in &requests[..2] {
        r.fulfill(request.ticket, response(200)).unwrap();
        assert!(r.resources[0].pending);
    }
    r.data().reply = Reply::Settle;
    r.fulfill(requests[2].ticket, response(200)).unwrap();
    assert!(!r.resources[0].pending);
    assert_eq!(r.resources[0].inspection.successful, generation);
    assert_eq!(
        r.resources[0].state.as_ref().unwrap().inspection_generation,
        generation
    );
}
