//! A dragged file's name (LLP 1094.000 D14): `names=` beside `files=` binds
//! one value the runner decodes, admitting each file with its name into
//! `dataTransferFiles` and `dataTransferNames`, and the compiler refuses what
//! it can tell from literals.

use crate::source::{boxed, Source};
use exact_kernel::{Kernel, PropId};
use exact_plan::Value;
use exact_runner::page::Platform;
use exact_runner::{DataError, DataSource, Event, Runner};

struct Grants;

impl DataSource for Grants {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
    fn grants(&self) -> &str {
        "fs.read app:/data\n"
    }
}

fn boot(src: &str, platform: Platform, kernel: Kernel) -> Runner<Source> {
    let plan = contract::bake(contract::compile(src).unwrap(), boxed(Grants)).unwrap();
    platform.boot(|| Runner::boot(plan, boxed(Grants), kernel, Default::default(), "/").unwrap())
}

fn view(r: &Runner<Source>, test_id: &str) -> u32 {
    let key = r.kernel().find_by_test_id(test_id)[0];
    r.kernel().node_by_key(key).unwrap().id
}

/// `(files, names)` as the host reads them.
fn held(r: &Runner<Source>, test_id: &str) -> (String, String) {
    let key = r.kernel().find_by_test_id(test_id)[0];
    let props = r.kernel().node_by_key(key).unwrap().props;
    let text = |p| props.str(p).unwrap_or_default().to_string();
    assert_eq!(
        props.str(PropId::DataTransferEntries),
        None,
        "the encoded value reaches no host"
    );
    (
        text(PropId::DataTransferFiles),
        text(PropId::DataTransferNames),
    )
}

fn refusals(r: &Runner<Source>) -> Vec<String> {
    r.journal()
        .filter_map(|l| l.find("drag: refused").map(|at| l[at..].to_string()))
        .collect()
}

/// A component whose node `card` carries `files` named by `names`, both
/// state, and a button `swap` that sets `names` to `then`.
fn card(files: &str, names: &str, then: &str) -> String {
    format!(
        "component A\n  state files: string[] = {files}\n  state names: string[] = {names}\n  action swap\n    names = {then}\n  view\n    column\n      column draggable=true dataTransfer=DataTransfer(files=files, names=names) testId=\"card\"\n        text \"a\"\n      button press=swap testId=\"swap\"\n        text \"b\"\n"
    )
}

#[test]
fn a_name_goes_with_its_file_through_admission() {
    // The refused first file takes its name; an empty name keeps the file's own; a bound name breaking a rule
    // as written (`Fuji.`) keeps the file's own and is journaled; a name with no type of its own gets the file's.
    let src = card(
        r#"["app:/data/../x.pdf", "app:/data/a.jpg", "app:/data/b.jpg", "app:/data/c.jpg", "app:/data/d.jpeg"]"#,
        r#"["Refused", "", "Mount Fuji", "Fuji.", "Photo.jpg"]"#,
        "[]",
    );
    let r = boot(&src, Platform::Ios, Kernel::with_monospace());
    assert_eq!(
        held(&r, "card"),
        (
            "app:/data/a.jpg\napp:/data/b.jpg\napp:/data/c.jpg\napp:/data/d.jpeg".into(),
            "\nMount Fuji.jpg\n\nPhoto.jpg".into()
        )
    );
    assert_eq!(
        refusals(&r),
        [
            "drag: refused: app:/data/../x.pdf is not a plain app:/ path",
            "drag: refused: name \"Fuji.\": a name does not end with a dot",
        ]
    );
}

#[test]
fn every_boundary_holds_and_the_offered_name_is_checked() {
    let long = "a".repeat(251);
    let longer = "a".repeat(252);
    let src = card(
        r#"["app:/data/a\nb.jpg", "app:/data/c.jpg", "app:/data/d.jpg", "app:/data/e.jpg", "app:/data/f.jpg"]"#,
        &format!(r#"["Gone", "a\nb", "{long}", "{longer}", "", "Extra"]"#),
        "[]",
    );
    let r = boot(&src, Platform::Macos, Kernel::with_monospace());
    // A line break cannot shift a list: the file with one is refused, and each name stays at its place.
    assert_eq!(
        held(&r, "card"),
        (
            "app:/data/c.jpg\napp:/data/d.jpg\napp:/data/e.jpg\napp:/data/f.jpg".into(),
            format!("\n{long}.jpg\n\n")
        )
    );
    assert_eq!(
        refusals(&r),
        [
            "drag: refused: \"app:/data/a\\nb.jpg\": a file with a line break cannot be carried",
            "drag: refused: name \"a\\nb\": a name holds no control character",
            format!("drag: refused: name \"{longer}\": a name is 1 to 255 bytes of UTF-8").as_str(),
            "drag: refused: name \"Extra\": there is no file in files= for it",
        ]
    );
}

#[test]
fn no_names_differ_from_one_empty_name_and_all_empty_names_carry_none() {
    let none = boot(
        &card(r#"["app:/data/a.jpg"]"#, "[]", "[]"),
        Platform::Ios,
        Kernel::with_monospace(),
    );
    assert_eq!(held(&none, "card"), ("app:/data/a.jpg".into(), "".into()));
    let surplus = boot(
        &card("[]", r#"[""]"#, "[]"),
        Platform::Ios,
        Kernel::with_monospace(),
    );
    assert_eq!(held(&surplus, "card"), ("".into(), "".into()));
    assert_eq!(
        refusals(&surplus),
        ["drag: refused: name \"\": there is no file in files= for it"]
    );
    let empty = boot(
        &card(r#"["app:/data/a.jpg"]"#, r#"[""]"#, "[]"),
        Platform::Ios,
        Kernel::with_monospace(),
    );
    assert_eq!(held(&empty, "card"), ("app:/data/a.jpg".into(), "".into()));
    assert!(refusals(&empty).is_empty());
}

#[test]
fn either_list_changing_admits_both_and_a_standing_refusal_is_journaled_once() {
    let src = card(
        r#"["app:/data/../x.pdf", "app:/data/a.jpg"]"#,
        r#"["", "First"]"#,
        r#"["", "Second"]"#,
    );
    let mut r = boot(&src, Platform::Ios, Kernel::with_monospace());
    assert_eq!(
        held(&r, "card"),
        ("app:/data/a.jpg".into(), "First.jpg".into())
    );
    r.dispatch(view(&r, "swap"), Event::Press).unwrap();
    assert_eq!(
        held(&r, "card"),
        ("app:/data/a.jpg".into(), "Second.jpg".into())
    );
    assert_eq!(
        refusals(&r),
        ["drag: refused: app:/data/../x.pdf is not a plain app:/ path"]
    );
    // The web carries no `app:/` file, and so none of its names.
    let web = boot(&src, Platform::Web, Kernel::with_monospace());
    assert_eq!(held(&web, "card"), ("".into(), "".into()));
}

#[test]
fn each_list_changed_or_cleared_alone_admits_both_again() {
    let src = "component A\n  state files: string[] = [\"app:/data/a.jpg\", \"app:/data/b.jpg\"]\n  state names: string[] = [\"First\", \"Second\"]\n  action fewer\n    files = [\"app:/data/b.jpg\"]\n  action unnamed\n    names = []\n  action emptied\n    files = []\n  view\n    column\n      column draggable=true dataTransfer=DataTransfer(files=files, names=names) testId=\"card\"\n        text \"a\"\n      button press=fewer testId=\"fewer\"\n        text \"b\"\n      button press=unnamed testId=\"unnamed\"\n        text \"c\"\n      button press=emptied testId=\"emptied\"\n        text \"d\"\n";
    let mut r = boot(src, Platform::Ios, Kernel::with_monospace());
    assert_eq!(
        held(&r, "card"),
        (
            "app:/data/a.jpg\napp:/data/b.jpg".into(),
            "First.jpg\nSecond.jpg".into()
        )
    );
    // Files changed alone: the names pair again by place, and the one past the files is refused.
    r.dispatch(view(&r, "fewer"), Event::Press).unwrap();
    assert_eq!(
        held(&r, "card"),
        ("app:/data/b.jpg".into(), "First.jpg".into())
    );
    assert_eq!(
        refusals(&r),
        ["drag: refused: name \"Second\": there is no file in files= for it"]
    );
    // Names cleared alone: the files keep their own names.
    r.dispatch(view(&r, "unnamed"), Event::Press).unwrap();
    assert_eq!(held(&r, "card"), ("app:/data/b.jpg".into(), "".into()));
    // Files cleared alone: both props cleared.
    r.dispatch(view(&r, "emptied"), Event::Press).unwrap();
    assert_eq!(held(&r, "card"), ("".into(), "".into()));
    assert_eq!(refusals(&r).len(), 1);
}

/// A refusal is known by what was refused, not by its message: a second
/// refusal of the same file, or another URL refused for the same reason, is
/// journaled, and none repeats while it stands.
#[test]
fn each_refusal_is_known_by_what_was_refused() {
    let src = "component A\n  state files: string[] = [\"app:/data/../x.pdf\"]\n  state link = \"/one\"\n  action more\n    files = [\"app:/data/../x.pdf\", \"app:/data/../x.pdf\"]\n    link = \"/two\"\n  view\n    column\n      column draggable=true dataTransfer=DataTransfer(files=files, url=link) testId=\"card\"\n        text \"a\"\n      button press=more testId=\"more\"\n        text \"b\"\n";
    let mut r = boot(src, Platform::Ios, Kernel::with_monospace());
    let file = "drag: refused: app:/data/../x.pdf is not a plain app:/ path";
    let url = "drag: refused: url is not an absolute http: or https: URL: another app cannot open this app's path";
    assert_eq!(refusals(&r), [file, url]);
    r.dispatch(view(&r, "more"), Event::Press).unwrap();
    assert_eq!(refusals(&r), [file, url, file, url]);
}

#[test]
fn a_node_built_again_journals_its_refusal_again() {
    let src = "component A\n  state shown = true\n  action toggle\n    shown = not shown\n  view\n    column\n      if shown\n        column draggable=true dataTransfer=DataTransfer(files=[\"app:/data/../x.pdf\", \"app:/data/a.jpg\"], names=[\"\", \"A\"]) testId=\"card\"\n          text \"a\"\n      button press=toggle testId=\"toggle\"\n        text \"b\"\n";
    let mut r = boot(src, Platform::Ios, Kernel::with_monospace());
    assert_eq!(held(&r, "card"), ("app:/data/a.jpg".into(), "A.jpg".into()));
    for _ in 0..2 {
        r.dispatch(view(&r, "toggle"), Event::Press).unwrap();
    }
    assert_eq!(held(&r, "card"), ("app:/data/a.jpg".into(), "A.jpg".into()));
    assert_eq!(
        refusals(&r),
        ["drag: refused: app:/data/../x.pdf is not a plain app:/ path"; 2]
    );
}

#[test]
fn a_line_feed_in_a_files_only_binding_refuses_only_its_file() {
    let src = "component A\n  state files: string[] = [\"app:/data/a\\nb.jpg\", \"app:/data/c.jpg\"]\n  view\n    column draggable=true dataTransfer=DataTransfer(files=files) testId=\"card\"\n      text \"a\"\n";
    let r = boot(src, Platform::Ios, Kernel::with_monospace());
    assert_eq!(held(&r, "card"), ("app:/data/c.jpg".into(), "".into()));
    assert_eq!(
        refusals(&r),
        ["drag: refused: \"app:/data/a\\nb.jpg\": a file with a line break cannot be carried"]
    );
}

#[test]
fn a_plan_compiled_before_names_is_read_as_before() {
    // A plan from before D14 binds `dataTransferFiles`, newline-joined paths, even one that begins as the
    // encoding would: its entries are read as paths, never decoded.
    let src = card(
        r#"["app:/data/a.jpg", "app:/data/b.jpg"]"#,
        r#"["A"]"#,
        "[]",
    );
    let mut plan = contract::compile(&src).unwrap();
    let mut rebound = 0;
    for row in plan.bindings.to_mut().iter_mut() {
        if row.id == PropId::DataTransferEntries as u16 {
            row.id = PropId::DataTransferFiles as u16;
            rebound += 1;
        }
    }
    assert_eq!(rebound, 1);
    let plan = contract::bake(plan, boxed(Grants)).unwrap();
    let r = Platform::Ios.boot(|| {
        Runner::boot(
            plan,
            boxed(Grants),
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap()
    });
    assert_eq!(held(&r, "card"), ("".into(), "".into()));
    assert_eq!(
        refusals(&r),
        [
            "drag: refused: app%3A%2Fdata%2Fa.jpg is neither a shipped assets/ file nor an app:/ path",
            "drag: refused: app%3A%2Fdata%2Fb.jpg is neither a shipped assets/ file nor an app:/ path",
            "drag: refused: \u{1e}A is neither a shipped assets/ file nor an app:/ path",
        ]
    );
}

#[test]
fn a_render_with_no_kernel_folds_the_same_names() {
    let src = card(
        r#"["app:/data/a.jpg", "app:/data/b.jpg"]"#,
        r#"["", "Mount Fuji"]"#,
        "[]",
    );
    let mut r = boot(&src, Platform::Ios, Kernel::with_monospace());
    let id = view(&r, "card");
    let doc = r.document_tree().unwrap();
    let props = &doc.node(id).unwrap().props;
    assert_eq!(
        props.str(PropId::DataTransferFiles),
        Some("app:/data/a.jpg\napp:/data/b.jpg")
    );
    assert_eq!(
        props.str(PropId::DataTransferNames),
        Some("\nMount Fuji.jpg")
    );
    assert_eq!(props.str(PropId::DataTransferEntries), None);
}

/// The encoding is the plan's own operations: an app's `fn` named `map`,
/// `join` or `encodeURIComponent` does not stand in for them.
#[test]
fn an_apps_own_functions_do_not_change_the_encoding() {
    let src = format!(
        "fn map(n: number): number = n\nfn join(a: string, b: string): string = \"joined\"\nfn encodeURIComponent(s: string): string = \"x\"\n{}",
        card(r#"["app:/data/a b.jpg", "app:/data/c.jpg"]"#, r#"["Mount Fuji", ""]"#, "[]")
    );
    let r = boot(&src, Platform::Ios, Kernel::with_monospace());
    assert_eq!(
        held(&r, "card"),
        (
            "app:/data/a b.jpg\napp:/data/c.jpg".into(),
            "Mount Fuji.jpg\n".into()
        )
    );
    assert!(refusals(&r).is_empty(), "{:?}", refusals(&r));
}

#[test]
fn the_compiler_refuses_what_literals_tell() {
    for (data, why) in [
        (
            r#"DataTransfer(text="a", names=["A"])"#,
            "`names=` names the files of `files=`, and this `DataTransfer` has no `files=`",
        ),
        (
            r#"DataTransfer(files=["app:/data/a.jpg"], names=["A", "B"])"#,
            "`names=` has 2 names for 1 file: each name goes with the file at its place in `files=`",
        ),
        (
            r#"DataTransfer(files=["app:/data/a.jpg"], names=["photos/1016.jpg"])"#,
            "`names=` takes file names, not \"photos/1016.jpg\": a name holds none of",
        ),
        (r#"DataTransfer(files=["app:/data/a.jpg"], names=["CON"])"#, "Windows keeps that name for a device"),
        (r#"DataTransfer(files=["app:/data/a.jpg"], names="A")"#, "`names=` is a list of strings"),
    ] {
        let src = format!("component A\n  view\n    column draggable=true dataTransfer={data}\n      text \"a\"\n");
        let e = contract::compile(&src).expect_err(data).to_string();
        assert!(e.contains(why), "{data}: {e}");
    }
    // An empty name keeps the file's own, and fewer names than files leave the rest their own.
    let src = "component A\n  view\n    column draggable=true dataTransfer=DataTransfer(files=[\"app:/data/a.jpg\", \"app:/data/b.jpg\"], names=[\"\"])\n      text \"a\"\n";
    contract::compile(src).unwrap();
}
