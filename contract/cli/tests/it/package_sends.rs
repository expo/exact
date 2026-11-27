//! A package's component sends through its own package's module, and no other
//! (LLP 1035.005.004 D6, D7).

use contract::CompileError;
use std::path::PathBuf;

/// A scratch app with the package `@scratch/comments`, which a test's
/// component is in, and `@scratch/other`.
struct App(PathBuf);

impl App {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("exact-package-sends-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let app = Self(dir);
        app.package(
            "comments",
            r#"{"source": "commentsLoad", "options": {"post": "string"}, "answers": "Thread"},
    {"send": "commentsPost", "options": {"post": "string"}, "answers": "Thread"},
    {"send": "commentsSeen", "options": {"post": "string"}, "answers": null}"#,
        );
        app.package(
            "other",
            r#"{"send": "otherPost", "options": {"post": "string"}, "answers": "Thread"},
    {"send": "otherSeen", "options": {"post": "string"}, "answers": null}"#,
        );
        app.write(
            "app/app.contract",
            "use Comments from \"@scratch/comments/ui\"\ncomponent App\n  view\n    Comments(post=\"p1\")\n",
        );
        app
    }

    fn write(&self, name: &str, source: &str) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }

    fn package(&self, name: &str, names: &str) {
        let at = format!("app/node_modules/@scratch/{name}");
        self.write(
            &format!("{at}/package.json"),
            &format!(
                r#"{{"name":"@scratch/{name}","version":"1.0.0","exports":{{".":"./exact.json","./ui":{{"contract":"./ui.contract"}}}}}}"#
            ),
        );
        self.write(
            &format!("{at}/exact.json"),
            &format!(
                r#"{{
  "kind": "interface", "name": "@scratch/{name}", "version": "1.0.0",
  "shapes": {{"Thread": {{"count": "number"}}}},
  "names": [{names}]
}}"#
            ),
        );
    }

    /// Compiles the app with a `Comments` whose file has `uses`, whose body
    /// starts with `declarations` and whose `submit` action is `body`.
    fn compile(&self, uses: &str, declarations: &str, body: &str) -> Result<(), CompileError> {
        self.write(
            "app/node_modules/@scratch/comments/ui.contract",
            &format!(
                "{uses}\ncomponent Comments\n  props\n    post: string\n  {declarations}\n  state busy: bool = false\n  action submit\n    {body}\n  view\n    button press=submit\n      text \"Post\"\n"
            ),
        );
        contract::compile_path(&self.0.join("app/app.contract")).map(|_| ())
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const POSTED: &str = "mutation posted as shape Thread";

#[test]
fn a_package_component_sends_through_its_own_packages_sends() {
    let app = App::new("own");
    let uses = "use commentsPost, commentsSeen, Thread from \"@scratch/comments\"";
    for body in [
        "send posted = commentsPost(post=post)",
        // Nested in the action's control flow.
        "if busy\n      busy = false\n    else\n      send posted = commentsPost(post=post)",
        // One that answers nothing is a statement (LLP 1133.004 D5).
        "commentsSeen(post=post)",
    ] {
        app.compile(uses, POSTED, body)
            .unwrap_or_else(|e| panic!("{body}: {e}"));
    }
    // Renamed by its `use`.
    app.compile(
        "use commentsPost as publish, Thread from \"@scratch/comments\"",
        POSTED,
        "send posted = publish(post=post)",
    )
    .unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn a_package_component_sends_through_no_other_module() {
    let app = App::new("foreign");
    for (uses, name, args) in [
        ("use Thread from \"@scratch/comments\"", "appPost", "post"),
        (
            "use otherPost, Thread from \"@scratch/other\"",
            "otherPost",
            "post=post",
        ),
        // Its package's send, without the `use` that makes it the module's.
        (
            "use Thread from \"@scratch/comments\"",
            "commentsPost",
            "post",
        ),
    ] {
        let e = app
            .compile(uses, POSTED, &format!("send posted = {name}({args})"))
            .unwrap_err();
        assert_eq!(e.id, "type-package-source", "{e}");
        let begins = format!("`Comments` comes from the package `@scratch/comments`, and its `send posted` asks `{name}`");
        assert!(e.message.starts_with(&begins), "{}", e.message);
    }
}

#[test]
fn a_package_components_bare_send_stays_in_its_own_package() {
    let app = App::new("foreign-bare");
    for (uses, name) in [
        ("use otherSeen from \"@scratch/other\"", "otherSeen"),
        (
            "use otherSeen as markSeen from \"@scratch/other\"",
            "markSeen",
        ),
    ] {
        for body in [
            format!("{name}(post=post)"),
            format!("if busy\n      busy = false\n    else\n      {name}(post=post)"),
            format!("match some(post)\n      case some(p)\n        {name}(post=p)\n      case none\n        busy = false"),
        ] {
            let e = app.compile(uses, "", &body).unwrap_err();
            assert_eq!(e.id, "type-package-source", "{body}: {e}");
            assert!(
                e.message.starts_with("`Comments` comes from the package `@scratch/comments`, and its `otherSeen(…)` asks `otherSeen`"),
                "{e}"
            );
            assert!(e.message.ends_with("an action prop that sends it"), "{e}");
        }
    }
    app.compile(
        "use commentsSeen as markSeen from \"@scratch/comments\"",
        "action local\n    busy = true",
        "local()\n    markSeen(post=post)",
    )
    .unwrap_or_else(|e| panic!("{e}"));
    // Apply the diagnostic's repair: the app sends through an action prop.
    app.write(
        "app/node_modules/@scratch/comments/ui.contract",
        "component Comments\n  props\n    post: string\n    seen: action\n  action submit\n    seen()\n  view\n    button press=submit\n      text post\n",
    );
    app.write(
        "app/app.contract",
        "use Comments from \"@scratch/comments/ui\"\nuse otherSeen from \"@scratch/other\"\ncomponent App\n  action seen\n    otherSeen(post=\"p1\")\n  view\n    Comments(post=\"p1\", seen=seen)\n",
    );
    contract::compile_path(&app.0.join("app/app.contract")).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn a_package_components_request_matches_the_kind_its_module_declares() {
    let app = App::new("kind");
    let uses = "use commentsLoad, commentsPost, commentsSeen, Thread from \"@scratch/comments\"";
    for (declarations, body, id) in [
        (
            "resource thread = commentsPost(post=post) as shape Thread",
            "busy = true",
            "contract-module-kind",
        ),
        (
            POSTED,
            "send posted = commentsLoad(post=post)",
            "contract-module-kind",
        ),
        (
            POSTED,
            "send posted = commentsSeen(post=post)",
            "contract-module-answers",
        ),
    ] {
        let e = app.compile(uses, declarations, body).unwrap_err();
        assert_eq!(e.id, id, "{e}");
    }
}
