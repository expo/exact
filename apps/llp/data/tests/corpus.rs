//! The corpus this reader exists for is the private one cloned at the
//! repository's `llp/`, so that is what these check against when it is there;
//! without it, the tests that read it pass without checking. @ref LLP 1033

use llp_data::link_references;
use markdown_parse::{plain, Run};
use std::collections::BTreeSet;

fn llp() -> Option<std::path::PathBuf> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../llp");
    if dir.is_dir() {
        return Some(dir);
    }
    eprintln!("no llp/ corpus at the repository root: nothing to check");
    None
}

#[test]
fn simultaneous_page_and_index_requests_read_one_corpus_snapshot() {
    use exact_plan::Value;
    use exact_runner::{Answer, DataSource, Store};
    let root = std::env::temp_dir().join(format!("llp-shared-corpus-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("1000-one.explainer.md");
    std::fs::write(&path, "# First snapshot\n\nOriginal body.").unwrap();
    let mut data = llp_data::Llp::new();
    let mut store = Store::new("", []);
    let args = [Value::str(&root.to_string_lossy()), Value::str("")];
    let mut workers = Vec::new();
    for source in ["page", "index"] {
        let Answer::Later(request) = data.answer(&mut store, source, &args).unwrap() else {
            panic!("the corpus should load on a worker");
        };
        workers.push(data.continuation(request.continuation.unwrap()).unwrap());
    }
    let first = workers.remove(0)();
    data.parse(&mut store, "page", &args, first).unwrap();
    // The other waiter must receive the same snapshot even when it runs
    // after the first resource has consumed its answer.
    std::fs::write(&path, "# Replaced snapshot\n\nNew body.").unwrap();
    let second = workers.remove(0)();
    let Answer::Now(index) = data.parse(&mut store, "index", &args, second).unwrap() else {
        panic!("the corpus is ready");
    };
    assert!(format!("{index:?}").contains("First snapshot"));
    assert!(!format!("{index:?}").contains("Replaced snapshot"));
    std::fs::remove_dir_all(root).unwrap();
}

/// The sidebar's rows over the corpus at `root`, `(number, depth)`.
fn index_rows(root: &std::path::Path, query: &str, newest: bool) -> Vec<(String, f64)> {
    use exact_plan::Value;
    use exact_runner::{Answer, DataSource, Store};
    let mut data = llp_data::Llp::new();
    let mut store = Store::new("", []);
    let args = [
        Value::str(&root.to_string_lossy()),
        Value::str(query),
        Value::str(""),
        Value::Bool(newest),
    ];
    let Answer::Later(request) = data.answer(&mut store, "index", &args).unwrap() else {
        panic!("the corpus should load on a worker");
    };
    let loaded = data.continuation(request.continuation.unwrap()).unwrap()();
    let Answer::Now(Value::Record(index)) = data.parse(&mut store, "index", &args, loaded).unwrap()
    else {
        panic!("the corpus is ready");
    };
    let Value::List(rows) = &index[4] else {
        panic!("the index lists its rows");
    };
    rows.iter()
        .map(|row| {
            let Value::Record(row) = row else {
                panic!("a row is a record")
            };
            (
                row[1].as_str().unwrap().to_string(),
                row[6].as_number().unwrap(),
            )
        })
        .collect()
}

#[test]
fn newest_first_keeps_each_sub_document_under_its_parent() {
    let root = std::env::temp_dir().join(format!("llp-newest-corpus-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, title) in [
        ("1030-a.plan.md", "A"),
        ("1030.000-a-one.plan.md", "A one"),
        ("1030.001-a-two.plan.md", "A two"),
        ("1031-b.plan.md", "B"),
        ("1031.000-b-one.plan.md", "B one"),
    ] {
        std::fs::write(root.join(name), format!("# {title}\n\nText.")).unwrap();
    }
    let rows = |query: &str| {
        index_rows(&root, query, true)
            .into_iter()
            .map(|(number, depth)| format!("{}{number}", " ".repeat(depth as usize)))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        rows(""),
        ["1031", " 1031.000", "1030", " 1030.000", " 1030.001"]
    );
    // A sub-document shown without its parent stays with its own number.
    assert_eq!(rows("one"), [" 1031.000", " 1030.000"]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn corpus_text_keeps_document_boundaries_and_survives_its_index() {
    let root = std::env::temp_dir().join(format!("llp-text-corpus-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let first = "# LLP 0001: First\n\nİstanbul Σίσυφος 界 é needle NEEDLE.";
    let second = "# LLP 0002: Second\n\nOther document.";
    std::fs::write(root.join("0001-first.rfc.md"), first).unwrap();
    std::fs::write(root.join("0002-second.rfc.md"), second).unwrap();
    std::fs::write(root.join("0003-invalid.rfc.md"), [0xff]).unwrap();
    std::fs::write(root.join("0004-empty.rfc.md"), "").unwrap();
    let entries = llp_data::read_index(&root).unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(&*entries[0].body, first);
    assert_eq!(&*entries[1].body, second);
    assert_eq!(&*entries[2].body, "");
    assert_eq!(entries[0].search("needle"), Some(2));
    assert_eq!(entries[1].search("needle"), None);
    assert_eq!(entries[0].search("i\u{307}stanbul"), Some(1));
    assert!(!format!("{:?}", entries[1].body).contains("First"));
    let kept = entries[0].clone();
    let reread = llp_data::read_index(&root).unwrap();
    assert_eq!(kept, reread[0]);
    drop(entries);
    drop(reread);
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(&*kept.body, first);
}

#[test]
fn the_index_is_this_repositorys_corpus_in_number_order() {
    let Some(llp) = llp() else { return };
    let entries = llp_data::read_index(&llp).expect("llp/");
    assert!(entries.len() > 30, "found {} documents", entries.len());

    let root = entries
        .iter()
        .find(|e| e.number == "1000")
        .expect("the root explainer");
    assert_eq!(root.kind, "explainer");
    assert_eq!(root.depth, 0);
    assert!(
        root.title.starts_with("Exact2 Project Root"),
        "{}",
        root.title
    );
    assert_eq!(root.overlay, "foundation");

    // A dotted number is a sub-document, and sorts under its parent.
    let sub = entries.iter().position(|e| e.number == "1030.000").unwrap();
    let parent = entries.iter().position(|e| e.number == "1030").unwrap();
    assert_eq!(entries[sub].depth, 1);
    assert!(parent < sub, "1030.000 sorts after 1030");
    assert!(entries[sub..].iter().all(|e| e.number != "1029"));

    // The working set is the `current/` overlay, and it is declared.
    let current: BTreeSet<_> = entries
        .iter()
        .filter(|e| e.overlay == "current")
        .map(|e| e.path.file_name().unwrap().to_owned())
        .collect();
    let declared: BTreeSet<_> = std::fs::read_dir(llp.join("current"))
        .expect("llp/current/")
        .map(|e| e.expect("working-set entry").file_name())
        .collect();
    assert_eq!(current, declared, "the reader indexes every declared link");
    assert!(
        current.len() <= 16,
        "rules/RULES.md caps the working set at 16, found {}",
        current.len()
    );
}

#[test]
fn search_looks_through_every_documents_text() {
    let Some(llp) = llp() else { return };
    let entries = llp_data::read_index(&llp).expect("llp/");
    let kernel = entries.iter().find(|e| e.number == "1001").unwrap();
    assert!(
        kernel.search("taffy").is_some_and(|hits| hits > 1),
        "the kernel spec names Taffy more than once"
    );
    assert_eq!(
        kernel.search("a phrase that appears in no document at all"),
        None
    );
    // A number and a status match too.
    assert!(kernel.search("1001").is_some());
}

#[test]
fn search_counts_body_hits_and_keeps_metadata_only_matches() {
    let root = std::env::temp_dir().join(format!("llp-search-corpus-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("0007-zeta.rfc.md"),
        "# ΖΕΤΑ\n\n**Status:** Reviewed\n\nΟΣ ΟΣΑ Σ ΣΣ İ AAaaa",
    )
    .unwrap();
    let entries = llp_data::read_index(&root).unwrap();
    let entry = &entries[0];
    for (query, expected) in [
        ("", Some(0)),
        ("0007", Some(0)),
        ("rfc", Some(0)),
        ("reviewed", Some(1)),
        ("ζετα", Some(1)),
        ("ος", Some(1)),
        ("οσ", Some(1)),
        ("i\u{307}", Some(1)),
        ("aa", Some(2)),
        ("absent", None),
    ] {
        assert_eq!(entry.search(query), expected, "query {query:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_reference_in_prose_becomes_a_link_and_an_unknown_one_does_not() {
    let runs = vec![Run::text("See LLP 1030 D7 and LLP 9999 for the rest.")];
    let linked = link_references(runs, &|number| {
        (number == "1030").then(|| "/llp/1030-delivery-unified.rfc.md".to_string())
    });
    assert_eq!(plain(&linked), "See LLP 1030 D7 and LLP 9999 for the rest.");
    let link: Vec<&Run> = linked.iter().filter(|r| !r.href.is_empty()).collect();
    assert_eq!(link.len(), 1);
    assert_eq!(link[0].text, "LLP 1030");
    assert_eq!(link[0].href, "/llp/1030-delivery-unified.rfc.md");
}

#[test]
fn a_dotted_reference_resolves_to_the_sub_document() {
    let linked = link_references(vec![Run::text("as LLP 1030.000 says")], &|number| {
        (number == "1030.000").then(|| "/llp/1030.000.rfc.md".to_string())
    });
    let link = linked.iter().find(|r| !r.href.is_empty()).unwrap();
    assert_eq!(link.text, "LLP 1030.000");
}

#[test]
fn code_and_existing_links_are_left_alone() {
    let code = Run {
        text: "LLP 1030".into(),
        code: true,
        ..Run::default()
    };
    let already = Run {
        text: "LLP 1030".into(),
        href: "https://example.com".into(),
        ..Run::default()
    };
    let linked = link_references(vec![code, already], &|_| Some("/somewhere".to_string()));
    assert!(linked[0].href.is_empty(), "a code span is code");
    assert_eq!(linked[1].href, "https://example.com");
}

/// A citation names a document three or four levels deep by its whole
/// number: the longest prefix the corpus knows wins, and a sentence's final
/// dot is never taken.
#[test]
fn a_deep_citation_links_the_document_it_names() {
    let find = |n: &str| match n {
        "1012.001" => Some("1012.001-agentic-usability-review.research.md".to_string()),
        "1012.001.000" => Some("1012.001.000-trustworthy-agent-drives.rfc.md".to_string()),
        "1053.000.000.001" => Some("1053.000.000.001-glass-morph.rfc.md".to_string()),
        _ => None,
    };
    let runs = link_references(
        vec![Run::text(
            "A stale build (LLP 1012.001.000). See LLP 1053.000.000.001, LLP 1012.001.",
        )],
        &find,
    );
    let links: Vec<(&str, &str)> = runs
        .iter()
        .filter(|r| !r.href.is_empty())
        .map(|r| (r.text.as_str(), r.href.as_str()))
        .collect();
    assert_eq!(
        links,
        [
            (
                "LLP 1012.001.000",
                "1012.001.000-trustworthy-agent-drives.rfc.md"
            ),
            (
                "LLP 1053.000.000.001",
                "1053.000.000.001-glass-morph.rfc.md"
            ),
            (
                "LLP 1012.001",
                "1012.001-agentic-usability-review.research.md"
            ),
        ]
    );
}

/// What `source` answers for `path`: the path it shows and its message.
fn shown(
    data: &mut llp_data::Llp,
    source: &str,
    path: &std::path::Path,
) -> (std::path::PathBuf, String) {
    use exact_plan::Value;
    use exact_runner::{Answer, DataSource, Store};
    let mut store = Store::new("", []);
    let args = [Value::str(&path.to_string_lossy()), Value::str("")];
    let answer = match data.answer(&mut store, source, &args).unwrap() {
        Answer::Later(request) => {
            let loaded = data.continuation(request.continuation.unwrap()).unwrap()();
            data.parse(&mut store, source, &args, loaded).unwrap()
        }
        now => now,
    };
    let Answer::Now(Value::Record(value)) = answer else {
        panic!("{source} answers a record");
    };
    let message = value[6].as_str().unwrap().to_string();
    assert_eq!(value[7], Value::Bool(!message.is_empty()));
    (value[0].as_str().unwrap().into(), message)
}

#[test]
fn a_missing_document_fails_and_keeps_the_open_one() {
    let root = std::env::temp_dir().join(format!("llp-missing-corpus-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let (first, second) = (root.join("1000-first.md"), root.join("1001-second.md"));
    std::fs::write(&first, "# First\n\nText.").unwrap();
    std::fs::write(&second, "# Second\n\nText.").unwrap();
    let missing = root.join("9999-missing.md");
    for source in ["document", "page"] {
        let mut data = llp_data::Llp::new();
        // A directory opens its first document.
        assert_eq!(
            shown(&mut data, source, &root),
            (first.clone(), String::new())
        );
        assert_eq!(
            shown(&mut data, source, &second),
            (second.clone(), String::new())
        );
        let (path, message) = shown(&mut data, source, &missing);
        assert_eq!(path, second, "{source}");
        assert!(message.contains("9999-missing.md"), "{source}: {message:?}");
    }
    // A document the index lists, deleted after the index was read.
    let mut data = llp_data::Llp::new();
    assert_eq!(
        shown(&mut data, "document", &second),
        (second.clone(), String::new())
    );
    std::fs::remove_file(&first).unwrap();
    let (path, message) = shown(&mut data, "document", &first);
    assert_eq!(path, second);
    assert!(message.contains("1000-first.md"), "{message:?}");
    std::fs::remove_dir_all(root).unwrap();
}
