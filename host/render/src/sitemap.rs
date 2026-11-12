use super::*;

/// The sitemap (LLP 1048.000 D2): every rendered route's location — a
/// parameterized one's as its `pages=` source lists them now, a route
/// without one left out — absolute against the configured origin. Without
/// an origin there is none.
pub(super) fn sitemap<D: DataSource + 'static>(shared: &Shared, data: fn() -> D) -> Response {
    let Some(origin) = shared.serve.origin.as_deref() else {
        return Response::text(404, "no origin, no sitemap\n");
    };
    let plan = &shared.plan;
    let mut locations = Vec::new();
    for row in plan
        .routes
        .iter()
        .filter(|r| r.render != RenderPolicy::Client && !r.notfound)
    {
        let pattern = plan.str(row.pattern);
        if !pattern.split('/').any(|s| s.starts_with(':')) {
            locations.push(pattern.to_string());
            continue;
        }
        let listed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::pages(plan, data(), row, shared.serve.deadline)
        }))
        .unwrap_or_else(|_| Err("the pages source panicked".into()));
        match listed {
            Ok(found) => locations.extend(found),
            Err(error) => {
                crate::log(format_args!("sitemap 503 error={error:?}"));
                return Response::text(503, "the sitemap's pages didn't answer\n")
                    .header("Cache-Control", "no-store")
                    .header("Retry-After", "1");
            }
        }
    }
    let locations = indexed(shared, data, locations);
    let xml = |t: &str| {
        t.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    let origin = origin.trim_end_matches('/');
    let mut body = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    for location in &locations {
        let _ = writeln!(
            body,
            "  <url><loc>{}</loc></url>",
            xml(&format!("{origin}{location}"))
        );
    }
    body.push_str("</urlset>\n");
    let lifetime = shared.serve.lifetime.as_secs();
    Response {
        status: 200,
        headers: vec![
            ("Content-Type", "application/xml; charset=utf-8".into()),
            (
                "Cache-Control",
                format!("public, max-age=0, s-maxage={lifetime}"),
            ),
        ],
        body: body.into_bytes(),
        file: None,
        streamed: false,
    }
}
