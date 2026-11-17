//! `build --stdin` and `symbols --stdin` read an editor's unsaved buffer as the
//! named file's text: their output is what the same text saved there gives.

use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
};

struct Dir(PathBuf);
impl Drop for Dir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn run(dir: &Dir, args: &[&str], stdin: Option<&str>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_contract"))
        .args(args)
        .current_dir(&dir.0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    input.write_all(stdin.unwrap_or("").as_bytes()).unwrap();
    drop(input);
    child.wait_with_output().unwrap()
}

#[test]
fn unsaved_text_on_stdin_gives_what_the_same_text_saved_gives() {
    let dir = Dir(std::env::temp_dir().join(format!("exact-unsaved-{}", std::process::id())));
    std::fs::create_dir_all(dir.0.join("lib")).unwrap();
    let row = "component Row\n  props\n    label: string\n  view\n    text label\n";
    std::fs::write(dir.0.join("lib/row.contract"), row).unwrap();
    let saved = "component App\n  view\n    text \"saved\"\n";
    // One that uses its sibling and type-checks, and one with a refusal on its fourth line.
    let clean =
        "use Row from \"./lib/row.contract\"\ncomponent App\n  view\n    Row(label=\"hello\")\n";
    let refused =
        "use Row from \"./lib/row.contract\"\ncomponent App\n  view\n    Row(label=nope)\n";
    // The types app.ts imports stay the saved file's: unsaved text writes none
    // (a build of the saved file rewrites them).
    let declaration = dir.0.join("app.contract.d.ts");
    for (text, code) in [(clean, 0), (refused, 1)] {
        for args in [
            &["build", "app.contract", "--json"][..],
            // The text form quotes the refused line under its caret.
            &["build", "app.contract"],
            &["symbols", "app.contract"],
            &["symbols", "app.contract", "--name", "Row"],
        ] {
            std::fs::write(dir.0.join("app.contract"), saved).unwrap();
            // The text form says when it rewrote them, so it runs without them.
            let declared = args != ["build", "app.contract"];
            if declared {
                std::fs::write(&declaration, "// the saved file's\n").unwrap();
            } else if declaration.exists() {
                std::fs::remove_file(&declaration).unwrap();
            }
            let with_stdin = [args, &["--stdin"]].concat();
            let unsaved = run(&dir, &with_stdin, Some(text));
            assert_eq!(
                std::fs::read_to_string(dir.0.join("app.contract")).unwrap(),
                saved
            );
            if declared {
                assert_eq!(
                    std::fs::read_to_string(&declaration).unwrap(),
                    "// the saved file's\n"
                );
            }
            std::fs::write(dir.0.join("app.contract"), text).unwrap();
            let on_disk = run(&dir, args, None);
            assert_eq!(unsaved.status.code(), Some(code), "{args:?}: {unsaved:?}");
            assert_eq!(unsaved.status.code(), on_disk.status.code(), "{args:?}");
            assert_eq!(
                String::from_utf8_lossy(&unsaved.stdout),
                String::from_utf8_lossy(&on_disk.stdout),
                "{args:?}"
            );
            assert_eq!(
                String::from_utf8_lossy(&unsaved.stderr),
                String::from_utf8_lossy(&on_disk.stderr),
                "{args:?}"
            );
        }
        std::fs::write(dir.0.join("app.contract"), saved).unwrap();
        // The refusal names the path given and the unsaved text's range; the
        // sibling the text uses resolves beside that path.
        let built: serde_json::Value = serde_json::from_slice(
            &run(
                &dir,
                &["build", "app.contract", "--json", "--stdin"],
                Some(text),
            )
            .stdout,
        )
        .unwrap();
        let symbols: serde_json::Value = serde_json::from_slice(
            &run(&dir, &["symbols", "--stdin", "app.contract"], Some(text)).stdout,
        )
        .unwrap();
        let row = symbols["definitions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["kind"] == "component" && d["name"] == "Row")
            .expect("the used file's Row");
        assert!(row["file"].as_str().unwrap().ends_with("lib/row.contract"));
        if code == 0 {
            assert_eq!(built, serde_json::json!([]));
        } else {
            let at = |d: &serde_json::Value| {
                (
                    d["id"].clone(),
                    d["file"].clone(),
                    d["line"].clone(),
                    d["col"].clone(),
                    d["end_col"].clone(),
                )
            };
            let expected = (
                "type-unknown-name".into(),
                "app.contract".into(),
                4.into(),
                15.into(),
                19.into(),
            );
            assert_eq!(at(&built[0]), expected);
            assert_eq!(at(&symbols["errors"][0]), expected);
        }
    }
    // `--stdin` as `--name`'s value is the name, a literal ID's spelling.
    let named = run(
        &dir,
        &["symbols", "app.contract", "--name", "--stdin"],
        None,
    );
    assert_eq!(named.status.code(), Some(0), "{named:?}");
    // Once only, and never with --entry, which compiles app.json's widget files.
    for args in [
        &["symbols", "app.contract", "--stdin", "--stdin"][..],
        &["build", "app.contract", "--stdin", "--stdin"],
        &[
            "build",
            "app.contract",
            "--stdin",
            "--entry",
            "card",
            "-o",
            "card.plan",
        ],
    ] {
        assert_eq!(
            run(&dir, args, Some(clean)).status.code(),
            Some(2),
            "{args:?}"
        );
    }
}

/// `build --stdin-as`: the unsaved text is a file the app uses, so the app is
/// built from disk with that one file as the editor holds it.
#[test]
fn unsaved_text_of_a_used_file_is_built_with_its_app() {
    let dir = Dir(std::env::temp_dir().join(format!("exact-unsaved-as-{}", std::process::id())));
    std::fs::create_dir_all(dir.0.join("lib")).unwrap();
    let row = "component Row\n  props\n    label: string\n  view\n    text label\n";
    std::fs::write(dir.0.join("lib/row.contract"), row).unwrap();
    let app =
        "use Row from \"./lib/row.contract\"\ncomponent App\n  view\n    Row(label=\"hello\")\n";
    std::fs::write(dir.0.join("app.contract"), app).unwrap();
    let build = |text: &str| {
        let args = [
            "build",
            "app.contract",
            "--json",
            "--errors-only",
            "--stdin-as",
            "lib/row.contract",
        ];
        let out = run(&dir, &args, Some(text));
        let errors: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        (out.status.code(), errors)
    };
    // As saved, the app builds; with the row's unsaved text refused on its fifth line, the refusal is the row's.
    assert_eq!(build(row).0, Some(0));
    let (code, errors) = build(&row.replace("text label", "text nope"));
    assert_eq!(code, Some(1));
    let refusal = &errors.as_array().unwrap()[0];
    assert!(
        refusal["file"].as_str().unwrap().ends_with("row.contract") && refusal["line"] == 5,
        "{errors}"
    );
    // And an unsaved repair of a file refused on disk builds.
    std::fs::write(
        dir.0.join("lib/row.contract"),
        row.replace("text label", "text nope"),
    )
    .unwrap();
    assert_eq!(build(row).0, Some(0));
    // `--stdin` and `--stdin-as` are one or the other.
    let both = [
        "build",
        "app.contract",
        "--stdin",
        "--stdin-as",
        "lib/row.contract",
    ];
    assert_eq!(run(&dir, &both, Some(row)).status.code(), Some(2));
}
