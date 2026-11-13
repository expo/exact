//! The page around a document (LLP 1048.000 D3, D6, D7): its head, its
//! checkpoint, and the locations a build renders.

use exact_runner::{Answer, DataError, DataSource, Store, Value};
use exact_web::document::{build_locations, checkpoint, digest, read_checkpoint, route_at, Site};
use exact_web::Host;

#[derive(Clone, Default)]
struct Says(&'static str);

impl DataSource for Says {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        Ok(Value::str(self.0))
    }
}

fn host(src: &str, data: Says, launch: &str) -> Host<Says> {
    let plan = contract::bake(contract::compile(src).unwrap(), data.clone()).unwrap();
    Host::boot(&plan.encode(), data, Default::default(), launch)
        .unwrap()
        .0
}

const SITE: Site<'static> = Site {
    name: "Site & Co",
    origin: Some("https://example.com/"),
};

#[test]
fn the_head_carries_the_active_head_with_urls_a_crawler_can_follow() {
    let src = r#"
component A
  resource said = say() as shape string
  view
    column
      head title=said description="A <b>bold</b> \"claim\"" image="/assets/card.png" canonical="/post/7" robots="index, follow"
      text "a"
"#;
    let host = host(src, Says("Post & <Title>"), "/post/7");
    let head = host
        .document()
        .unwrap()
        .page_head(host.runner().plan(), &SITE, "/post/7")
        .unwrap();
    for want in [
        "<title>Post &amp; &lt;Title&gt;</title>",
        r#"<meta name="viewport" content="width=device-width, initial-scale=1">"#,
        r#"<meta name="description" content="A &lt;b&gt;bold&lt;/b&gt; &quot;claim&quot;">"#,
        r#"<meta name="robots" content="index, follow">"#,
        r#"<link rel="canonical" href="https://example.com/post/7">"#,
        r#"<meta property="og:site_name" content="Site &amp; Co">"#,
        r#"<meta property="og:url" content="https://example.com/post/7">"#,
        r#"<meta property="og:image" content="https://example.com/assets/card.png">"#,
        r#"<meta name="twitter:card" content="summary_large_image">"#,
    ] {
        assert!(head.contains(want), "{want}\n{head}");
    }
    assert!(!head.contains("preload"), "{head}");
}

#[test]
fn a_rendered_title_uses_the_selected_route_before_the_app_name() {
    exact_web::link(exact_web_capabilities::ALL);
    let src = r#"routes nav
  home "/"
    item "/item"
    explicit "/explicit"
    empty "/empty"
    bare "/bare"
    formatted "/formatted"

component A
  view
    main navigationKey=`${top(nav).id}` navigationBack="back"
      for e in stack(nav) key=e.id
        column navigationKey=`${e.id}`
          if e.name != "bare"
            header
              if e.name == "formatted"
                text "**Inbox** & [Archive](/archive)" markup="markdown" role="heading" aria-level=1
              else
                text (e.name == "home" ? "Home" : "Item & <Detail>") role="heading" aria-level=1
          if e.name == "explicit"
            head title="Authored"
          if e.name == "empty"
            head title=""
"#;
    for (location, title) in [
        ("/", "Home"),
        ("/item", "Item &amp; &lt;Detail&gt;"),
        ("/explicit", "Authored"),
        ("/empty", ""),
        ("/bare", "Site &amp; Co"),
        ("/formatted", "Inbox &amp; Archive"),
    ] {
        let host = host(src, Says(""), location);
        let head = host
            .document()
            .unwrap()
            .page_head(host.runner().plan(), &SITE, location)
            .unwrap();
        for want in [
            format!("<title>{title}</title>"),
            format!("<meta property=\"og:title\" content=\"{title}\">"),
            format!("<meta name=\"twitter:title\" content=\"{title}\">"),
        ] {
            assert!(head.contains(&want), "{location}: {want}\n{head}");
        }
    }
}

#[test]
fn a_route_title_uses_the_panel_named_by_the_roots_tabs() {
    exact_web::link(exact_web_capabilities::ALL);
    let host = host(
        r#"component App
  view
    main navigationBack="back" navigationKey="selected"
      row role="tablist"
        button role="tab" aria-controls="first"
          text "First"
        button role="tab" aria-controls="second"
          text "Second"
      column role="tabpanel" id="first"
        column navigationKey="other"
          header
            text "Covered" role="heading" aria-level=1
      column role="tabpanel" id="second"
        column navigationKey="selected"
          header
            column
              text "  Selected  " role="heading" aria-level=1
"#,
        Says(""),
        "/",
    );
    let head = host
        .document()
        .unwrap()
        .page_head(host.runner().plan(), &SITE, "/")
        .unwrap();
    assert!(head.starts_with("<title>Selected</title>"), "{head}");
}

#[test]
fn without_a_head_or_route_heading_the_title_is_the_apps() {
    let plain = host(
        "component A\n  view\n    column viewport-fit=\"cover\" interactive-widget=\"resizes-content\"\n      text \"a\"\n",
        Says(""),
        "/",
    );
    let doc = plain.document().unwrap();
    let no_origin = Site {
        name: "App",
        origin: None,
    };
    let head = doc
        .page_head(plain.runner().plan(), &no_origin, "/")
        .unwrap();
    assert!(head.starts_with("<title>App</title>"), "{head}");
    assert!(head.contains(
        r#"content="width=device-width, initial-scale=1, viewport-fit=cover, interactive-widget=resizes-content""#
    ));
    for absent in ["description", "canonical", "og:url", "og:image", "robots"] {
        assert!(!head.contains(absent), "{absent}: {head}");
    }
    assert!(head.contains(r#"<meta name="twitter:card" content="summary">"#));
    // A script-bearing canonical or image is dropped, never written.
    let host2 = host(
        "component A\n  view\n    column\n      head canonical=\"javascript:alert(1)\" image=\"javascript:alert(2)\"\n      text \"a\"\n",
        Says(""),
        "/",
    );
    let head = host2
        .document()
        .unwrap()
        .page_head(host2.runner().plan(), &SITE, "/")
        .unwrap();
    assert!(!head.contains("javascript"), "{head}");
}

#[test]
fn the_checkpoint_never_closes_its_script_or_opens_a_comment() {
    let host = host("component A\n  view\n    text \"a\"\n", Says(""), "/");
    let hostile = "/x</script><!--]]>&";
    let json = checkpoint(host.runner(), hostile);
    let (lt, gt, amp) = ("\\u003c", "\\u003e", "\\u0026");
    assert_eq!(
        json,
        format!(
            "{{\"location\":\"/x{lt}/script{gt}{lt}!--]]{gt}{amp}\",\"time\":0,\"logic\":null,\"answers\":[],\"pending\":[]}}"
        )
    );
    for bad in ["<", ">", "&"] {
        assert!(!json.contains(bad), "{json}");
    }
}

#[test]
fn a_checkpoint_reads_back_as_the_runner_wrote_it() {
    // Answers carry user text; the page carries them as base64.
    let host = host(
        "component A\n  resource said = say() as shape string\n  view\n    text said\n",
        Says("</script><!-- & \u{1F4AC}"),
        "/said?q=1",
    );
    let json = checkpoint(host.runner(), "/said?q=1");
    assert!(
        !json.contains('<') && !json.contains('>') && !json.contains('&'),
        "{json}"
    );
    // No stream is open and no answer read the store, so neither field is written
    assert!(
        !json.contains("\"open\"") && !json.contains("\"readers\""),
        "{json}"
    );
    let read = read_checkpoint(&json).unwrap();
    assert_eq!(read, host.runner().document_checkpoint("/said?q=1"));
    assert_eq!(read.answers[0].3, Value::str("</script><!-- & \u{1F4AC}"));
    for broken in [
        "",
        "{}x",
        "{\"time\":\"0\"}",
        "{\"answers\":\"!!\"}",
        "{\"extra\":1}",
    ] {
        assert!(read_checkpoint(broken).is_err(), "{broken}");
    }
}

/// `viewer()` answers the store's `token`.
struct Viewer;

impl DataSource for Viewer {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }

    fn answer(&mut self, store: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
        Ok(Answer::Now(Value::str(
            store.get("token").unwrap_or("anonymous"),
        )))
    }

    fn grants(&self) -> &str {
        "secret.keep token\n"
    }
}

#[test]
fn a_checkpoint_lists_the_answers_that_read_the_store() {
    let plan = contract::compile(
        "component A\n  resource viewer = viewer() as shape string\n  view\n    text viewer\n",
    )
    .unwrap();
    let host = Host::boot(&plan.encode(), Viewer, Default::default(), "/")
        .unwrap()
        .0;
    let json = checkpoint(host.runner(), "/");
    assert!(json.ends_with(",\"readers\":[0]}"), "{json}");
    assert_eq!(
        read_checkpoint(&json).unwrap(),
        host.runner().document_checkpoint("/")
    );
}

#[test]
fn checkpoint_values_are_json_that_reads_back_exactly() {
    // Every kind of plan value, as `push_value` spells it: text a page's
    // compression matches against the document's own strings.
    let text = r#"{"location":"/","time":1.5,"logic":null,"answers":[["a","src",["x",{"n":"NaN"}],{"r":[1,-0,2.25,true,null,{},{"s":"y"},["p","q"],{"n":"-Infinity"}]}]],"pending":["b"],"open":[0],"readers":[0]}"#;
    let read = read_checkpoint(text).unwrap();
    let (name, source, args, value) = &read.answers[0];
    assert_eq!((name.as_str(), source.as_str()), ("a", "src"));
    assert_eq!(args[0], Value::str("x"));
    assert!(matches!(args[1], Value::Number(n) if n.is_nan()));
    let Value::Record(fields) = value else {
        panic!("{value:?}")
    };
    assert_eq!(fields[0], Value::Number(1.0));
    assert!(matches!(fields[1], Value::Number(n) if n == 0.0 && n.is_sign_negative()));
    assert_eq!(fields[2], Value::Number(2.25));
    assert_eq!(fields[3], Value::Bool(true));
    assert_eq!(fields[4], Value::Unit);
    assert_eq!(fields[5], Value::NONE);
    assert_eq!(fields[6], Value::some(Value::str("y")));
    assert_eq!(
        fields[7],
        Value::list(vec![Value::str("p"), Value::str("q")])
    );
    assert_eq!(fields[8], Value::Number(f64::NEG_INFINITY));
    assert_eq!(read.pending, ["b"]);
    assert_eq!((read.open, read.readers), (vec![0], vec![0]));
    assert!(read_checkpoint(&text.replace("{\"s\"", "{\"z\"")).is_err());
    // Refuse an `open` or `readers` index past the answers
    for field in ["open", "readers"] {
        let past = text.replace(&format!("\"{field}\":[0]"), &format!("\"{field}\":[1]"));
        assert!(read_checkpoint(&past).is_err(), "{past}");
    }
}

#[test]
fn a_runtime_adopts_the_document_it_would_have_rendered() {
    let plan = caltrain::build().unwrap().encode();
    let boot = |launch: &str| {
        exact_web::link(exact_web_capabilities::ALL);
        Host::boot(&plan, caltrain_data::Caltrain, Default::default(), launch)
            .unwrap()
            .0
    };
    // What a render at `/` writes into the page.
    let rendered = boot("/");
    let page = checkpoint(rendered.runner(), "/");
    let root = rendered.document().unwrap().root;
    let written = digest(&rendered.runner().plan().encode(), "/", &page, &root);
    let open = |launch: &str, page: &str, digest: &str| {
        let (host, batch) = Host::boot_checkpoint(
            &plan,
            caltrain_data::Caltrain,
            page,
            digest,
            Vec::new(),
            None,
            Default::default(),
            launch,
        )
        .unwrap();
        (host, batch)
    };
    let adopted = |batch: &str| batch.contains("{\"op\":\"adopt\",\"adopted\":true}");
    let (host, batch) = open("/", &page, &written);
    assert!(adopted(&batch), "{}", &batch[..200.min(batch.len())]);
    assert!(host
        .runner()
        .journal()
        .any(|l| l.contains("checkpoint: 7 of 7 answers taken")));
    // A document rendered at `/` does not belong to another path: render fresh.
    let (host, batch) = open("/elsewhere?utm=x", &page, &written);
    assert!(!batch.contains("\"op\":\"adopt\""));
    assert!(host
        .runner()
        .journal()
        .any(|l| l.contains("document: rendered at /, not /elsewhere?utm=x; rendering fresh")));
    assert!(!adopted(&open("/", &page, &"0".repeat(64)).1));
    let other = checkpoint(rendered.runner(), "/other");
    let digest_other = digest(
        &rendered.runner().plan().encode(),
        "/other",
        &other,
        "<p>not it</p>",
    );
    assert!(!adopted(&open("/other", &other, &digest_other).1));
    // A checkpoint that doesn't read boots as a page without one.
    let (host, batch) = open("/", "{not json", &written);
    assert!(!batch.contains("\"op\":\"adopt\""));
    assert!(host
        .runner()
        .journal()
        .any(|l| l.contains("document: the checkpoint doesn't read")));
}

#[test]
fn a_page_whose_view_ignores_the_query_adopts_its_canonical_path() {
    let plan = contract::bake(
        contract::compile(
            r#"routes nav
  home "/" render=build
component App
  resource greeting = say() as shape string
  view
    text greeting
"#,
        )
        .unwrap(),
        Says("Hello"),
    )
    .unwrap()
    .encode();
    exact_web::link(exact_web_capabilities::ALL);
    let rendered = Host::boot(&plan, Says("Hello"), Default::default(), "/")
        .unwrap()
        .0;
    let page = checkpoint(rendered.runner(), "/");
    let root = rendered.document().unwrap().root;
    let written = digest(&rendered.runner().plan().encode(), "/", &page, &root);
    let (host, batch) = Host::boot_checkpoint(
        &plan,
        Says("Hello"),
        &page,
        &written,
        Vec::new(),
        None,
        Default::default(),
        "//?utm=x",
    )
    .unwrap();
    assert!(batch.contains("{\"op\":\"adopt\",\"adopted\":true}"));
    assert!(batch.contains("\"url\":\"/?utm=x\""), "{batch}");
    assert_eq!(host.location(), "/?utm=x");
}

#[test]
fn a_build_renders_the_routes_that_ask_and_the_not_found_document() {
    let plan = contract::compile(
        "routes nav\n  tab home \"/\" render=build\n    about \"/about\" render=build\n    post \"/post/:post\" render=request\n  tab client \"/client\"\n  notfound render=build\ncomponent A\n  view\n    text \"a\"\n",
    )
    .unwrap();
    assert_eq!(
        build_locations(&plan).unwrap(),
        vec![
            ("/".to_owned(), false),
            ("/about".to_owned(), false),
            ("/404.html".to_owned(), true)
        ]
    );
    // Undeclared routes render nothing at build.
    let plain = contract::compile(
        "routes nav\n  tab home \"/\"\n  notfound\ncomponent A\n  view\n    text \"a\"\n",
    )
    .unwrap();
    assert!(build_locations(&plain).unwrap().is_empty());
    // The not-found document's location must be one no pattern matches.
    let clash = contract::compile("routes nav\n  tab home \"/\" render=build\n  page \"/404.html\"\n  notfound render=build\ncomponent A\n  view\n    text \"a\"\n").unwrap();
    assert!(build_locations(&clash)
        .unwrap_err()
        .contains("route `page` matches"));
}

#[test]
fn the_plans_fonts_are_declared_for_a_reader_without_javascript() {
    // TypeTour's directory, for its font files.
    let app =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/typetour/app.contract");
    let src = "font \"Exciting Lilt\"\n  700 = \"assets/ExcitingLilt-Bold.ttf\"\ncomponent A\n  view\n    text \"a\" font-family=\"Exciting Lilt\" font-weight=700\n";
    let plan = contract::compile_path_source(&app, src).unwrap();
    let host = Host::boot(&plan.encode(), Says(""), Default::default(), "/")
        .unwrap()
        .0;
    let doc = host.document().unwrap();
    let head = doc.page_head(host.runner().plan(), &SITE, "/").unwrap();
    let family = head
        .split("@font-face{font-family:\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap();
    assert!(family.starts_with("ExactPlanStack"), "{head}");
    assert!(
        head.contains(&format!(
            r#"<style>@font-face{{font-family:"{family}";src:url("assets/ExcitingLilt-Bold.ttf");font-weight:700;font-stretch:100%;font-style:normal}}</style>"#
        )),
        "{head}"
    );
    assert!(
        head.contains(r#"<link rel="preload" href="assets/ExcitingLilt-Bold.ttf" as="font" type="font/ttf" crossorigin>"#),
        "{head}"
    );
    // The document's text asks for the same family name.
    assert!(
        doc.root
            .contains(&format!("font-family:&quot;{family}&quot;")),
        "{}",
        doc.root
    );
}

#[test]
fn a_location_resolves_to_its_route_or_the_not_found_route() {
    let plan = contract::compile(
        "routes nav\n  tab home \"/\" render=build\n    post \"/post/:post\" render=cached\n  notfound\ncomponent A\n  view\n    text \"a\"\n",
    )
    .unwrap();
    let name = |at: &str| route_at(&plan, at).map(|r| plan.str(r.name).to_string());
    assert_eq!(name("/").as_deref(), Some("home"));
    assert_eq!(name("/post/7?x=1").as_deref(), Some("post"));
    assert!(route_at(&plan, "/nowhere").unwrap().notfound);
    let plain = contract::compile("component A\n  view\n    text \"a\"\n").unwrap();
    assert!(route_at(&plain, "/").is_none());
}

#[test]
fn resolved_language_is_published_at_boot_and_after_switching() {
    let mut plan = contract::compile("component App\n  state language = \"en\"\n  view\n    column width=300\n      text \"123\" testId=\"text\"\n").unwrap();
    plan.locale = Some(exact_plan::SlotsId(0));
    // No format data: the formatters print en-US (LLP 1054.000.003 D6).
    let format = exact_plan::StrId(plan.strings.len() as u32);
    plan.strings.push("");
    for rtl in [false, true] {
        let name = exact_plan::StrId(plan.strings.len() as u32);
        plan.strings.push(if rtl { "ar" } else { "en" });
        plan.locales.push(exact_plan::LocalesRow {
            name,
            rtl,
            format,
            texts: Default::default(),
        });
    }
    let plan = contract::bake(plan, Says("")).unwrap();
    let (mut host, batch) = Host::boot(&plan.encode(), Says(""), Default::default(), "/").unwrap();
    assert!(batch.contains(r#"{"op":"language","lang":"en","dir":"ltr"}"#));
    let batch = host.set_place("ar-EG", "UTC", None, None);
    assert!(
        batch.contains(r#"{"op":"language","lang":"ar","dir":"rtl"}"#),
        "{batch}"
    );
    let doc = host.document().unwrap();
    assert_eq!((doc.lang.as_str(), doc.dir.as_str()), ("ar", "rtl"));
    assert!(!host
        .set_place("ar-SA", "UTC", None, None)
        .contains(r#""op":"language""#));
    let batch = host.set_place("de", "UTC", None, None);
    assert!(batch.contains(r#"{"op":"language","lang":"en","dir":"ltr"}"#));
}

/// What the page knows at launch (`exact_set_launch`): the boot's first
/// batch already says the place's language and the date's offset, which
/// the page used to tell only after the boot; a fact that does not validate
/// leaves the last one, and the running runner is not told.
#[test]
fn the_launch_place_and_date_are_the_first_batchs() {
    let mut plan = contract::compile("shape Time\n  utcOffset: number\n  locale: string\ncomponent App\n  state language = \"en\"\n  state seen = language\n  resource time = exactTime() as shape Time\n  view\n    text `${time.locale}/${time.utcOffset}/${seen}` testId=\"text\"\n").unwrap();
    plan.locale = Some(exact_plan::SlotsId(0));
    // No format data: the formatters print en-US (LLP 1054.000.003 D6).
    let format = exact_plan::StrId(plan.strings.len() as u32);
    plan.strings.push("");
    for rtl in [false, true] {
        let name = exact_plan::StrId(plan.strings.len() as u32);
        plan.strings.push(if rtl { "ar" } else { "en" });
        plan.locales.push(exact_plan::LocalesRow {
            name,
            rtl,
            format,
            texts: Default::default(),
        });
    }
    let plan = contract::bake(plan, Says("")).unwrap().encode();
    let mut bridge = exact_web::abi::Bridge::new();
    bridge.set_launch_time(1_790_000_000_000.0, 120.0);
    let n = bridge.input_write(b"ar-EG\0Africa/Cairo\x0042");
    bridge.set_launch_place(n);
    bridge.set_launch_time(f64::NAN, 0.0);
    let n = bridge.input_write(b"\0Africa/Cairo\x0042");
    bridge.set_launch_place(n);
    let len = bridge.boot(&plan, Says(""), 390.0, 844.0, "/");
    let batch = String::from_utf8_lossy(bridge.output_bytes(len as usize)).into_owned();
    assert!(
        batch.contains(r#"{"op":"language","lang":"ar","dir":"rtl"}"#),
        "{batch}"
    );
    // A root initializer reading the locale slot reads the launch's.
    assert!(batch.contains("ar-EG/120/ar"), "{batch}");
}

#[test]
fn a_direct_stored_boot_knows_the_launch_facts_before_settlement() {
    use exact_runner::time::{Launch, Place, WallTime};
    use std::{cell::RefCell, rc::Rc};

    struct Observed(Rc<RefCell<Vec<Vec<Value>>>>);
    impl DataSource for Observed {
        fn query(&mut self, _: &str, args: &[Value]) -> Result<Value, DataError> {
            self.0.borrow_mut().push(args.to_vec());
            Ok(Value::str("observed"))
        }
    }
    let mut plan = contract::compile(
        r#"shape Time
  epochAtZero: number
  utcOffset: number
  locale: string
  formatLocale: string
  timeZone: string
  seed: number
component App
  state language = "en"
  state seen = language
  resource time = exactTime() as shape Time
  state started = performanceNow()
  resource observed = observe(time, seen, started) as shape string
  view
    text observed
"#,
    )
    .unwrap();
    plan.locale = Some(exact_plan::SlotsId(0));
    let format = exact_plan::StrId(plan.strings.len() as u32);
    plan.strings.push("");
    for language in ["en", "ar"] {
        let name = exact_plan::StrId(plan.strings.len() as u32);
        plan.strings.push(language);
        plan.locales.push(exact_plan::LocalesRow {
            name,
            rtl: language == "ar",
            format,
            texts: Default::default(),
        });
    }
    let plan = plan.encode();
    let mut facts = Launch {
        time: Some(WallTime {
            epoch_at_zero: 1_790_000_000_000.,
            utc_offset: 120.,
        }),
        place: Some(Place {
            locale: "ar-EG".into(),
            format_locale: "en-KR-u-hc-h23".into(),
            time_zone: "Europe/Berlin".into(),
            seed: 42.,
        }),
        clock: Some(250.),
        ..Launch::NONE
    };
    let expected = |facts: &Launch| {
        let time = facts.time.unwrap();
        let place = facts.place.as_ref().unwrap();
        Value::Record(
            vec![
                Value::Number(time.epoch_at_zero),
                Value::Number(time.utc_offset),
                Value::str(&place.locale),
                Value::str(&place.format_locale),
                Value::str(&place.time_zone),
                Value::Number(place.seed),
            ]
            .into(),
        )
    };
    let calls = Rc::new(RefCell::new(Vec::new()));
    let (mut host, first) = Host::boot_stored(
        &plan,
        Observed(calls.clone()),
        Vec::new(),
        Default::default(),
        "/",
        facts.clone(),
    )
    .unwrap();
    assert_eq!(
        *calls.borrow(),
        vec![vec![
            expected(&facts),
            Value::str("ar"),
            Value::Number(250.)
        ]]
    );
    assert_eq!(host.runner().slot("seen"), Some(&Value::str("ar")));
    assert_eq!(host.runner().now_ms(), 250.);
    assert!(first.contains("observed"), "{first}");

    host.advance(1250.);
    let carried = host.carry();
    facts.time.as_mut().unwrap().epoch_at_zero += 1000.;
    facts.clock = Some(9000.);
    let current = expected(&facts);
    calls.borrow_mut().clear();
    let (host, _) = exact_runner::time::launching_with(facts, || {
        Host::boot_with(
            &plan,
            Observed(calls.clone()),
            Some(&carried),
            Default::default(),
            "/",
        )
    })
    .unwrap();
    assert_eq!(host.runner().now_ms(), 1250.);
    assert_eq!(
        *calls.borrow(),
        vec![vec![current, Value::str("ar"), Value::Number(250.)]]
    );
}
