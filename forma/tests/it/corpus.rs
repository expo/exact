//! Golden IR and refusals for the corpus under `tests/corpus/` (LLP 1121
//! §5's "golden IR and folded tables"). A `.forma` that checks has a
//! `.json` beside it; one that is refused has a `.err`, one diagnostic a
//! line. `BLESS=1 cargo test -p exact-forma` rewrites them.

use crate::common::vocab;
use exact_forma::check;
use std::path::Path;

#[test]
fn family_stacks_are_refused_at_the_design_value() {
    let vocab = vocab();
    for family in [
        "\"Comic Sans MS\", cursive",
        "Inter, fantasy",
        "Inter, math",
        "Inter, emoji",
        "Inter, fangsong",
        "\"\", system-ui",
        "''",
        "\"a;b\", system-ui",
        "\"a{b}\", system-ui",
        "\"a\\b\", system-ui",
        "Inter, inherit",
        "Inter, initial",
        "Inter, unset",
        "Inter, revert",
        "Inter, revert-layer",
        "default",
        "-1font",
        "serif extra",
        "\"Inter\" \"Extra\"",
        r#"'"Missing"', monospace"#,
        r#"'"Missing', monospace"#,
        r#"'Missing"', monospace"#,
        r#"'Say "hi"', serif"#,
        r#"'"monospace"', monospace"#,
        &vec!["Inter"; 65].join(", "),
    ] {
        for (kind, value) in [
            ("family", family.to_string()),
            ("type", format!("16px/1.4 {family}")),
        ] {
            let source = format!("forma t\n\ntheme\n  {kind}.text = {value}\n");
            let errors = check(&source, &vocab).expect_err(&source);
            assert!(
                errors.iter().any(|e| e.line == 4 && e.col > 0),
                "{errors:?}"
            );
        }
    }
    let source = "forma t\n\naxes\n  density = regular | compact\n\ntheme\n  family.text = system-ui, sans-serif\n    when density=compact: Inter, fantasy\n  type.body = 16px/1.4 family.text\n";
    let errors = check(source, &vocab).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.line == 8 && e.message.contains("fantasy")),
        "{errors:?}"
    );

    for family in [
        "\"O'Brien\", serif",
        r#""'Missing'", serif"#,
        r#"'Say "hi" again', serif"#,
        "\"cursive\", 'inherit', \"monospace\"",
        "\"Acme, Display\", Times New Roman, serif",
        "-apple-system, SYSTEM-UI, ui-rounded",
        "serif\u{a0}extra, system-ui",
        &vec!["Inter"; 64].join(", "),
    ] {
        let source = format!(
            "forma t\n\ntheme\n  family.text = {family}\n  type.body = 16px/1.4 family.text\n"
        );
        let folding = check(&source, &vocab).unwrap_or_else(|e| panic!("{source}: {e:?}"));
        exact_forma::family_names(&folding.tokens[0].values[0].css()).unwrap();
    }
}

#[test]
fn corpus() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let bless = std::env::var_os("BLESS").is_some();
    let vocab = vocab();
    let mut failures = Vec::new();
    let mut count = 0;
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    for path in files
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "forma"))
    {
        count += 1;
        let src = std::fs::read_to_string(path).unwrap();
        let (got, ext, other) = match check(&src, &vocab) {
            Ok(folding) => (folding.ir(), "json", "err"),
            Err(errors) => (
                errors.iter().map(|e| format!("{e}\n")).collect::<String>(),
                "err",
                "json",
            ),
        };
        let golden = path.with_extension(ext);
        if bless {
            std::fs::write(&golden, &got).unwrap();
            let _ = std::fs::remove_file(path.with_extension(other));
            continue;
        }
        match std::fs::read_to_string(&golden) {
            Ok(want) if want == got => {}
            Ok(want) => failures.push(format!(
                "{}:\n--- want\n{want}--- got\n{got}",
                golden.display()
            )),
            Err(_) => failures.push(format!("{} is missing; got\n{got}", golden.display())),
        }
    }
    assert!(count > 10, "the corpus is {count} files");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
