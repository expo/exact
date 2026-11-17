//! A request with a source's grant scope (`exact_data::Mixed` sets one on
//! each request) runs on the worker's own transport, so it reuses the
//! connections that transport keeps.
use super::tests::{collect, job};
use super::*;
use ibex2::{
    boundary::HostError,
    stdlib::{
        abort::AbortSignal,
        fetch::{Headers, StreamingResponse, Transport},
    },
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::channel;

/// One transport of many, which records itself as each request's carrier.
struct Counted {
    id: usize,
    served: Arc<Mutex<Vec<usize>>>,
}
impl Transport for Counted {
    fn open(
        &self,
        request: &ibex2::stdlib::fetch::Request,
        signal: &AbortSignal,
    ) -> Result<StreamingResponse, HostError> {
        self.served.lock().unwrap().push(self.id);
        Ok(ibex2::stdlib::fetch::Response {
            status: 200,
            status_text: "OK".into(),
            headers: Headers::new(),
            body: b"done".to_vec(),
            url: request.url.clone(),
            redirected: false,
        }
        .into_stream(request.body_limit(), signal.clone()))
    }
}

#[test]
fn a_scoped_request_rides_its_workers_transport() {
    let grants = "net.fetch https://example.test\nnet.fetch https://other.test";
    let made = Arc::new(AtomicUsize::new(0));
    let served = Arc::new(Mutex::new(Vec::new()));
    let host: StreamHost = {
        let (made, served) = (made.clone(), served.clone());
        Arc::new(move || {
            let id = made.fetch_add(1, Ordering::SeqCst);
            let served = served.clone();
            ibex2::host::Host::with_transport(Box::new(Counted { id, served }))
        })
    };
    let ordered = host().endow(ibex2::grant::GrantSet::parse(grants).unwrap());
    let (wake, woke) = channel();
    let core = Core::with_owners_on(
        owners(Some(ordered), grants),
        grants,
        Box::new(move || {
            let _ = wake.send(());
        }),
        host,
    );
    let scoped = |ticket: u64, independent: bool| {
        let mut request = Request::get("https://example.test/x");
        if independent {
            request = request.independent_http(4096);
        }
        request.grants = Some("net.fetch https://example.test".into());
        job(ticket, request)
    };
    for ticket in 1..=8 {
        core.run(scoped(ticket, false), None).unwrap();
        let outcome = collect(&core, &woke, 1).remove(0).1;
        assert!(
            matches!(outcome, Outcome::Response(ref r) if r.status == 200),
            "{outcome:?}"
        );
    }
    assert_eq!(
        *served.lock().unwrap(),
        [0; 8],
        "the ordered owner's transport"
    );
    for ticket in 9..=16 {
        core.run(scoped(ticket, true), None).unwrap();
        let outcome = collect(&core, &woke, 1).remove(0).1;
        assert!(
            matches!(outcome, Outcome::Response(ref r) if r.status == 200),
            "{outcome:?}"
        );
    }
    // The ordered owner's transport and one for each independent worker.
    assert!(made.load(Ordering::SeqCst) <= 1 + INDEPENDENT, "{made:?}");
    // A scope still narrows: an origin the app may reach and the scope may not.
    let mut outside = Request::get("https://other.test/x");
    outside.grants = Some("net.fetch https://example.test".into());
    core.run(job(17, outside), None).unwrap();
    let outcome = collect(&core, &woke, 1).remove(0).1;
    assert!(matches!(outcome, Outcome::Failed { .. }), "{outcome:?}");
    assert_eq!(served.lock().unwrap().len(), 16, "nothing was sent");
}

/// The boot endows the ordered owner with the data module's grants before
/// the plan is read; the core's grants add the origins the plan's declared
/// sources carry (LLP 1162.001 D3), and the ordered owner admits them too.
#[test]
fn the_ordered_owner_takes_the_apps_grants_with_its_declared_origins() {
    let module = "net.fetch https://example.test";
    let app = "net.fetch https://example.test\nnet.fetch https://declared.test";
    let served = Arc::new(Mutex::new(Vec::new()));
    let host: StreamHost = {
        let served = served.clone();
        Arc::new(move || {
            let served = served.clone();
            ibex2::host::Host::with_transport(Box::new(Counted { id: 0, served }))
        })
    };
    let ordered = host().endow(ibex2::grant::GrantSet::parse(module).unwrap());
    let (wake, woke) = channel();
    let core = Core::with_owners_on(
        owners(Some(ordered), app),
        app,
        Box::new(move || {
            let _ = wake.send(());
        }),
        host,
    );
    // Unscoped, as a data module's request is: the ordered lane's grants are
    // the independent lanes' (which the core endows from `app`).
    core.run(job(1, Request::get("https://declared.test/forecast")), None)
        .unwrap();
    let outcome = collect(&core, &woke, 1).remove(0).1;
    assert!(
        matches!(outcome, Outcome::Response(ref r) if r.status == 200),
        "{outcome:?}"
    );
    // An origin neither grants is refused before contact.
    core.run(job(2, Request::get("https://elsewhere.test/")), None)
        .unwrap();
    let outcome = collect(&core, &woke, 1).remove(0).1;
    assert!(matches!(outcome, Outcome::Failed { .. }), "{outcome:?}");
    assert_eq!(served.lock().unwrap().len(), 1, "one request was sent");
}
