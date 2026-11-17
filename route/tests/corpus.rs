//! The language-neutral corpus and every law of the core.
//!
//! @ref LLP 1038 §3 (laws), D9 (these same records run through the TS binding).
//! Expected visits are plain JSON, including every intermediate value and
//! refusal. URL, component-encoding and query-read answers were read from
//! Chrome; no router implementation writes its own expected values here.

use exact_route::*;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

#[derive(Deserialize)]
struct Corpus {
    tables: BTreeMap<String, Table>,
    param_names: BTreeMap<String, Vec<String>>,
    canonical: Vec<Pair>,
    location_of: Vec<Pair>,
    fragments: Vec<Pair>,
    components: Vec<Pair>,
    search: Vec<Search>,
    matches: Vec<MatchCase>,
    chains: Vec<ChainCase>,
    paths: Vec<PathCase>,
    checks: Vec<CheckCase>,
    sequences: Vec<Sequence>,
    reads: Vec<ReadCase>,
    laws: Vec<Law>,
    law_locations: BTreeMap<String, Vec<String>>,
}
#[derive(Deserialize)]
struct Pair {
    input: String,
    expected: String,
}
#[derive(Deserialize)]
struct Search {
    url: String,
    name: String,
    expected: String,
}
#[derive(Deserialize)]
struct MatchCase {
    table: String,
    location: String,
    expected: Option<Match>,
}
#[derive(Deserialize)]
struct ChainCase {
    table: String,
    location: String,
    expected: Vec<Destination>,
}
#[derive(Deserialize)]
struct PathCase {
    table: String,
    name: String,
    params: Vec<String>,
    expected: Option<String>,
    error: Option<String>,
}
#[derive(Deserialize)]
struct CheckCase {
    name: String,
    table: Table,
    error: Option<String>,
    route: Option<usize>,
}
#[derive(Deserialize)]
struct Sequence {
    name: String,
    table: String,
    steps: Vec<Step>,
}
#[derive(Deserialize)]
struct Step {
    verb: String,
    arg: Option<String>,
    expected: Router,
    refusal: Option<Refusal>,
}
#[derive(Deserialize)]
struct ReadCase {
    sequence: usize,
    step: usize,
    name: String,
    expected: Vec<String>,
}
#[derive(Deserialize)]
struct Law {
    name: String,
    statement: String,
}

fn corpus() -> &'static Corpus {
    static CORPUS: OnceLock<Corpus> = OnceLock::new();
    CORPUS.get_or_init(|| serde_json::from_str(include_str!("corpus.json")).unwrap())
}

fn success(result: (Router, Option<Refusal>)) -> Router {
    assert_eq!(result.1, None);
    result.0
}

fn apply(table: &Table, r: Router, verb: &str, arg: Option<&str>) -> (Router, Option<Refusal>) {
    match verb {
        "launch" => Router::launch(table, arg.unwrap()),
        "open" => open(table, r, arg.unwrap()),
        "push" => push(table, r, arg.unwrap()),
        "replace" => replace(table, r, arg.unwrap()),
        "back" => back(table, r),
        "select" => select(table, r, arg.unwrap()),
        "go" => go(table, r, arg.unwrap()),
        _ => panic!("unknown corpus verb {verb}"),
    }
}

fn finish(failures: Vec<String>) {
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn corpus_tables_and_parameter_orders() {
    let c = corpus();
    assert_eq!(c.tables.len(), 3);
    for (name, table) in &c.tables {
        assert_eq!(table.check(), Ok(()), "{name}");
        assert_eq!(table.param_names(), c.param_names[name], "{name}");
        let launch = success(Router::launch(table, "/"));
        assert!(launch
            .tabs
            .iter()
            .all(|tab| tab.stack.len() == 1 && tab.stack[0].name == tab.name));
    }
    let value = &c.sequences[0].steps[0].expected;
    let json = serde_json::to_string(value).unwrap();
    assert!(json.contains(r#""params":{"person":"","post":"","question":""}"#));
    assert_eq!(serde_json::from_str::<Router>(&json).unwrap(), *value);
}

#[test]
fn corpus_matches_and_chains() {
    let c = corpus();
    let mut failures = Vec::new();
    for case in &c.matches {
        let table = &c.tables[&case.table];
        let result = table.matches(&case.location);
        let expected_pattern = case
            .expected
            .clone()
            .filter(|m| table.routes.iter().any(|r| r.name == m.name && !r.notfound));
        let pattern = table.matches_pattern(&case.location);
        if pattern != expected_pattern {
            failures.push(format!(
                "pattern {} {}: {pattern:?} != {expected_pattern:?}",
                case.table, case.location
            ));
        }
        if result != case.expected {
            failures.push(format!(
                "match {} {}: {result:?} != {:?}",
                case.table, case.location, case.expected
            ));
        }
    }
    for case in &c.chains {
        let result = c.tables[&case.table].chain(&case.location);
        if result != case.expected {
            failures.push(format!(
                "chain {} {}: {result:?} != {:?}",
                case.table, case.location, case.expected
            ));
        }
    }
    finish(failures);
}

#[test]
fn corpus_paths_and_static_rejects() {
    let c = corpus();
    let mut failures = Vec::new();
    for case in &c.paths {
        let args: Vec<&str> = case.params.iter().map(String::as_str).collect();
        let result = c.tables[&case.table].path(&case.name, &args);
        let (value, error) = match result {
            Ok(s) => (Some(s), None),
            Err(e) => (None, Some(e.code)),
        };
        if value != case.expected || error != case.error {
            failures.push(format!(
                "path {} {}: {value:?}, {error:?}",
                case.table, case.name
            ));
        }
    }
    for case in &c.checks {
        let result = case.table.check().err();
        let error = result.as_ref().map(|e| &e.code);
        let route = result.as_ref().map(|e| e.route);
        if error != case.error.as_ref() || route != case.route {
            failures.push(format!("check {}: {result:?}", case.name));
        }
    }
    finish(failures);
}

#[test]
fn corpus_incoming_urls_and_component_encoding() {
    let mut failures = Vec::new();
    for (pairs, operation) in [
        (&corpus().location_of, location_of as fn(&str) -> String),
        (&corpus().fragments, fragment_of),
        (&corpus().components, encode_uri_component),
    ] {
        for pair in pairs {
            let actual = operation(&pair.input);
            if actual != pair.expected {
                failures.push(format!(
                    "{:?}: {actual:?} != {:?}",
                    pair.input, pair.expected
                ));
            }
        }
    }
    finish(failures);
}

#[test]
fn corpus_reads() {
    let c = corpus();
    let mut failures = Vec::new();
    for case in &c.search {
        let e = Entry {
            id: 0,
            name: String::new(),
            url: case.url.clone(),
            tab: String::new(),
            params: Params::new(),
            fragment: String::new(),
        };
        let value = search_param(&e, &case.name);
        if value != case.expected {
            failures.push(format!(
                "search {:?} {:?}: {value:?} != {:?}",
                case.url, case.name, case.expected
            ));
        }
    }
    for case in &c.reads {
        let r = &c.sequences[case.sequence].steps[case.step].expected;
        let value = params(r, &case.name);
        if value != case.expected {
            failures.push(format!(
                "params {} {} {:?}: {value:?} != {:?}",
                case.sequence, case.step, case.name, case.expected
            ));
        }
    }
    for sequence in &c.sequences {
        for step in &sequence.steps {
            let r = &step.expected;
            let expected = &r.tabs.iter().find(|t| t.name == r.tab).unwrap().stack;
            assert_eq!(stack(r), expected);
            assert_eq!(top(r), expected.last());
            assert_eq!(depth(r), expected.len());
        }
    }
    finish(failures);
}

#[test]
fn corpus_verb_sequences() {
    let c = corpus();
    let mut failures = Vec::new();
    for sequence in &c.sequences {
        let table = &c.tables[&sequence.table];
        let mut r = Router::default();
        for (i, step) in sequence.steps.iter().enumerate() {
            let before = r.clone();
            let (next, refusal) = apply(table, r, &step.verb, step.arg.as_deref());
            if next != step.expected || refusal != step.refusal {
                failures.push(format!(
                    "{} step {i} {} {:?}\nactual {} {:?}\nexpected {} {:?}",
                    sequence.name,
                    step.verb,
                    step.arg,
                    serde_json::to_string(&next).unwrap(),
                    refusal,
                    serde_json::to_string(&step.expected).unwrap(),
                    step.refusal
                ));
            }
            if refusal.is_some() {
                assert_eq!(next, before, "a refused verb changed its value");
            }
            assert!(next.next >= before.next);
            let before_ids: BTreeSet<_> = before
                .tabs
                .iter()
                .flat_map(|t| &t.stack)
                .map(|e| e.id)
                .collect();
            for entry in next.tabs.iter().flat_map(|t| &t.stack) {
                assert!(
                    before_ids.contains(&entry.id) || entry.id >= before.next,
                    "an old id was reused"
                );
            }
            r = next;
        }
    }
    finish(failures);
}

// Each named law is a test, exercised over every recorded reachable value of
// all three tables. The JSON supplies its statement and the location domain.
fn law(name: &str, run: impl Fn(&Table, &Router, &[String])) {
    let c = corpus();
    assert_eq!(c.laws.len(), 11);
    assert!(!c
        .laws
        .iter()
        .find(|l| l.name == name)
        .unwrap()
        .statement
        .is_empty());
    for sequence in &c.sequences {
        for step in &sequence.steps {
            run(
                &c.tables[&sequence.table],
                &step.expected,
                &c.law_locations[&sequence.table],
            );
        }
    }
}

#[test]
fn law_back_undoes_a_push_on_the_selected_stack() {
    law("back_push", |t, r, urls| {
        for url in urls {
            if top(r).is_some_and(|e| e.url == canonical(url)) {
                continue; // the top's own location: law_push_of_the_top_is_unchanged
            }
            let pushed = success(push(t, r.clone(), url));
            let popped = success(back(t, pushed));
            assert_eq!(stack(&popped), stack(r));
            assert_eq!(popped.tabs, r.tabs);
        }
    });
}
#[test]
fn law_push_of_the_top_is_unchanged() {
    // A same-URL navigation replaces its entry (HTML); a link to the screen
    // shown adds no visit.
    law("push_top", |t, r, _| {
        if let Some(location) = top(r).map(Entry::location) {
            assert_eq!(&success(push(t, r.clone(), &location)), r);
        }
    });
}
#[test]
fn law_back_at_root_is_unchanged() {
    law("back_root", |t, r, _| {
        let root = success(select(t, r.clone(), &r.tab));
        assert_eq!(depth(&root), 1);
        assert_eq!(success(back(t, root.clone())), root);
    });
}
#[test]
fn law_selecting_a_tab_twice_leaves_its_root() {
    law("select_twice", |t, r, _| {
        for tab in &r.tabs {
            let once = success(select(t, r.clone(), &tab.name));
            let twice = success(select(t, once, &tab.name));
            assert_eq!(depth(&twice), 1);
            assert_eq!(top(&twice), tab.stack.first());
        }
    });
}
#[test]
fn law_open_is_idempotent() {
    law("open_idempotent", |t, r, urls| {
        for url in urls {
            let once = success(open(t, r.clone(), url));
            assert_eq!(success(open(t, once.clone(), url)), once);
        }
    });
}
#[test]
fn law_replace_keeps_the_visit_id() {
    law("replace_id", |t, r, urls| {
        for url in urls {
            let (replaced, refusal) = replace(t, r.clone(), url);
            if let Some(refusal) = refusal {
                assert_eq!(depth(r), 1);
                assert_ne!(t.matches(url).unwrap().name, r.tab);
                assert_eq!(
                    refusal.message,
                    "replace cannot change the tab's root route"
                );
                assert_eq!(replaced, *r);
            }
            assert_eq!(top(&replaced).unwrap().id, top(r).unwrap().id);
            assert_eq!(replaced.next, r.next);
            assert_eq!(replaced.tab, r.tab);
        }
    });
}
#[test]
fn law_go_to_the_top_is_unchanged() {
    law("go_top", |t, r, _| {
        assert_eq!(success(go(t, r.clone(), &top(r).unwrap().location())), *r);
    });
}
#[test]
fn law_a_fragment_only_change_is_the_same_visit() {
    // @ref LLP 1153 D5 — identity is the path and query; the fragment rides.
    law("fragment_in_place", |t, r, _| {
        let top_url = top(r).unwrap().url.clone();
        for fragment in ["", "moves", "b%20c"] {
            let location = match fragment {
                "" => top_url.clone(),
                f => format!("{top_url}#{f}"),
            };
            let mut expected = r.clone();
            let index = expected
                .tabs
                .iter()
                .position(|tab| tab.name == r.tab)
                .unwrap();
            expected.tabs[index].stack.last_mut().unwrap().fragment = fragment.into();
            for verb in [push, replace, go] {
                assert_eq!(
                    success(verb(t, r.clone(), &location)),
                    expected,
                    "{location}"
                );
            }
        }
    });
}
#[test]
fn law_visit_ids_are_distinct_and_below_next() {
    law("unique_ids", |_, r, _| {
        let mut ids = BTreeSet::new();
        for tab in &r.tabs {
            assert!(!tab.stack.is_empty());
            assert_eq!(tab.stack[0].name, tab.name);
            for entry in &tab.stack {
                assert!(ids.insert(entry.id));
                assert!(entry.id < r.next);
                assert_eq!(entry.tab, tab.name);
            }
        }
    });
}
#[test]
fn law_entries_roundtrip_through_matching() {
    law("entry_roundtrip", |t, r, _| {
        for entry in r.tabs.iter().flat_map(|t| &t.stack) {
            assert_eq!(canonical(&entry.url), entry.url);
            assert_eq!(
                t.matches(&entry.url),
                Some(Match {
                    name: entry.name.clone(),
                    params: entry.params.clone()
                })
            );
        }
    });
}
#[test]
fn law_canonical_is_idempotent_and_agrees_with_chrome() {
    assert!(corpus()
        .laws
        .iter()
        .any(|l| l.name == "canonical_idempotent"));
    let mut failures = Vec::new();
    for pair in &corpus().canonical {
        let actual = canonical(&pair.input);
        if actual != pair.expected {
            failures.push(format!(
                "Chrome {:?}: {actual:?} != {:?}",
                pair.input, pair.expected
            ));
        }
        if canonical(&actual) != actual {
            failures.push(format!(
                "not idempotent: {:?} -> {actual:?} -> {:?}",
                pair.input,
                canonical(&actual)
            ));
        }
    }
    finish(failures);
}

#[test]
fn empty_values_and_id_exhaustion_refuse_without_partial_changes() {
    let table = &corpus().tables["messages"];
    let empty = Router::default();
    assert_eq!(stack(&empty), []);
    assert_eq!(top(&empty), None);
    assert_eq!(depth(&empty), 0);
    assert_eq!(success(back(table, empty.clone())), empty);
    for verb in ["open", "push", "replace", "go"] {
        let (after, reason) = apply(table, empty.clone(), verb, Some("/absent"));
        assert_eq!(after, empty);
        assert_eq!(reason.unwrap().message, "no route matches /absent");
    }
    let mut exhausted = success(Router::launch(table, "/"));
    exhausted.next = (1u64 << 53) - 1;
    for verb in ["push", "open", "go"] {
        let (after, reason) = apply(table, exhausted.clone(), verb, Some("/t/5/details"));
        assert_eq!(after, exhausted);
        assert_eq!(reason.unwrap().message, "router entry ids exhausted");
    }
    assert_eq!(
        success(replace(table, exhausted.clone(), "/?draft=1")).next,
        exhausted.next
    );
    let mut almost = exhausted;
    almost.next -= 1;
    let (after, reason) = open(table, almost.clone(), "/t/5/details");
    assert_eq!(
        after, almost,
        "allocating the first ancestor must not partly commit"
    );
    assert_eq!(reason.unwrap().message, "router entry ids exhausted");
}

// @ref LLP 1038 D9 — replay the existing expected values through the real VM.
#[path = "../../runner/tests/it/support/router.rs"]
mod runner_plan;

struct NoData;
impl exact_runner::DataSource for NoData {
    fn query(
        &mut self,
        source: &str,
        _: &[exact_plan::Value],
    ) -> Result<exact_plan::Value, exact_runner::DataError> {
        panic!("corpus has no source {source}")
    }
}

#[test]
fn corpus_verb_sequences_through_the_runner() {
    let c = corpus();
    let mut failures = Vec::new();
    for sequence in &c.sequences {
        let plan = runner_plan::plan(&c.tables[&sequence.table]);
        let mut runner = None;
        for (i, step) in sequence.steps.iter().enumerate() {
            if step.verb == "launch" {
                runner = Some(
                    exact_runner::Runner::boot(
                        plan.clone(),
                        NoData,
                        exact_kernel::Kernel::with_monospace(),
                        Default::default(),
                        step.arg.as_deref().unwrap(),
                    )
                    .unwrap(),
                );
            } else {
                let args = step
                    .arg
                    .as_deref()
                    .map(exact_plan::Value::str)
                    .into_iter()
                    .collect();
                runner
                    .as_mut()
                    .expect("launch first")
                    .act(&step.verb, args)
                    .unwrap();
            }
            let r = runner.as_ref().unwrap();
            let value = r.carry().router.unwrap().1;
            if value != step.expected {
                failures.push(format!(
                    "runner {} step {i}: {value:?} != {:?}",
                    sequence.name, step.expected
                ));
            }
            if let Some(refusal) = &step.refusal {
                if !r.journal().any(|line| line.contains(&refusal.message)) {
                    failures.push(format!(
                        "runner {} step {i}: missing refusal {refusal}",
                        sequence.name
                    ));
                }
            }
        }
    }
    finish(failures);
}

#[test]
fn corpus_paths_survive_push_without_losing_parameters() {
    let c = corpus();
    for case in &c.paths {
        let table = &c.tables[&case.table];
        let args = case.params.iter().map(String::as_str).collect::<Vec<_>>();
        let Ok(url) = table.path(&case.name, &args) else {
            continue;
        };
        let r = success(push(table, success(Router::launch(table, "/")), &url));
        let entry = top(&r).unwrap();
        assert_eq!(entry.name, case.name);
        let route = table
            .routes
            .iter()
            .find(|route| route.name == case.name)
            .unwrap();
        for (name, value) in route
            .pattern
            .split('/')
            .filter_map(|s| s.strip_prefix(':'))
            .zip(&case.params)
        {
            assert_eq!(&entry.params[name], value);
        }
    }
}

#[test]
fn path_errors_identify_names_and_ordered_parameters_without_changing_validation() {
    let mut table = corpus().tables["plain"].clone();
    let fallback = corpus().tables["interview"]
        .routes
        .iter()
        .find(|r| r.notfound)
        .unwrap()
        .clone();
    table.routes.push(fallback.clone());
    let choices = "available routes: `landing`, `item`, `edit`, `archive`, `unicode`, `done`";
    for name in ["missing", "notfound"] {
        let error = table.path(name, &[]).unwrap_err();
        assert_eq!(error.code, "route-unknown");
        assert_eq!(
            error.message,
            format!("unknown path route `{name}`; {choices}")
        );
    }
    for (name, args, message) in [
        (
            "landing",
            vec!["extra"],
            "`landing` expects 0 path parameters, received 1",
        ),
        (
            "item",
            vec![],
            "`item` expects 1 path parameter (`item`), received 0",
        ),
        (
            "edit",
            vec![],
            "`edit` expects 1 path parameter (`item`), received 0",
        ),
        (
            "item",
            vec!["a", "b"],
            "`item` expects 1 path parameter (`item`), received 2",
        ),
        (
            "archive",
            vec!["2026"],
            "`archive` expects 2 path parameters (`year`, `item`), received 1",
        ),
    ] {
        let error = table.path(name, &args).unwrap_err();
        assert_eq!(error.code, "route-unknown");
        assert_eq!(error.message, message);
    }
    for value in ["", ".", ".."] {
        let error = table.path("item", &[value]).unwrap_err();
        assert_eq!(error.code, "route-unknown");
        assert_eq!(
            error.message,
            "a path parameter cannot be empty, `.` or `..`"
        );
    }
    assert_eq!(
        table.path("archive", &["2026", "é/a?b"]).unwrap(),
        "/archive/2026/%C3%A9%2Fa%3Fb"
    );
    for routes in [vec![], vec![fallback]] {
        let error = Table { routes }.path("missing", &[]).unwrap_err();
        assert_eq!(
            error.message,
            "unknown path route `missing`; no routes can be used with path"
        );
    }
}

/// LLP 1030.004 D2: a window opened at a route shows that screen alone. A
/// push and Back stay above it, a retarget makes another screen its only
/// visit, and another tab is seeded at its root when it is selected.
#[test]
fn a_rooted_launch_is_the_screen_alone() {
    let t = &corpus().tables["interview"];
    let r = success(Router::launch_rooted(t, "/prompt/7/write#draft"));
    assert_eq!(r.tab, "prompts");
    assert_eq!(r.tabs.len(), 1);
    let only = &r.tabs[0].stack;
    assert_eq!(only.len(), 1);
    assert_eq!(
        (only[0].name.as_str(), only[0].url.as_str()),
        ("write", "/prompt/7/write")
    );
    assert_eq!(
        (only[0].tab.as_str(), only[0].fragment.as_str()),
        ("prompts", "draft")
    );
    assert_eq!(only[0].params["question"], "7");
    assert_eq!((only[0].id, r.next), (0, 1));
    // No Back from the root.
    assert_eq!(success(back(t, r.clone())), r);

    // A push stacks above the root, and Back returns to it.
    let pushed = success(push(t, r.clone(), "/people/ann"));
    assert_eq!(depth(&pushed), 2);
    assert_eq!(success(back(t, pushed.clone())).tabs, r.tabs);
    // Reselecting the tab pops to the rooted screen, not the tab's route.
    assert_eq!(
        top(&success(select(t, pushed.clone(), "prompts")))
            .unwrap()
            .name,
        "write"
    );

    // At depth one the root takes another location of its own route only.
    let other = success(replace(t, r.clone(), "/prompt/8/write"));
    assert_eq!(
        (top(&other).unwrap().id, top(&other).unwrap().url.as_str()),
        (0, "/prompt/8/write")
    );
    let (same, refusal) = replace(t, r.clone(), "/prompts");
    assert_eq!(same, r);
    assert_eq!(
        refusal.unwrap().message,
        "replace cannot change the tab's root route"
    );

    // Another tab is seeded at its root, in declaration order, and kept.
    let home = success(select(t, pushed.clone(), "home"));
    assert_eq!(home.tab, "home");
    assert_eq!(
        home.tabs
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>(),
        ["home", "prompts"]
    );
    assert_eq!(
        top(&home).map(|e| (e.name.as_str(), e.id)),
        Some(("home", 2))
    );
    let back_again = success(select(t, home.clone(), "prompts"));
    assert_eq!(stack(&back_again), stack(&pushed));
    let messages = success(open(t, home, "/messages"));
    assert_eq!(
        messages
            .tabs
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>(),
        ["home", "prompts", "messages"]
    );
    let (unknown, refusal) = select(t, r.clone(), "settings");
    assert_eq!(
        (unknown, refusal.unwrap().message),
        (r.clone(), "unknown tab settings".into())
    );

    // A retarget makes the new location the only visit; ids go on.
    let moved = success(root(t, pushed, "/settings"));
    assert_eq!(moved.tab, "profile");
    assert_eq!(moved.tabs.len(), 1);
    assert_eq!(
        stack(&moved)
            .iter()
            .map(|e| (e.name.as_str(), e.id))
            .collect::<Vec<_>>(),
        [("settings", 2)]
    );
    assert_eq!(moved.next, 3);
    // `interview` absorbs an unmatched location in its notfound route.
    assert_eq!(
        top(&success(root(t, moved, "/no/such"))).unwrap().name,
        "notfound"
    );
    let plain = &corpus().tables["plain"];
    let r = success(Router::launch_rooted(plain, "/item/3"));
    let (same, refusal) = root(plain, r.clone(), "/no/such");
    assert_eq!(
        (same, refusal.unwrap().message),
        (r, "no route matches /no/such".into())
    );
}

/// Before any launch a select still names no tab.
#[test]
fn select_before_launch_is_refused() {
    let t = &corpus().tables["interview"];
    let (r, refusal) = select(t, Router::default(), "home");
    assert_eq!(
        (r, refusal.unwrap().message),
        (Router::default(), "unknown tab home".into())
    );
}
