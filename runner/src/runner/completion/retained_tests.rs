use super::*;

fn runner(source: Source, initial: f64) -> Runner<Source> {
    Runner::boot(
        plan_at(initial),
        source,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

fn durable(outcome: Outcome) -> Completion {
    Completion::DurableOperation {
        outcome,
        timing: None,
    }
}

fn assert_durable(cap: &CompletionCapability, owner: Owner) {
    let b = cap.binding.0.lock().unwrap();
    assert_eq!(b.owner, owner);
    assert_eq!(b.class, Class::DurableProcessLocal);
    assert!(matches!(b.protocol, Protocol::DurableOperation));
}

#[test]
fn retained_work_transfers_the_issued_binding_even_after_host_extraction() {
    for take_first in [false, true] {
        let mut r = runner(
            Source {
                retained: true,
                ..Default::default()
            },
            1.,
        );
        let out = request(&mut r);
        let slot = out.completion.as_ref().unwrap();
        let early = take_first.then(|| capability(&out));
        r.act("wave", vec![Value::Number(0.)]).unwrap();
        assert!(r.is_module_work(out.ticket));
        assert!(r.pending.is_empty());
        let record = r.request_completion(out.ticket).unwrap();
        assert!(Arc::ptr_eq(&slot.0, &record.0));
        let cap = early.unwrap_or_else(|| capability(&out));
        assert_durable(&cap, Owner::ModuleWork);
        assert!(record.take().is_none());
        r.deliver(cap, durable(response("retained")))
            .unwrap()
            .unwrap();
        assert_eq!(r.data().landed, [(7, response("retained"))]);
        assert_eq!(r.data().parses, 0);
        r.fulfill(out.ticket, response("duplicate")).unwrap();
        assert_eq!(r.data().landed.len(), 1);
    }
}

#[test]
fn timer_adoption_keeps_the_original_binding_and_consumes_integer_delivery() {
    let mut r = runner(
        Source {
            race: true,
            ..Default::default()
        },
        1.,
    );
    let out = request(&mut r);
    let slot = out.completion.as_ref().unwrap();
    r.advance(10.).unwrap();
    let cap = slot.0.lock().unwrap();
    assert_eq!(cap.owner, Owner::ModuleWork);
    assert_eq!(cap.class, Class::DurableProcessLocal);
    drop(cap);
    let next = request(&mut r);
    assert_ne!(next.ticket, out.ticket);
    assert!(matches!(metadata(&capability(&next)).2, Owner::Ui(_)));
    r.fulfill(out.ticket, response("late")).unwrap();
    assert!(
        slot.take().is_none(),
        "integer delivery spends the transferred authority"
    );
    assert_eq!(r.data().landed, [(7, response("late"))]);
    assert!(r.holds(next.ticket));
}

#[test]
fn reports_and_their_local_continuations_get_fresh_durable_authority() {
    let mut r = runner(Source::default(), 0.);
    assert!(r.completion_identity.get().is_none());
    r.data().reports.push(crate::Report {
        token: 17,
        request: Request::get("https://example.test/report"),
        by: crate::Reporter::None,
    });
    r.take_reports();
    let ticket = r.requests[0].ticket;
    assert!(
        r.request_completion(ticket).is_none(),
        "only handoff issues authority"
    );
    let out = request(&mut r);
    let cap = capability(&out);
    assert_durable(&cap, Owner::ModuleWork);
    assert!(out.clone().completion.unwrap().take().is_none());
    r.data().module_yields = true;
    r.deliver(cap, durable(response("report"))).unwrap();
    let next = request(&mut r);
    assert_ne!(next.ticket, out.ticket);
    assert_eq!(next.request.continuation, Some(55));
    let next_slot = next.completion.as_ref().unwrap();
    assert!(!Arc::ptr_eq(&out.completion.unwrap().0, &next_slot.0));
    let cap = capability(&next);
    assert_durable(&cap, Owner::ModuleWork);
    r.deliver(cap, durable(response("continued"))).unwrap();
    assert!(next_slot.take().is_none());
    assert_eq!(
        r.data().landed,
        [(17, response("report")), (17, response("continued"))]
    );
    assert!(!r.has_pending());
}

#[test]
fn background_rounds_have_process_local_authority_independent_of_ui_retirement() {
    let mut r = runner(Source::default(), 1.);
    assert_eq!(r.pending.len(), 1, "the fixture must start with UI work");
    assert!(r.completion_identity.get().is_none());
    let ui = request(&mut r);
    assert!(r.holds(ui.ticket));
    r.data().rounds = 2;
    r.arm_background();
    let ticket = r.background_ticket().unwrap();
    assert!(r.request_completion(ticket).is_none());
    let out = request(&mut r);
    assert_ne!(out.ticket, ui.ticket);
    let cap = capability(&out);
    assert_durable(&cap, Owner::Background);
    assert!(out.clone().completion.unwrap().take().is_none());
    r.act("wave", vec![Value::Number(0.)]).unwrap();
    assert!(!r.holds(ui.ticket));
    assert!(r.pending.is_empty());
    assert!(r.holds(out.ticket));
    let batch = r.batch;
    assert!(r
        .deliver(cap, durable(response("first")))
        .unwrap()
        .is_none());
    assert_eq!(r.batch, batch);
    let next = request(&mut r);
    assert_ne!(next.ticket, out.ticket);
    r.fulfill(next.ticket, response("last")).unwrap();
    assert!(next.completion.unwrap().take().is_none());
    r.fulfill(out.ticket, response("duplicate")).unwrap();
    assert_eq!(
        r.data().landed,
        [
            (crate::BACKGROUND, response("first")),
            (crate::BACKGROUND, response("last"))
        ]
    );
    assert!(!r.has_pending());
}
