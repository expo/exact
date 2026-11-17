//! A document named relative to the current directory opens in its corpus:
//! a bare file name, `./` and the name, a file in a relative folder, and the
//! same file by its absolute path. Its own test binary, since it sets the
//! process's current directory.

use exact_plan::Value;
use exact_runner::{Answer, DataSource, Store};

/// The `page` value for `asked`, its corpus read on the worker as the app's
/// would be.
fn page(asked: &str) -> String {
    let mut data = llp_data::Llp::new();
    let mut store = Store::new("", []);
    let args = [Value::str(asked), Value::str("")];
    let answer = match data.answer(&mut store, "page", &args).unwrap() {
        Answer::Later(request) => {
            let outcome = data.continuation(request.continuation.unwrap()).unwrap()();
            data.parse(&mut store, "page", &args, outcome).unwrap()
        }
        now => now,
    };
    let Answer::Now(value) = answer else {
        panic!("{asked}: the corpus is read")
    };
    format!("{value:?}")
}

#[test]
fn a_document_named_relative_to_the_current_directory_opens_in_its_corpus() {
    let root = std::env::temp_dir().join(format!("llp-relative-{}", std::process::id()));
    std::fs::create_dir_all(root.join("sub")).unwrap();
    for dir in [root.clone(), root.join("sub")] {
        std::fs::write(dir.join("1000-valid.md"), "# Valid title\n\nThe body.").unwrap();
        std::fs::write(dir.join("1001-next.md"), "# Next title\n\nMore.").unwrap();
    }
    std::env::set_current_dir(&root).unwrap();
    let absolute = root.join("1000-valid.md");
    for asked in [
        "1000-valid.md",
        "./1000-valid.md",
        "sub/1000-valid.md",
        &absolute.to_string_lossy(),
    ] {
        let page = page(asked);
        assert!(page.contains("Valid title"), "{asked}: {page}");
        // Its corpus is read: the next document is its neighbour.
        assert!(page.contains("1001-next.md"), "{asked}: {page}");
    }
    std::fs::remove_dir_all(&root).unwrap();
}
