use super::*;
use exact_runner::request::{Preload, PreloadFillError, PreloadPart, PreloadWhen};

#[test]
fn filled_settings_are_checked_held_and_journalled() {
    const TEST: &str = "preload::tests::env::filled_settings_are_checked_held_and_journalled";
    let Ok(case) = std::env::var("EONE_EXECUTOR_CASE") else {
        let mut failures = Vec::new();
        for (case, value, header) in [
            ("unset", None, None),
            ("empty", Some(""), Some("")),
            ("launch", Some("https://launch.example"), Some("launch")),
            ("grant", Some("https://denied.example"), None),
            ("header", None, Some("invalid\nheader")),
            ("secret", Some("https://"), None),
            ("blob", Some("blob:https://"), None),
            ("javascript", Some("javascript:"), None),
            ("data", Some("data:"), None),
            ("relative", Some("//host"), None),
            ("backslash", Some(r"https:\\host"), None),
            ("userinfo", Some("https://user@built.example/"), None),
            ("path", Some("https://built.example/"), None),
            ("query", Some("https://built.example/?token="), None),
            ("newline", None, Some("Bearer abc\n")),
            ("space", None, Some("Bearer abc ")),
            ("http-space", None, Some(" \t\r\nBearer abc\n\r\t ")),
            ("cr", None, Some("invalid\rheader")),
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
            if !output.status.success() {
                failures.push(format!(
                    "{case}: {}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
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
    let url_secret = matches!(
        case.as_str(),
        "secret"
            | "blob"
            | "javascript"
            | "data"
            | "relative"
            | "backslash"
            | "userinfo"
            | "path"
            | "query"
    );
    if url_secret {
        baked.url = &[
            Env("EONE_EXECUTOR_URL", "https://built.example/"),
            Secret("session"),
        ];
        baked.secrets = &["session"];
    }
    let filled = Definition::fill(&baked, |_| Some("private-token".into()));
    if matches!(case.as_str(), "header" | "cr")
        || (url_secret && !matches!(case.as_str(), "path" | "query"))
    {
        let fact = filled.err().unwrap();
        assert_eq!(fact.name, "feed");
        assert_eq!(
            fact.outcome,
            if matches!(case.as_str(), "header" | "cr") {
                Event::InvalidHeader
            } else {
                Event::InvalidUrl
            }
        );
        assert_eq!(
            fact.origin,
            if matches!(case.as_str(), "header" | "cr") {
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
            if let Some(header) = std::env::var("EONE_EXECUTOR_HEADER")
                .ok()
                .filter(|s| !s.is_empty())
            {
                let expected = ibex2::stdlib::fetch::normalize_value(&header);
                assert_eq!(request.headers.get("X-Note"), Some(expected));
            }
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
    let value = match case.as_str() {
        "launch" => "launch",
        "newline" | "space" | "http-space" => "Bearer abc",
        _ => "built",
    };
    let target = match case.as_str() {
        "path" => format!("{origin}/private-token"),
        "query" => format!("{origin}/?token=private-token"),
        _ => format!("{origin}/feed"),
    };
    let mut request = Request::get(&target);
    request.reach.origin = exact_runner::CallOrigin::App;
    request.method = "POST".into();
    request.headers = vec![("X-Note".into(), value.into())];
    request.body = std::env::var("EONE_EXECUTOR_HEADER")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "built".into())
        .into_bytes();
    let (reply, timing) = set
        .control()
        .settle(&job(1, request))
        .expect("the equal read takes the preload");
    assert!(timing.cached);
    assert!(matches!(reply, Outcome::Response(r) if r.body == b"preloaded"));
    assert_eq!(seen.recv_timeout(HANG).unwrap(), target);
    assert!(seen.try_recv().is_err());
    let facts = set.take_journal();
    assert!(!format!("{facts:?}").contains("private-token"));
    assert!(facts
        .iter()
        .any(|fact| fact.origin == origin && fact.outcome == Event::Taken));
}

const DEFINITION: Preload = Preload {
    name: "feed",
    method: "POST",
    url: &[
        PreloadPart::Literal("https://api.example/"),
        PreloadPart::Secret("session"),
    ],
    headers: &[(
        "Authorization",
        &[
            PreloadPart::Literal("Bearer "),
            PreloadPart::Secret("session"),
        ],
    )],
    body: Some(&[
        PreloadPart::Secret("session"),
        PreloadPart::Literal("{"),
        PreloadPart::Secret("other"),
    ]),
    timeout_ms: Some(42),
    when: PreloadWhen::Foreground,
    secrets: &["other", "session"],
};

#[test]
fn filling_replaces_every_reference_from_one_read_per_secret() {
    let mut reads = Vec::new();
    let request = DEFINITION
        .fill(
            |name| {
                reads.push(name.to_owned());
                Some(format!("value-{name}"))
            },
            preload_url,
        )
        .unwrap();
    assert_eq!(reads, ["other", "session"]);
    assert_eq!(request.url, "https://api.example/value-session");
    assert_eq!(
        request.headers,
        [("Authorization".into(), "Bearer value-session".into())]
    );
    assert_eq!(request.body, b"value-session{value-other");
    assert_eq!(request.method, "POST");
    assert_eq!(request.timeout_ms, Some(42));
}

#[test]
fn a_missing_secret_returns_its_name_and_no_request() {
    assert_eq!(
        DEFINITION.fill(|name| (name == "other").then(|| "yes".into()), preload_url),
        Err(PreloadFillError::MissingSecret("session"))
    );
}

#[test]
fn a_literal_definition_never_reads_secrets() {
    let definition = Preload {
        url: &[PreloadPart::Literal("https://api.example/")],
        headers: &[],
        body: None,
        secrets: &[],
        ..DEFINITION
    };
    assert_eq!(
        definition
            .fill(|_| panic!("no secret reference"), preload_url)
            .unwrap()
            .url,
        "https://api.example/"
    );
}
