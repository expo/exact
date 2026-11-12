use exact_kernel::{Kernel, NodeType};
use exact_plan::{asm::Asm, builder::PlanBuilder, MutationMode, TypeKind};
use exact_runner::{
    Answer, DataError, DataSource, HttpScheduling, Outcome, Request, RequestClass, RequestOut,
    Response, Runner, Store, Value,
};

struct Source {
    request: Request,
    next: Option<Request>,
}

impl DataSource for Source {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        unreachable!("the source hands back requests")
    }

    fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
        Ok(Answer::Later(self.request.clone()))
    }

    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[Value],
        _: Outcome,
    ) -> Result<Answer, DataError> {
        Ok(self
            .next
            .take()
            .map_or_else(|| Answer::Now(Value::str("done")), Answer::Later))
    }
}

fn boot(request: Request) -> Runner<Source> {
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let string = b.primitive(TypeKind::String);
    let optional = b.option(string);
    b.resource("read", "request", &[], string, None);
    let none = b.constant(&Value::NONE);
    let source = b.str("request");
    for (name, mode) in [
        ("ordinary", MutationMode::Newest),
        ("parallel", MutationMode::Parallel),
    ] {
        let slot = b.slot(name, optional, none);
        let mutation = b.mutation(name, slot, string);
        b.set_mutation_mode(mutation, mode);
        let mut body = Asm::new();
        body.send(mutation, source, 0);
        let body = b.code(body);
        b.action(name, &[], &[slot], body);
    }
    b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
    Runner::boot(
        b.finish().unwrap(),
        Source {
            request,
            next: None,
        },
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

fn one_request(r: &mut Runner<Source>) -> RequestOut {
    let mut requests = r.take_requests();
    assert_eq!(requests.len(), 1, "{requests:?}");
    requests.pop().unwrap()
}

#[test]
fn plain_http_requests_are_classified_by_their_target_for_every_method() {
    for method in [
        "GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS", "TRACE",
    ] {
        let request = Request {
            method: method.into(),
            ..Request::get("https://example.test/data")
        };
        let mut r = boot(request);
        assert_eq!(one_request(&mut r).class, RequestClass::Read, "{method}");
        r.act("ordinary", vec![]).unwrap();
        assert_eq!(one_request(&mut r).class, RequestClass::Ordered, "{method}");
        let mutation = r
            .plan()
            .mutations
            .iter()
            .position(|m| r.plan().str(m.name) == "parallel")
            .unwrap() as u32;
        assert_eq!(mutation, 1);
        for _ in 0..2 {
            r.act("parallel", vec![]).unwrap();
            assert_eq!(
                one_request(&mut r).class,
                RequestClass::Parallel(mutation),
                "{method}"
            );
        }
    }
}

#[test]
fn other_work_stays_ordered_for_resources_and_parallel_sends() {
    let get = || Request::get("https://example.test/data");
    for request in [
        get().body_to("app:/cache/reply"),
        Request::post_json("https://example.test/data", "").body_from("app:/data/body"),
        Request {
            stream: true,
            ..get()
        },
        Request::storage(b"{}".to_vec()),
        Request::continuation(1),
        Request::capture_surface("canvas"),
        Request {
            socket: true,
            ..get()
        },
        Request {
            listen: Some(Box::default()),
            ..get()
        },
        get().long_poll("cursor", "id"),
        get().upload_progress(),
        Request::native(vec![]),
        get().independent_http(4096),
    ] {
        let mut r = boot(request.clone());
        assert_eq!(
            one_request(&mut r).class,
            RequestClass::Ordered,
            "{request:?}"
        );
        r.act("parallel", vec![]).unwrap();
        assert_eq!(
            one_request(&mut r).class,
            RequestClass::Ordered,
            "{request:?}"
        );
    }
    for url in [exact_runner::PROCESS_URL, exact_runner::auth::AUTH_URL] {
        let request = Request {
            url: url.into(),
            ..get()
        };
        assert_eq!(request.http, HttpScheduling::Ordered);
        assert!(!request.is_plain_http(), "{url}");
    }
}

#[test]
fn each_request_after_a_reply_is_classified_again() {
    let mut r = boot(Request::get("https://example.test/data"));
    let first = one_request(&mut r);
    assert_eq!(first.class, RequestClass::Read);
    r.data().next = Some(Request::continuation(1));
    r.fulfill(first.ticket, response()).unwrap();
    let next = one_request(&mut r);
    assert_eq!(next.class, RequestClass::Ordered);
    r.data().next = Some(Request::post_json("https://example.test/detail", "{}"));
    r.fulfill(next.ticket, response()).unwrap();
    assert_eq!(one_request(&mut r).class, RequestClass::Read);
    r.act("parallel", vec![]).unwrap();
    let send = one_request(&mut r);
    assert_eq!(send.class, RequestClass::Parallel(1));
    r.data().next = Some(Request::continuation(2));
    r.fulfill(send.ticket, response()).unwrap();
    let next = one_request(&mut r);
    assert_eq!(next.class, RequestClass::Ordered);
    r.data().next = Some(Request::get("https://example.test/detail"));
    r.fulfill(next.ticket, response()).unwrap();
    assert_eq!(one_request(&mut r).class, RequestClass::Parallel(1));
}

fn response() -> Outcome {
    Outcome::Response(Response {
        status: 200,
        headers: vec![],
        body: vec![],
    })
}
