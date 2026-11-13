//! The kernel-free render, the differential check's fast lane (LLP 1048.004
//! D6): a document written from its runner's instance tree is the one a
//! kernel's nodes give, byte for byte — the fixture's pages, and every
//! corpus and conformance plan's first page.

use super::{plan, warm_transport, Blog, Post, SITE};
use exact_plan::Value;
use exact_render::{render_with_at, Ids, Projection, Rendered};
use exact_runner::{Answer, DataError, DataSource, Request, Store};
use std::path::Path;
use std::time::Duration;

fn same(kernel: &Rendered, direct: &Rendered, what: &str) {
    assert_eq!(
        kernel.document.root, direct.document.root,
        "{what}: the root"
    );
    assert_eq!(kernel.head, direct.head, "{what}: the head");
    assert_eq!(
        kernel.checkpoint, direct.checkpoint,
        "{what}: the checkpoint"
    );
    assert_eq!(
        kernel.document.keyframes, direct.document.keyframes,
        "{what}: the keyframes"
    );
    assert_eq!(
        kernel.document.scroll_document, direct.document.scroll_document,
        "{what}: the scroller"
    );
    assert_eq!(kernel.activate, direct.activate, "{what}: the activation");
    assert_eq!(kernel.settled, direct.settled, "{what}: how it settled");
}

#[test]
fn the_fixture_s_pages_are_the_same_without_a_kernel() {
    warm_transport();
    let plan = plan();
    for (post, deadline) in [
        (Post::Soon, Duration::from_secs(5)),
        (Post::Never, Duration::from_millis(150)),
        (Post::Storage, Duration::from_secs(5)),
        (Post::Elsewhere, Duration::from_secs(5)),
    ] {
        let data = || Blog::new(post);
        let render = |projection| {
            render_with_at(
                &plan,
                &data,
                Default::default(),
                "/post/7",
                &SITE,
                deadline,
                Ids::Any,
                projection,
                1_700_000_000_000.0,
                &Default::default(),
            )
            .unwrap()
        };
        let (kernel, direct) = (render(Projection::Kernel), render(Projection::Direct));
        same(&kernel, &direct, "/post/7");
    }
}

/// A source that answers every call later, from an origin no grant names:
/// each resource keeps its placeholder, as a render refused by its
/// environment shows it.
#[derive(Default)]
struct Nowhere;

#[test]
fn direct_documents_keep_text_row_presence_and_clears() {
    let plan = contract::compile(
        "component App\n  view\n    column font-weight=700 line-height=2\n      text font-weight=\"unset\" line-height=\"unset\" \"Inherited\"\n",
    ).unwrap();
    let render = |projection| {
        render_with_at(
            &plan,
            &|| Nowhere,
            Default::default(),
            "/",
            &SITE,
            Duration::from_secs(2),
            Ids::Any,
            projection,
            1_700_000_000_000.0,
            &Default::default(),
        )
        .unwrap()
    };
    let (kernel, direct) = (render(Projection::Kernel), render(Projection::Direct));
    same(&kernel, &direct, "text row presence");
    for row in ["font-weight", "line-height"] {
        assert!(
            direct
                .document
                .root
                .contains(&format!("--exact-said-{row}: ;")),
            "{}",
            direct.document.root
        );
        assert!(
            direct
                .document
                .root
                .contains(&format!("--exact-said-{row}:inherit;")),
            "{}",
            direct.document.root
        );
    }
}

impl DataSource for Nowhere {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }

    fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
        Ok(Answer::Later(Request::get("https://nowhere.invalid/")))
    }
}

#[test]
fn every_corpus_and_conformance_plan_is_the_same_without_a_kernel() {
    warm_transport();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files: Vec<_> = ["contract/corpus", "host/web-js/conformance"]
        .iter()
        .flat_map(|dir| std::fs::read_dir(root.join(dir)).unwrap())
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "contract"))
        .collect();
    files.sort();
    let (mut compared, mut skipped) = (0, Vec::new());
    for file in &files {
        let Ok(plan) = contract::compile(&std::fs::read_to_string(file).unwrap()) else {
            continue;
        };
        let render = |projection| {
            render_with_at(
                &plan,
                &|| Nowhere,
                Default::default(),
                "/",
                &SITE,
                Duration::from_secs(2),
                Ids::Any,
                projection,
                1_700_000_000_000.0,
                &Default::default(),
            )
        };
        let what = file.display().to_string();
        let Ok(kernel) = render(Projection::Kernel) else {
            skipped.push(format!("{what}: no kernel render"));
            continue;
        };
        match render(Projection::Direct) {
            Ok(direct) => {
                same(&kernel, &direct, &what);
                compared += 1;
            }
            Err(why) => skipped.push(format!("{what}: {why}")),
        }
    }
    // Every plan a kernel renders is written without one too (LLP
    // 1048.004 stage 2: virtualized lists and `id` references included),
    // except one whose initializer reads a resource through a prop, which
    // `projects_as_booted` sends to the kernel (budget.contract, 976bef093),
    // and one with `rem` or `em` lengths, which the fold leaves to the
    // kernel's inherited font sizes (rem.contract, 077fe52b4).
    assert!(
        skipped.iter().all(|why| why.ends_with("no kernel render")
            || why.ends_with("a slot's initializer reads a resource")
            || why.ends_with("a relative length (rem, em)"))
            && compared > 0,
        "{compared} of {} plans compared; skipped: {skipped:#?}",
        files.len()
    );
}

#[test]
fn a_javascript_page_preloads_the_hero_from_a_default_render() {
    let plan = contract::compile(
        r#"
component Hero
  view
    column
      column inert=true
        image "assets/hero.png?one=1&two=2" alt="Covered"
      image "assets/hero.png?one=1&two=2" alt="Hero"
"#,
    )
    .unwrap();
    let rendered = exact_render::render(
        &plan,
        || Nowhere,
        Default::default(),
        "/",
        &SITE,
        Duration::from_secs(10),
    )
    .unwrap();
    assert!(!rendered.document.root.contains("fetchpriority"));
    let page = exact_render::page(super::serve::JS_SHELL, &rendered).unwrap();
    assert!(
        page.contains(
            "<link rel=\"preload\" as=\"image\" href=\"assets/hero.png?one=1&amp;two=2\">"
        ),
        "{page}"
    );
    assert!(
        page.contains("<img fetchpriority=\"high\" alt=\"Hero\""),
        "{page}"
    );
    assert_eq!(page.matches("fetchpriority=\"high\"").count(), 1);
}

#[test]
fn streamed_and_buffered_pages_keep_the_route_title_and_hero_image() {
    use super::serve::{dist, get, header, serving, JS_SHELL};
    use exact_render::{Serve, Server};

    warm_transport();
    let dir = dist("hero");
    std::fs::write(dir.join("shell.html"), JS_SHELL).unwrap();
    let plan = contract::compile(
        r#"
routes nav
  tab home "/" render=request
    kept "/kept" render=cached
  notfound render=build
component Hero
  view
    main navigationKey="home" navigationBack="back"
      column navigationKey="home"
        header
          text "**Hero** route" markup="markdown" role="heading" aria-level=1
        image "data:image/png;base64,AA==" alt="Inline"
        image "symbol:circle" alt="Symbol"
        column inert=true
          image "assets/inert.png" alt="Inert"
        column visibility="hidden"
          image "assets/hidden.png" alt="Hidden"
        image "assets/hero.png?one=1&two=2" alt="Hero"
        image "assets/second.png" alt="Second"
"#,
    )
    .unwrap();
    let served = serving(
        Server::bind(
            Serve {
                dist: dir,
                port: 0,
                name: "Hero".into(),
                origin: None,
                deadline: Duration::from_secs(10),
                renders: 2,
                queue: 8,
                viewport: Default::default(),
                lifetime: Duration::from_secs(120),
                generations: None,
            },
            plan,
            "",
        )
        .unwrap(),
        || Nowhere,
    );
    let check = |page: &str| {
        assert!(page.contains("<title>Hero route</title>"), "{page}");
        let link = "<link rel=\"preload\" as=\"image\" href=\"assets/hero.png?one=1&amp;two=2\">";
        let at = page.find(link).expect("the hero preload");
        assert!(at < page.find("<div id=\"exact-root\">").unwrap());
        assert!(
            page.contains("<img fetchpriority=\"high\" alt=\"Hero\""),
            "{page}"
        );
        assert_eq!(page.matches("fetchpriority=\"high\"").count(), 1);
        assert_eq!(page.matches("as=\"image\"").count(), 1);
    };
    let (status, _, buffered) = get(served.addr, "/");
    assert_eq!(status, 200);
    check(&buffered);
    let (streamed, _) = super::boot::navigate(served.addr, "/");
    check(&streamed);
    let (streamed, _) = super::boot::navigate(served.addr, "/kept");
    check(&streamed);
    let (status, headers, kept) = get(served.addr, "/kept");
    assert_eq!(status, 200);
    assert!(header(&headers, "age").is_some());
    check(&kept);
}

#[test]
fn hero_selection_skips_covered_routes_and_stops_at_lazy_rows() {
    let source = r#"
routes nav
  home "/" render=request
    detail "/detail" render=request
  notfound render=build
component Pages
  action follow(url: string)
    nav = go(nav, url)
  view
    main navigationKey=`${top(nav).id}` navigationBack="back" navigate=follow
      for e in stack(nav) key=e.id
        column navigationKey=`${e.id}` position="absolute" inset=0
          if e.name == "home"
            for id in ["a", "b"] key=id
              image `assets/${id}.png` alt=id
            image "assets/after-rows.png" alt="After rows"
          if e.name == "detail"
            image "assets/hero.png" alt="Hero"
"#;
    let plan = contract::compile(source).unwrap();
    for location in ["/", "/detail"] {
        let render = |projection| {
            render_with_at(
                &plan,
                &|| Nowhere,
                Default::default(),
                location,
                &SITE,
                Duration::from_secs(10),
                Ids::Any,
                projection,
                1_700_000_000_000.0,
                &Default::default(),
            )
            .unwrap()
        };
        let kernel = render(Projection::Kernel);
        let direct = render(Projection::Direct);
        same(&kernel, &direct, location);
        let expected = (location == "/detail").then_some("assets/hero.png");
        for rendered in [kernel, direct] {
            assert_eq!(
                rendered
                    .document
                    .hero_image
                    .as_ref()
                    .map(|image| image.src.as_str()),
                expected
            );
            let page = exact_render::page(super::serve::JS_SHELL, &rendered).unwrap();
            assert_eq!(
                page.matches("fetchpriority=\"high\"").count(),
                usize::from(expected.is_some())
            );
            assert_eq!(
                page.matches("as=\"image\"").count(),
                usize::from(expected.is_some())
            );
        }
    }
}
