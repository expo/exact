use super::*;

#[test]
fn filled_settings_are_checked_held_and_journalled() {
    const TEST: &str = "preload::tests::env::filled_settings_are_checked_held_and_journalled";
    let Ok(case) = std::env::var("EONE_EXECUTOR_CASE") else {
        for (case, value, header) in [
            ("unset", None, None),
            ("empty", Some(""), Some("")),
            ("launch", Some("https://launch.example"), Some("launch")),
            ("grant", Some("https://denied.example"), None),
            ("header", None, Some("invalid\nheader")),
            ("secret", Some("https://"), None),
        ] {
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args([TEST, "--exact", "--nocapture"])
                .env("EONE_EXECUTOR_CASE", case)
                .env_remove("EONE_EXECUTOR_URL")
                .env_remove("EONE_EXECUTOR_HEADER");
            if let Some(value) = value {
                command.env("EONE_EXECUTOR_URL", value);
            }
            if let Some(value) = header {
                command.env("EONE_EXECUTOR_HEADER", value);
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{case}: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        return;
    };
    use exact_runner::request::{Preload as Baked, PreloadPart::*, PreloadWhen};
    let mut baked = Baked {
        name: "feed",
        method: "POST",
        url: &[
            Env("EONE_EXECUTOR_URL", "https://built.example"),
            Literal("/feed"),
        ],
        headers: &[("X-Note", &[Env("EONE_EXECUTOR_HEADER", "built")])],
        body: Some(&[Env("EONE_EXECUTOR_HEADER", "built")]),
        when: PreloadWhen::Start,
        timeout_ms: None,
        secrets: &[],
    };
    if case == "secret" {
        baked.url = &[
            Env("EONE_EXECUTOR_URL", "https://built.example/"),
            Secret("session"),
        ];
        baked.secrets = &["session"];
    }
    let filled = Definition::fill(&baked, |_| Some("private-token".into()));
    if matches!(case.as_str(), "header" | "secret") {
        let fact = filled.err().unwrap();
        assert_eq!(fact.name, "feed");
        assert_eq!(
            fact.outcome,
            if case == "header" {
                Event::InvalidHeader
            } else {
                Event::InvalidUrl
            }
        );
        assert_eq!(
            fact.origin,
            if case == "header" {
                "https://built.example"
            } else {
                ""
            }
        );
        assert!(!format!("{fact:?}").contains("private-token"));
        return;
    }
    struct Recording(Sender<String>);
    impl ibex2::stdlib::fetch::Transport for Recording {
        fn open(
            &self,
            request: &ibex2::stdlib::fetch::Request,
            signal: &ibex2::stdlib::abort::AbortSignal,
        ) -> Result<ibex2::stdlib::fetch::StreamingResponse, HostError> {
            self.0.send(request.url.clone()).unwrap();
            Ok(ibex2::stdlib::fetch::Response {
                status: 200,
                status_text: String::new(),
                headers: ibex2::stdlib::fetch::Headers::new(),
                body: b"preloaded".to_vec(),
                url: request.url.clone(),
                redirected: false,
            }
            .into_stream(request.body_limit(), signal.clone()))
        }
    }
    let (sent, seen) = channel();
    let set = Preloads::start(
        vec![filled.unwrap()],
        "net.fetch https://built.example\nnet.fetch https://launch.example",
        move || ibex2::host::Host::with_transport(Box::new(Recording(sent.clone()))),
    );
    if case == "grant" {
        complete_set(&set);
        assert!(seen.try_recv().is_err());
        let facts = set.take_journal();
        assert_eq!(facts.len(), 1);
        assert_eq!(
            facts[0],
            Fact {
                name: "feed".into(),
                origin: "https://denied.example".into(),
                outcome: Event::RefusedGrant
            }
        );
        return;
    }
    set.control().wait_ready(1);
    let origin = if case == "launch" {
        "https://launch.example"
    } else {
        "https://built.example"
    };
    let value = if case == "launch" { "launch" } else { "built" };
    let mut request = Request::get(&format!("{origin}/feed"));
    request.method = "POST".into();
    request.headers = vec![("X-Note".into(), value.into())];
    request.body = value.as_bytes().to_vec();
    let (reply, timing) = set
        .control()
        .settle(&job(1, request))
        .expect("the equal read takes the preload");
    assert!(timing.cached);
    assert!(matches!(reply, Outcome::Response(r) if r.body == b"preloaded"));
    assert_eq!(seen.recv_timeout(HANG).unwrap(), format!("{origin}/feed"));
    assert!(seen.try_recv().is_err());
    assert!(set
        .take_journal()
        .iter()
        .any(|fact| fact.origin == origin && fact.outcome == Event::Taken));
}
