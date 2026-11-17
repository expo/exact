use exact_runner::{FailureKind, Outcome};

#[test]
fn a_reply_that_lands_as_the_deadline_passes_stands() {
    use super::timed_out;
    let reply = Outcome::Response(exact_runner::Response {
        status: 201,
        headers: Vec::new(),
        body: Vec::new(),
    });
    assert!(matches!(timed_out(reply, true, 50), Outcome::Response(r) if r.status == 201));
    let aborted = Outcome::Failed {
        kind: FailureKind::Aborted,
        message: "native request aborted".into(),
    };
    assert!(matches!(
        timed_out(aborted.clone(), true, 50),
        Outcome::Failed { kind: FailureKind::Timeout, message } if message == "the request timed out after 50 ms"
    ));
    assert_eq!(timed_out(aborted.clone(), false, 50), aborted);
}
