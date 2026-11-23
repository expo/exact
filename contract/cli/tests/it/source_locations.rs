//! Original file/range diagnostics across imports and compiler passes.

#[cfg(unix)]
use std::path::Path;
use std::{path::PathBuf, process::Command};

struct App(PathBuf);
impl App {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("exact-source-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn write(&self, name: &str, source: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, source).unwrap();
        path
    }
    fn root(&self) -> PathBuf {
        self.write(
            "app.contract",
            "use Row from \"./lib/row.contract\"\ncomponent App\n  view\n    Row()\n",
        )
    }
    fn refuse(&self, source: &str, pass: &str, token: &str) -> contract::CompileError {
        let row = self
            .write("lib/row.contract", source)
            .canonicalize()
            .unwrap();
        let error = contract::compile_path(&self.root()).unwrap_err();
        assert_eq!(error.pass, pass, "{error}");
        assert_eq!(error.file.as_deref(), Some(row.as_path()), "{error}");
        let (line, text) = source
            .lines()
            .enumerate()
            .find(|(_, line)| line.contains(token))
            .unwrap();
        let col = text.find(token).unwrap() as u32 + 1;
        assert_eq!(
            (error.span.line, error.span.col),
            (line as u32 + 1, col),
            "{error}"
        );
        error
    }
}
impl Drop for App {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn conditional_action_bindings_name_the_callers_file_and_binding() {
    let app = App::new("conditional-action-binding");
    let caller = "use Child from \"./child.contract\"\ncomponent Caller\n  state n = 0\n  state flag = true\n  action a\n    n = 1\n  action b\n    n = 2\n  view\n    Child(tap=flag ? a : b)\n";
    for handler in ["tap", "tap()"] {
        for provided in [false, true] {
            let section = if provided { "inject" } else { "props" };
            app.write(
                "child.contract",
                &format!("component Child\n  {section}\n    tap: action\n  view\n    button \"go\" press={handler}\n"),
            );
            let caller = if provided {
                caller.replace(
                    "  view\n    Child(tap=flag ? a : b)",
                    "  provide\n    tap = flag ? a : b\n  view\n    Child()",
                )
            } else {
                caller.to_owned()
            };
            for imported in [false, true] {
                let path = app
                    .write("caller.contract", &caller)
                    .canonicalize()
                    .unwrap();
                let entry = if imported {
                    app.write(
                        "app.contract",
                        "use Caller from \"./caller.contract\"\ncomponent App\n  view\n    Caller()\n",
                    )
                } else {
                    path.clone()
                };
                let errors = contract::compile_path_all(&entry, false).err().unwrap();
                assert_eq!(errors.len(), 1, "{errors:?}");
                let error = &errors[0];
                let (id, col) = if provided {
                    ("type-action-provide-shape", 5)
                } else {
                    ("type-action-prop-shape", 11)
                };
                assert_eq!(error.id, id, "{error}");
                assert_eq!(error.file.as_deref(), Some(path.as_path()), "{error}");
                assert_eq!(
                    (error.span.line, error.span.col, error.span.end_col),
                    (10, col, col + 3),
                    "{error}"
                );
            }
        }
    }
}

fn assert_locations(entry: &std::path::Path, expected: &[(&str, (&str, u32, u32, u32))]) {
    let errors = contract::compile_path_all(entry, false)
        .err()
        .expect("the fixture should report an error");
    let mut found: Vec<_> = errors
        .iter()
        .map(|e| {
            let file = e
                .file
                .as_deref()
                .unwrap()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap();
            (
                file,
                (e.id.as_str(), e.span.line, e.span.col, e.span.end_col),
            )
        })
        .collect();
    found.sort();
    assert_eq!(found, expected, "{}", entry.display());
}

#[test]
fn a_forwarded_prop_names_its_supplier_and_keeps_the_consumers_own_error() {
    let app = App::new("forwarded-name");
    let root = app.write(
        "app.contract",
        "use Frame from \"./frame.contract\"\ncomponent App\n  view\n    Frame()\n",
    );
    app.write(
        "forward.contract",
        "use Inspector from \"./inspector.contract\"\ncomponent Forward\n  props\n    value: string\n  view\n    Inspector(title=value)\n",
    );
    app.write(
        "inspector.contract",
        "component Inspector\n  props\n    title: string\n  view\n    if title\n      text title\n",
    );
    for (state, caller) in [
        (
            "state label =",
            Some(("syntax-expected-expression", 3, 16, 16)),
        ),
        ("state other = 1", Some(("type-unknown-name", 5, 19, 24))),
        ("state label = \"ok\"", None),
    ] {
        app.write(
            "frame.contract",
            &format!("use Forward from \"./forward.contract\"\ncomponent Frame\n  {state}\n  view\n    Forward(value=label)\n"),
        );
        let mut expected: Vec<_> = caller
            .into_iter()
            .map(|at| ("frame.contract", at))
            .collect();
        expected.push(("inspector.contract", ("type-condition", 5, 8, 13)));
        assert_locations(&root, &expected);
    }
}

#[test]
fn an_invalid_provider_reports_its_own_file_at_root_and_below_a_shell() {
    let app = App::new("provided-name");
    let shell = app.write(
        "app.contract",
        "use Frame from \"./frame.contract\"\ncomponent App\n  view\n    Frame()\n",
    );
    let frame = app.write("frame.contract", "use Inspector from \"./inspector.contract\"\ncomponent Frame\n  provide\n    accent = 1\n    title = missing\n  view\n    Inspector()\n");
    app.write("inspector.contract", "component Inspector\n  inject\n    accent: string\n    title: string\n  view\n    if title\n      text title\n");
    for entry in [&shell, &frame] {
        assert_locations(
            entry,
            &[
                ("frame.contract", ("type-provide", 4, 5, 11)),
                ("frame.contract", ("type-unknown-name", 5, 13, 20)),
                ("inspector.contract", ("type-condition", 6, 8, 13)),
            ],
        );
    }
}

#[test]
fn imported_type_analyze_and_lower_errors_keep_the_offending_token_range() {
    let app = App::new("passes");
    for (source, pass, token) in [
        (
            "component Row\n  view\n    text missingName\n",
            "types",
            "missingName",
        ),
        (
            "component Row\n  view\n    view align-items=\"middle\"\n",
            "lower",
            "align-items",
        ),
        (
            "component Row\n  state on = true\n  view\n    view align-items=(on ? \"middle\" : \"center\")\n",
            "lower",
            "\"middle\"",
        ),
        (
            "routes nav\n  home \"/\"\ncomponent Row\n  view\n    text \"row\"\n",
            "analyze",
            "routes",
        ),
    ] {
        let error = app.refuse(source, pass, token);
        assert_eq!(error.span.end_col, error.span.col + token.len() as u32);
    }
}

#[test]
fn imported_lexer_parser_and_import_errors_name_their_own_file() {
    let app = App::new("syntax");
    app.refuse(
        "component Row\n  view\n    text \"unterminated\n",
        "syntax",
        "\"unterminated",
    );
    app.refuse("component Row\n  view\n    text )\n", "syntax", ")");
    app.refuse("component Row\n  view\n    text `é ${}`\n", "syntax", "}");
    app.refuse(
        "component Row\n  view\n    text `é ${   }`\n",
        "syntax",
        "}",
    );
    app.refuse("component Row\n  view\n    text `é ${@}`\n", "syntax", "@");
    app.refuse(
        "component Row\n  view\n    text `é ${   @ }`\n",
        "syntax",
        "@",
    );
    app.refuse(
        "component Row\n  view\n    text `é ${   ) }`\n",
        "syntax",
        ")",
    );
    app.refuse(
        "use Missing from \"./missing.contract\"\ncomponent Row\n  view\n    text \"row\"\n",
        "use",
        "use",
    );
}

#[test]
fn spaces_around_template_expressions_leave_the_plan_unchanged() {
    let source =
        |body: &str| format!("component App\n  state a = 1\n  view\n    column\n      {body}\n");
    for (spaced, compact) in [
        ("text `${ a }`", "text `${a}`"),
        ("text `${ a}`", "text `${a}`"),
        ("text `${a }`", "text `${a}`"),
        ("text `${   a   }`", "text `${a}`"),
        ("text `x ${ a + 1 } y`", "text `x ${a + 1} y`"),
        (
            r#"text `${ a > 0 ? "pos" : "neg" }`"#,
            r#"text `${a > 0 ? "pos" : "neg"}`"#,
        ),
        ("column testId=`row-${ a }`", "column testId=`row-${a}`"),
        ("text `é ${ a } ${ a + 1 }`", "text `é ${a} ${a + 1}`"),
        ("text `${ `nested ${ a }` }`", "text `${`nested ${a}`}`"),
        (
            r#"text `${ match some(a) { case some(n) => n, case none => 0 } }`"#,
            r#"text `${match some(a) { case some(n) => n, case none => 0 }}`"#,
        ),
    ] {
        let plan = contract::compile(&source(spaced)).unwrap_or_else(|e| panic!("{spaced}: {e}"));
        assert_eq!(
            plan.encode(),
            contract::compile(&source(compact)).unwrap().encode(),
            "{spaced}"
        );
    }
}

#[test]
fn transitive_imports_and_repeated_template_interpolations_keep_byte_columns() {
    let app = App::new("templates");
    let root = app.write(
        "app.contract",
        "use Outer from \"./lib/outer.contract\"\ncomponent App\n  view\n    Outer()\n",
    );
    app.write(
        "lib/outer.contract",
        "use Row from \"./row.contract\"\ncomponent Outer\n  view\n    Row()\n",
    );
    for line in [
        "    text `é ${1} xx ${missingName}`",
        "    text `é ${`nested ${missingName}`}`",
        "    text `é ${ 1 } xx ${   missingName }`",
        "    text `é ${ `nested ${   missingName }` }`",
    ] {
        let row = app
            .write(
                "lib/row.contract",
                &format!("component Row\n  view\n{line}\n"),
            )
            .canonicalize()
            .unwrap();
        let error = contract::compile_path(&root).unwrap_err();
        assert_eq!(error.file.as_deref(), Some(row.as_path()), "{error}");
        assert_eq!(error.span.line, 3);
        let col = line.find("missingName").unwrap() as u32 + 1;
        assert_eq!(
            (error.span.col, error.span.end_col),
            (col, col + 11),
            "{error}"
        );
    }
}

#[test]
fn provided_root_source_is_not_reread_and_root_errors_keep_the_requested_path() {
    let app = App::new("snapshot");
    let root = app.root();
    app.write(
        "lib/row.contract",
        "component Row\n  view\n    text \"row\"\n",
    );
    let snapshot = "component App\n  view\n    text missingSnapshot\n";
    let error = contract::compile_path_source(&root, snapshot).unwrap_err();
    assert_eq!(error.file.as_deref(), Some(root.as_path()));
    assert!(error.message.contains("missingSnapshot"), "{error}");
    let error = contract::compile(snapshot).unwrap_err();
    assert_eq!(error.file, None);
    assert!(!error.to_string().contains("app.contract"));
    let missing = app.0.join("absent.contract");
    let error = contract::compile_path(&missing).unwrap_err();
    assert_eq!(error.file.as_deref(), Some(missing.as_path()));
    assert_eq!(error.to_string().matches("absent.contract").count(), 1);
}

#[test]
fn cli_build_and_types_print_the_imported_filename_once() {
    let app = App::new("cli");
    let root = app.root();
    let row = app
        .write(
            "lib/row.contract",
            "component Row\n  view\n    text missingName\n",
        )
        .canonicalize()
        .unwrap();
    for subcommand in ["build", "types"] {
        let result = Command::new(env!("CARGO_BIN_EXE_contract"))
            .arg(subcommand)
            .arg(&root)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(1));
        let stderr = String::from_utf8(result.stderr).unwrap();
        assert!(
            stderr.starts_with(&format!("{}:3:10 [", row.display())),
            "{stderr}"
        );
        assert!(!stderr.contains("app.contract"), "{stderr}");
    }
}

fn diamond(app: &App) -> PathBuf {
    app.write("row.contract", "component Row\n  view\n    text \"row\"\n");
    app.write(
        "left.contract",
        "use Row from \"./row.contract\"\ncomponent Left\n  view\n    Row()\n",
    );
    app.write(
        "right.contract",
        "use Row from \"./row.contract\"\ncomponent Right\n  view\n    Row()\n",
    );
    app.write("app.contract", "use Left from \"./left.contract\"\nuse Right from \"./right.contract\"\ncomponent App\n  view\n    view\n      Left()\n      Right()\n")
}

#[test]
fn diamonds_reuse_files_without_losing_name_checks_or_accepting_cycles() {
    let app = App::new("diamond");
    let root = diamond(&app);
    contract::compile_path(&root).unwrap();
    // This lookup reaches an already completed file, but its requested name is still checked.
    app.write(
        "right.contract",
        "use Absent from \"./row.contract\"\ncomponent Right\n  view\n    text \"right\"\n",
    );
    let error = contract::compile_path(&root).unwrap_err();
    assert_eq!(error.id, "contract-use-unknown");
    assert_eq!(
        error.file.as_deref(),
        Some(
            app.0
                .join("right.contract")
                .canonicalize()
                .unwrap()
                .as_path()
        )
    );
    app.write(
        "row.contract",
        "use Left from \"./left.contract\"\ncomponent Row\n  view\n    text \"row\"\n",
    );
    let error = contract::compile_path(&root).unwrap_err();
    assert_eq!(error.id, "contract-use-cycle");
    assert_eq!(
        error.file.as_deref(),
        Some(app.0.join("row.contract").canonicalize().unwrap().as_path())
    );
}

#[test]
fn repeated_imports_check_the_dependency_exports_at_each_use_site() {
    let app = App::new("repeated");
    app.write("leaf.contract", "fn leaf(): number = 7\n");
    app.write(
        "lib.contract",
        "use leaf from \"./leaf.contract\"\nfn local(): number = leaf()\n",
    );
    let root_source = "use local from \"./lib.contract\"\nuse leaf from \"./lib.contract\"\ncomponent App\n  view\n    text `${local() + leaf()}`\n";
    let root = app.write("app.contract", root_source);
    let repeated = contract::compile_path(&root).unwrap();
    let once = contract::compile_path_source(
        &root,
        &root_source.replace(
            "use local from \"./lib.contract\"\nuse leaf from",
            "use local, leaf from",
        ),
    )
    .unwrap();
    assert_eq!(repeated.encode(), once.encode());

    // An existing declaration in the importer is not an export of the dependency.
    let source = root_source.replace("use leaf from", "use App from");
    let error = contract::compile_path_source(&root, &source).unwrap_err();
    assert_eq!(error.id, "contract-use-unknown");
    assert_eq!(error.file.as_deref(), Some(root.as_path()));
    assert_eq!(error.span.line, 2);

    // A conflicting intervening dependency still fails at its own use site.
    app.write("conflict.contract", "fn leaf(): number = 8\n");
    let source = root_source.replace(
        "use leaf from",
        "use leaf from \"./conflict.contract\"\nuse leaf from",
    );
    let error = contract::compile_path_source(&root, &source).unwrap_err();
    assert_eq!(error.id, "contract-use-duplicate");
    assert_eq!(error.span.line, 3);
}

#[test]
fn repeated_names_along_an_import_chain_preserve_the_plan() {
    let app = App::new("repeated-chain");
    for depth in (0..16).rev() {
        let imports = if depth == 15 {
            String::new()
        } else {
            let next = depth + 1;
            format!("use a{next} from \"./part{next}.contract\"\nuse b{next} from \"./part{next}.contract\"\n")
        };
        app.write(
            &format!("part{depth}.contract"),
            &format!("{imports}fn a{depth}(): number = 0\nfn b{depth}(): number = 1\n"),
        );
    }
    let root = app.write("app.contract", "use a0 from \"./part0.contract\"\nuse b0 from \"./part0.contract\"\ncomponent App\n  view\n    text `${a0() + b0()}`\n");
    let imported = contract::compile_path(&root).unwrap();
    let flat = contract::compile("fn a0(): number = 0\nfn b0(): number = 1\ncomponent App\n  view\n    text `${a0() + b0()}`\n").unwrap();
    assert_eq!(imported.encode(), flat.encode());
}

#[test]
fn two_files_declarations_are_two_however_alike() {
    // A declaration's identity is its file and place, not its text (LLP
    // 1091 D7): one name from two files is refused even when they agree.
    let app = App::new("duplicate");
    let root = app.write("app.contract", "use Row from \"./one.contract\"\nuse Row from \"./two.contract\"\ncomponent App\n  view\n    Row()\n");
    app.write("one.contract", "component Row\n  view\n    view width=1\n");
    app.write("two.contract", "component Row\n  view\n    view width=1\n");
    let error = contract::compile_path(&root).unwrap_err();
    assert_eq!(error.id, "contract-use-duplicate");
    assert_eq!(error.span.line, 2);
    // Splitter rough 1: the refusal names both files, not "different".
    assert!(
        error.message.contains("declared in two.contract")
            && error.message.contains("the one declared in one.contract")
            && error.message.contains("however alike"),
        "{}",
        error.message
    );
    // Renamed, both load: the second is `Row__two` to the passes after.
    app.write("app.contract", "use Row from \"./one.contract\"\nuse Row as Other from \"./two.contract\"\ncomponent App\n  view\n    column\n      Row()\n      Other()\n");
    contract::compile_path(&root).unwrap();
}

#[cfg(unix)]
#[test]
fn symlink_aliases_share_identity_and_still_detect_cycles() {
    let app = App::new("alias");
    let root = diamond(&app);
    std::os::unix::fs::symlink(Path::new("row.contract"), app.0.join("alias.contract")).unwrap();
    app.write(
        "right.contract",
        "use Row from \"./alias.contract\"\ncomponent Right\n  view\n    Row()\n",
    );
    contract::compile_path(&root).unwrap();
    let repeated_alias = "use Row from \"./row.contract\"\nuse Row from \"./alias.contract\"\ncomponent App\n  view\n    Row()\n";
    contract::compile_path_source(&root, repeated_alias).unwrap();
    let error = contract::compile_path_source(
        &root,
        &repeated_alias.replace("use Row from \"./alias", "use Absent from \"./alias"),
    )
    .unwrap_err();
    assert_eq!(error.id, "contract-use-unknown");
    assert_eq!(error.span.line, 2);
    app.write(
        "row.contract",
        "use Row from \"./alias.contract\"\ncomponent Row\n  view\n    text \"row\"\n",
    );
    assert_eq!(
        contract::compile_path(&root).unwrap_err().id,
        "contract-use-cycle"
    );
}

#[test]
fn overlapping_import_subgraphs_keep_transitive_exports_and_source_identity() {
    let app = App::new("overlapping-dag");
    for index in 0..28 {
        let imports: String = [index + 1, index + 2]
            .into_iter()
            .filter(|next| *next < 28)
            .map(|next| format!("use value{next} from \"./part{next}.contract\"\n"))
            .collect();
        app.write(
            &format!("part{index}.contract"),
            &format!("{imports}fn value{index}(): number = {index}\n"),
        );
    }
    let source = "use value0 from \"./part0.contract\"\nuse value27 from \"./part26.contract\"\ncomponent App\n  view\n    text `${value0() + value27()}`\n";
    let root = app.write("app.contract", source);
    let imported = contract::compile_path(&root).unwrap();
    let flat = contract::compile("fn value0(): number = 0\nfn value27(): number = 27\ncomponent App\n  view\n    text `${value0() + value27()}`\n").unwrap();
    assert_eq!(imported.encode(), flat.encode());
    let symbols: serde_json::Value =
        serde_json::from_str(&contract::symbols_json(&root, None).unwrap()).unwrap();
    let leaves: Vec<_> = symbols["definitions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|definition| definition["name"] == "value27")
        .collect();
    assert_eq!(leaves.len(), 1);
    assert_eq!(
        leaves[0]["file"],
        app.0
            .join("part27.contract")
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
    );
    let refused = source.replace("use value27 from", "use App from");
    let error = contract::compile_path_source(&root, &refused).unwrap_err();
    assert_eq!(error.id, "contract-use-unknown");
    assert_eq!(error.span.line, 2);
    assert_eq!(error.file.as_deref(), Some(root.as_path()));
}
