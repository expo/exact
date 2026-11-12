use super::serve::{dist, get, serving};
use exact_plan::Value;
use exact_render::{Serve, Server};
use exact_runner::{DataError, DataSource};
use std::time::Duration;

struct Answers;

impl DataSource for Answers {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        Ok(Value::str(&"answer".repeat(300)))
    }
}

#[test]
fn waiting_request_pages_name_large_checkpoints() {
    super::warm_transport();
    let dir = dist("deferred-checkpoints");
    let plan = contract::compile(
        "routes nav\n  home \"/\" render=request activate=interaction\n\ncomponent App\n  resource answer = answer() as shape string\n  view\n    text answer\n",
    ).unwrap();
    let server = Server::bind(
        Serve {
            dist: dir.clone(),
            port: 0,
            name: "Answers".into(),
            origin: None,
            deadline: Duration::from_secs(10),
            renders: 1,
            queue: 8,
            viewport: Default::default(),
            lifetime: Duration::ZERO,
            generations: None,
        },
        plan,
        "",
    )
    .unwrap();
    let running = serving(server, || Answers);
    let (status, _, page) = get(running.addr, "/");
    assert_eq!(status, 200);
    let src = page
        .split("data-src=\"")
        .nth(1)
        .expect("a named checkpoint")
        .split('"')
        .next()
        .unwrap();
    let (status, headers, body) = get(running.addr, src);
    assert_eq!(status, 200);
    assert!(headers.contains(&("content-type".into(), "application/json".into())));
    assert!(headers.contains(&(
        "cache-control".into(),
        "private, max-age=3600, immutable".into()
    )));
    let checkpoint: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(checkpoint["location"], "/");
    assert_eq!(checkpoint["answers"][0][3], "answer".repeat(300));
    drop(running);
    std::fs::remove_dir_all(dir).unwrap();
}
