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
            dist: dir.clone(),
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
    std::fs::remove_dir_all(dir).unwrap();
}
