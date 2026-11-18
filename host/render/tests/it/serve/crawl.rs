use super::*;

#[test]
fn the_sitemap_lists_each_rendered_route_and_its_listed_pages() {
    let served = start("sitemap", 1, 8, 300);
    let addr = served.addr;
    let (status, headers, body) = get(addr, "/sitemap.xml");
    assert_eq!(status, 200);
    assert_eq!(
        header(&headers, "content-type"),
        Some("application/xml; charset=utf-8")
    );
    for url in [
        "https://blog.test/",
        "https://blog.test/post/7",
        "https://blog.test/post/an%20idea",
    ] {
        assert!(body.contains(&format!("<loc>{url}</loc>")), "{url}\n{body}");
    }
    // A listed value whose location an earlier route answers is that route's page, listed once, as its own
    // (LLP 1048.000 D2)
    assert_eq!(
        body.matches("<loc>https://blog.test/post/latest</loc>")
            .count(),
        1,
        "{body}"
    );
    // A route without a pages source isn't in it; neither is the shell's,
    // nor a page that says `noindex`, as the build's sitemap leaves them.
    assert!(!body.contains("/live/") && !body.contains("/app"), "{body}");
    assert!(!body.contains("/draft/"), "{body}");
    assert_eq!(get(addr, "/draft/d1").0, 200);
    let (status, _, body) = get(addr, "/robots.txt");
    assert_eq!(status, 200);
    assert_eq!(
        body,
        "User-agent: *\nAllow: /\nSitemap: https://blog.test/sitemap.xml\n"
    );
}

#[test]
fn authored_routes_keep_the_crawler_paths() {
    let dir = dist("crawler-routes");
    let plan = contract::compile(
        r#"routes nav
  home "/" render=build
  robots "/robots.txt" render=build
  sitemap "/sitemap.xml" render=request

component Crawled
  view
    text `Authored ${top(nav).name}`
"#,
    )
    .unwrap();
    let server = Server::bind(
        Serve {
            dist: dir.to_path_buf(),
            port: 0,
            name: "Crawled".into(),
            origin: Some("https://blog.test".into()),
            deadline: Duration::from_secs(5),
            renders: 1,
            queue: 8,
            viewport: Default::default(),
            lifetime: Duration::from_secs(60),
            generations: None,
        },
        plan,
        "",
    )
    .unwrap();
    let served = serving(server, || Posts);
    for (path, name) in [("/robots.txt", "robots"), ("/sitemap.xml", "sitemap")] {
        let (status, headers, body) = get(served.addr, path);
        assert_eq!(status, 200);
        assert_eq!(
            header(&headers, "content-type"),
            Some("text/html; charset=utf-8")
        );
        assert!(body.contains(&format!("Authored {name}")), "{body}");
    }
    drop(served);
}

#[test]
fn fixture_documents_keep_not_found_status() {
    struct FixturePost;
    impl DataSource for FixturePost {
        fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
            Err(DataError::UnknownSource(source.into()))
        }

        fn answer(
            &mut self,
            store: &mut Store,
            source: &str,
            args: &[Value],
        ) -> Result<Answer, DataError> {
            if source == "post" {
                Ok(Answer::Later(Request::get("http://127.0.0.1:1/post")))
            } else {
                Posts.answer(store, source, args)
            }
        }

        fn parse(
            &mut self,
            _: &mut Store,
            _: &str,
            _: &[Value],
            outcome: Outcome,
        ) -> Result<Answer, DataError> {
            match outcome {
                Outcome::Response(response) => {
                    Ok(Answer::Now(post(&String::from_utf8_lossy(&response.body))))
                }
                _ => Err(DataError::Unavailable("post fetch failed".into())),
            }
        }

        fn grants(&self) -> &str {
            "net.fetch http://127.0.0.1:1/"
        }
    }
    let fixture = serde_json::json!({
        "status": 200, "headers": {"content-type": "text/plain"},
        "body": "Fixture not-found message"
    });
    let table = format!("http://127.0.0.1:1/post\t-\t-\t0\t1\t{fixture}\t0");
    let encoded = exact_data::envelope::base64(table.as_bytes());
    let mut statuses = Vec::new();
    for status in [404, 410] {
        let dir = dist(&format!("fixture-status-{status}"));
        let source = SRC
            .replace("status=404", &format!("status={status}"))
            .replace("text \"Nothing here\"", "text post.title");
        let plan = contract::compile(&source).unwrap();
        let server = Server::bind(
            Serve {
                dist: dir.to_path_buf(),
                port: 0,
                name: "Blog".into(),
                origin: None,
                deadline: Duration::from_secs(2),
                renders: 1,
                queue: 8,
                viewport: Default::default(),
                lifetime: Duration::from_secs(120),
                generations: None,
            },
            plan,
            FixturePost.grants(),
        )
        .unwrap();
        let served = serving(server, || FixturePost);
        let (actual, headers, body) = fetch(served.addr, &format!(
            "GET /missing?agent=1&failFetch=1 HTTP/1.1\r\nHost: localhost\r\nX-Exact-Faults: {encoded}\r\n\r\n"));
        statuses.push((
            actual,
            body.contains("Fixture not-found message"),
            header(&headers, "cache-control").map(str::to_owned),
        ));
    }
    assert_eq!(
        statuses,
        [
            (404, true, Some("no-store".into())),
            (410, true, Some("no-store".into())),
        ]
    );
}
