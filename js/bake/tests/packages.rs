//! Package code in a data module (LLP 1027 D5, "Package code"): what the
//! bundle reaches of an installed package is captured, runs alike in every
//! producer, and what no executor has is refused by package and file.
use exact_js::Paired;
use exact_js_bake::{bake, Tools};
use exact_kernel::Kernel;
use exact_runner::{Runner, Value};
use serde_json::Value as Json;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const CONTRACT: &str = r#"
component App
  state count = 0
  resource message = message(count) as shape string
  view
    column
      text message testId="message"
"#;

struct App(PathBuf);
impl App {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("exact-packages-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir(&path).unwrap();
        let app = Self(path);
        app.put("app.contract", CONTRACT);
        app.put(
            "app.json",
            r#"{"name":"Logic","app":{"id":"test.exact.logic","name":"Logic"}}"#,
        );
        app
    }
    fn put(&self, name: &str, text: &str) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    /// `app.ts` answering `message` with `expression` (of `count`).
    fn answer(&self, imports: &str, expression: &str) {
        self.put(
            "app.ts",
            &format!("import type {{ Sources, Answer }} from './app.contract.d.ts';\n{imports}\nexport const appId = 'test.exact.logic';\nexport const grants = '';\nconst sources: Sources = {{ message: ([count]) => {expression} }};\nexport const answer: Answer = (source, args, store, storage) => sources[source](args, store, storage);\n"),
        );
    }
}
impl Drop for App {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn web_build(app: &Path) -> Result<(), String> {
    let out = app.join("web-out");
    let _ = std::fs::remove_dir_all(&out);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let built = std::process::Command::new(std::env::var_os("BUN").unwrap_or_else(|| "bun".into()))
        .arg(root.join("host/web-js/build.mjs"))
        // `js` builds the server bundle too, which replays the capture under Bun.
        .args(["logic", "--render", "js", "--out"])
        .arg(&out)
        .env("EXACT_APP_DIR", app)
        .current_dir(&root)
        .output()
        .unwrap();
    if built.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&built.stderr).into_owned())
    }
}

/// The web build, then (with Hermes) the resident and one-shot producers.
fn every(app: &App) -> Vec<Result<(), String>> {
    let mut results = vec![web_build(&app.0)];
    if exact_js::ENGINE_LINKED {
        let mut producer = exact_js_bake::Producer::new(Tools::default()).unwrap();
        results.push(producer.bake(&app.0, None).map(|_| ()));
        results.push(bake(&app.0, &Tools::default()).map(|_| ()));
    }
    results
}

fn message(app: &App) -> Value {
    let baked = bake(&app.0, &Tools::default()).unwrap();
    let receipt: Json = serde_json::from_str(&baked.receipt).unwrap();
    let paired = Paired::decode(
        &baked.receipt,
        &baked.plan,
        baked.bytecode.clone(),
        "test.exact.logic",
        receipt["grants"].as_str().unwrap(),
    )
    .unwrap();
    let runner = Runner::boot(
        paired.plan,
        paired.module,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    runner.resource("message").unwrap().clone()
}

/// A side effect on this machine, written as no static check sees it: it
/// runs only if something executes the file in the build's own process.
fn sentinel(ran: &Path) -> String {
    format!(
        "globalThis['pro' + 'cess']?.getBuiltinModule?.('fs')?.writeFileSync({:?}, 'ran');\n",
        ran.to_str().unwrap()
    )
}

/// The web build's record of the captured package code.
fn record(app: &App) -> Json {
    serde_json::from_slice(
        &std::fs::read(app.0.join("web-out/.gen/typescript/__exact_packages.json")).unwrap(),
    )
    .unwrap()
}

/// ESM and CommonJS packages (`exports` with a `browser` condition, `main`,
/// a `browser` field's `false`, `.mjs`/`.cjs`, JSON, a nested dependency) and
/// the app's own `.js` bundle from the capture alike in every producer: the
/// page and Hermes run the `browser` file, the unselected Node entry never
/// runs, each captured file's recorded digest is its bytes, the receipt
/// carries the record's digests, and a bake is reproducible.
#[test]
fn installed_packages_and_app_javascript_bundle_from_the_capture() {
    let app = App::new("bundle");
    let ran = app.0.join("ran");
    app.put(
        "node_modules/esm/package.json",
        r#"{"name":"esm","version":"2.0.0","type":"module","exports":{".":{"browser":"./web.js","default":"./node.js"},"./extra":"./extra.mjs"}}"#,
    );
    app.put(
        "node_modules/esm/web.js",
        "export const where = 'web-build';\n",
    );
    app.put(
        "node_modules/esm/node.js",
        &format!("{}export const where = 'node-build';\n", sentinel(&ran)),
    );
    app.put("node_modules/esm/extra.mjs", "export default 'extra';\n");
    app.put(
        "node_modules/cjs/package.json",
        r#"{"name":"cjs","version":"1.5.4","main":"./lib/index.js","browser":{"fs":false}}"#,
    );
    app.put(
        "node_modules/cjs/lib/index.js",
        "const fs = require('fs');\nconst table = require('./table.json');\nconst pad = require('pad');\nconst twice = require('./twice.cjs');\nexports.encode = (n) => pad(String(twice(n) + table.offset)) + (typeof fs === 'object' ? 'fs' : '');\n",
    );
    app.put("node_modules/cjs/lib/table.json", r#"{"offset":100}"#);
    app.put(
        "node_modules/cjs/lib/twice.cjs",
        "module.exports = (n) => n * 2;\n",
    );
    app.put(
        "node_modules/cjs/node_modules/pad/package.json",
        r#"{"name":"pad","version":"0.1.0"}"#,
    );
    // A dominating test: a package that runs without the DOM is not refused.
    app.put(
        "node_modules/cjs/node_modules/pad/index.js",
        "module.exports = (s) => (typeof window !== 'undefined' ? window.name : '') + '<' + s + '>';\n",
    );
    app.put(
        "packages.d.ts",
        "declare module 'esm' { export const where: string; }\ndeclare module 'esm/extra' { const extra: string; export default extra; }\ndeclare module 'cjs' { export function encode(n: number): string; }\n",
    );
    app.put(
        "src/helper.js",
        "export const joined = (...parts) => parts.join(' ');\n",
    );
    app.answer(
        "import { where } from 'esm';\nimport extra from 'esm/extra';\nimport { encode } from 'cjs';\nimport { joined } from './src/helper.js';",
        "joined(where, extra, encode(count))",
    );
    for result in every(&app) {
        result.unwrap();
    }
    assert!(!ran.exists(), "the unselected Node entry never runs");
    let page: String = std::fs::read_dir(app.0.join("web-out"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "js"))
        .map(|e| std::fs::read_to_string(e.path()).unwrap())
        .collect();
    assert!(
        page.contains("web-build") && !page.contains("node-build"),
        "the page bundles the browser file"
    );
    let record = record(&app);
    let files = record["files"].as_array().unwrap();
    for file in files {
        let bytes = std::fs::read(file[2].as_str().unwrap()).unwrap();
        assert_eq!(
            file[3].as_str().unwrap(),
            format!("{:x}", Sha256::digest(&bytes)),
            "{file}"
        );
    }
    let names: Vec<String> = files
        .iter()
        .map(|f| format!("{}/{}", f[0].as_str().unwrap(), f[1].as_str().unwrap()))
        .collect();
    assert!(
        names.contains(&"cjs@1.5.4/lib/twice.cjs".into())
            && names.contains(&"pad@0.1.0/index.js".into()),
        "{names:?}"
    );
    assert!(
        !names.iter().any(|n| n.ends_with("node.js")),
        "only what the bundle reaches is captured: {names:?}"
    );
    let installed = record["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p[0] == "pad@0.1.0")
        .unwrap()[2]
        .clone();
    assert!(
        installed[0]
            .as_str()
            .unwrap()
            .ends_with("node_modules/cjs/node_modules/pad"),
        "{installed}"
    );
    if !exact_js::ENGINE_LINKED {
        return;
    }
    assert_eq!(message(&app), Value::str("web-build extra <100>fs"));
    let first = bake(&app.0, &Tools::default()).unwrap();
    let again = bake(&app.0, &Tools::default()).unwrap();
    assert_eq!(
        (&first.bytecode, &first.receipt),
        (&again.bytecode, &again.receipt)
    );
    let receipt: Json = serde_json::from_str(&first.receipt).unwrap();
    assert_eq!(
        receipt["packages"], record["digests"],
        "the receipt carries the capture's digests"
    );
    // A captured file changed is another bake.
    app.put(
        "node_modules/cjs/lib/twice.cjs",
        "module.exports = (n) => n * 3;\n",
    );
    let changed: Json =
        serde_json::from_str(&bake(&app.0, &Tools::default()).unwrap().receipt).unwrap();
    assert_ne!(
        changed["packages"]["cjs@1.5.4"],
        receipt["packages"]["cjs@1.5.4"]
    );
    assert_eq!(
        changed["packages"]["pad@0.1.0"],
        receipt["packages"]["pad@0.1.0"]
    );
}

/// An install that is a link (as Bun's and pnpm's isolated layouts make
/// them) is recorded where it is installed, and retargeting it to another
/// copy of the same version is another bake; links swapped between two
/// packages change the record too.
#[cfg(unix)]
#[test]
fn a_retargeted_install_link_is_another_bake() {
    let app = App::new("links");
    for (dir, word) in [
        ("node_modules/.store/a1", "first"),
        ("node_modules/.store/a2", "second"),
        ("node_modules/.store/b", "bee"),
    ] {
        let name = if dir.ends_with("/b") { "b" } else { "a" };
        app.put(
            &format!("{dir}/package.json"),
            &format!(r#"{{"name":"{name}","version":"1.0.0","main":"index.js"}}"#),
        );
        app.put(
            &format!("{dir}/index.js"),
            &format!("module.exports = '{word}';\n"),
        );
    }
    std::fs::create_dir_all(app.0.join("node_modules")).unwrap();
    let link = |name: &str, to: &str| {
        let path = app.0.join("node_modules").join(name);
        let _ = std::fs::remove_file(&path);
        std::os::unix::fs::symlink(app.0.join(to), path).unwrap();
    };
    link("a", "node_modules/.store/a1");
    link("b", "node_modules/.store/b");
    app.put("packages.d.ts", "declare module 'a' { const a: string; export default a; }\ndeclare module 'b' { const b: string; export default b; }\n");
    app.answer("import a from 'a';\nimport b from 'b';", "a + b + count");
    web_build(&app.0).unwrap();
    let installed = record(&app)["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p[0] == "a@1.0.0")
        .unwrap()[2]
        .clone();
    assert_eq!(
        installed[0].as_str().unwrap(),
        app.0
            .canonicalize()
            .unwrap()
            .join("node_modules/a")
            .to_str()
            .unwrap(),
        "recorded as installed, not as stored"
    );
    let digests = record(&app)["digests"].clone();
    link("a", "node_modules/.store/a2");
    web_build(&app.0).unwrap();
    assert_ne!(
        record(&app)["digests"]["a@1.0.0"],
        digests["a@1.0.0"],
        "a retargeted link is another capture"
    );
    if exact_js::ENGINE_LINKED {
        assert_eq!(message(&app), Value::str("secondbee0"));
    }
    // The same files reached by each other's names: the resolutions are captured too.
    let digests = record(&app)["digests"].clone();
    link("a", "node_modules/.store/b");
    link("b", "node_modules/.store/a2");
    web_build(&app.0).unwrap();
    let swapped = record(&app)["digests"].clone();
    assert!(swapped["a@1.0.0"] != digests["a@1.0.0"] && swapped["b@1.0.0"] != digests["b@1.0.0"]);
}

/// Code from a string throws on every host (Hermes compiles none; the web's
/// module realm refuses it as Hermes does), so a package that uses it is no
/// longer refused: each producer builds it and warns once per site, naming
/// the package, its version and the file, a guarded probe and an unguarded
/// call alike (#128).
#[test]
fn package_code_from_a_string_builds_with_a_warning_per_site() {
    let app = App::new("strings");
    app.put(
        "packages.d.ts",
        "declare module 'lazy' { export const fast: () => boolean; export const made: () => number; export const guarded: () => number; }\n",
    );
    app.put(
        "node_modules/lazy/package.json",
        r#"{"name":"lazy","version":"2.0.0","main":"index.js"}"#,
    );
    app.put(
        "node_modules/lazy/index.js",
        concat!(
            "exports.fast = () => { try { const F = Function; new F(''); return true; } catch { return false; } };\n",
            "exports.made = () => new Function('return 1')() + (0, eval)('1') + globalThis.eval('1');\n",
            // A `typeof` test guards nothing here: the stand-in is a function, and the call throws.
            "exports.guarded = () => { if (typeof globalThis.Function === 'function') return globalThis.Function('return 42')(); if (typeof globalThis.eval === 'function') return globalThis.eval('1'); };\n",
            // A stand-in's own `constructor` is the throwing stand-in, fixed in place: a probe through it is warned.
            "exports.ctor = () => { try { return Function.constructor('return 1')() + eval.constructor('return 1')(); } catch { return 0; } };\n",
            // A parameter named `Function` hides the global in its own function only; `instanceof` builds nothing.
            "exports.own = function (Function) { return new Function('x') && eval instanceof Function; }; exports.later = () => { try { return new Function('return 1')(); } catch { return 0; } };\n",
        ),
    );
    // A package whose runtime is TypeScript or JSX, with its types apart, is checked as JavaScript is.
    app.put(
        "node_modules/typed/package.json",
        r#"{"name":"typed","version":"1.0.0","main":"index.ts","types":"index.d.ts"}"#,
    );
    app.put(
        "node_modules/typed/index.d.ts",
        "export declare const compile: (source: string) => unknown;\n",
    );
    app.put(
        "node_modules/typed/index.ts",
        "import { run } from './run.jsx';\nexport const compile = (source: string): unknown => { try { return new Function(source) && run(); } catch { return null; } };\n",
    );
    app.put(
        "node_modules/typed/run.jsx",
        "export const run = () => eval('1');\n",
    );
    // The app's own TypeScript is checked for code from a string too.
    // A namespace's body and an enum's members run; a `const` that holds a class is not `Function` because an inner
    // scope later binds that class's name to `Function`.
    app.answer(
        "import { fast, made, guarded } from 'lazy'; import { compile } from 'typed'; namespace N { export const v = () => new Function('')(); } enum E { A = ((f: unknown) => 1)(() => new Function('')) } class Base {} const F = Base; export function proto() { const Base = Function; return [F.prototype, Base, N.v, E.A]; }",
        "String(fast() ? made() + guarded() : count) + (count < 0 ? new Function('')() : '') + (count < -1 ? compile('') : '')",
    );
    let sites = [
        "lazy@2.0.0/index.js:1:40 warning [code-from-string] uses 'Function'",
        "lazy@2.0.0/index.js:2:26 warning [code-from-string] uses 'Function'",
        "lazy@2.0.0/index.js:2:55 warning [code-from-string] uses 'eval'",
        "lazy@2.0.0/index.js:2:68 warning [code-from-string] uses 'eval'",
        "lazy@2.0.0/index.js:3:81 warning [code-from-string] uses 'Function'",
        "lazy@2.0.0/index.js:3:167 warning [code-from-string] uses 'eval'",
        "lazy@2.0.0/index.js:4:37 warning [code-from-string] uses 'Function'",
        "lazy@2.0.0/index.js:4:74 warning [code-from-string] uses 'eval'",
        "lazy@2.0.0/index.js:5:135 warning [code-from-string] uses 'Function'",
        "typed@1.0.0/index.ts:2:72 warning [code-from-string] uses 'Function'",
        "typed@1.0.0/run.jsx:1:26 warning [code-from-string] uses 'eval'",
        "app.ts:5:113 warning [code-from-string] uses 'Function'",
        "app.ts:2:119 warning [code-from-string] uses 'Function'",
        "app.ts:2:180 warning [code-from-string] uses 'Function'",
        "app.ts:2:265 warning [code-from-string] uses 'Function'",
    ];
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let built = std::process::Command::new(std::env::var_os("BUN").unwrap_or_else(|| "bun".into()))
        .arg(root.join("host/web-js/build.mjs"))
        .args(["logic", "--out"])
        .arg(app.0.join("web-out"))
        .env("EXACT_APP_DIR", &app.0)
        .current_dir(&root)
        .output()
        .unwrap();
    let said = String::from_utf8_lossy(&built.stderr);
    assert!(built.status.success(), "{said}");
    for site in sites {
        assert_eq!(
            said.matches(&format!("{site}, which builds code from a string"))
                .count(),
            1,
            "{site}: {said}"
        );
    }
    if exact_js::ENGINE_LINKED {
        let mut producer = exact_js_bake::Producer::new(Tools::default()).unwrap();
        for warnings in [
            producer.bake(&app.0, None).unwrap().warnings,
            bake(&app.0, &Tools::default()).unwrap().warnings,
        ] {
            for site in sites {
                assert_eq!(
                    warnings.iter().filter(|w| w.starts_with(site)).count(),
                    1,
                    "{site}: {warnings:?}"
                );
            }
        }
    }
    // Line 5's parameter and `instanceof` are not the global's builder: its one warning is the later call.
    assert_eq!(said.matches("lazy@2.0.0/index.js:5:").count(), 1, "{said}");
    // The list is the producer's, in the stage: an app file of its name is refused, not overwritten.
    app.put("__exact_code_warnings.json", "[\"the app's own\"]");
    for result in every(&app) {
        assert!(result
            .expect_err("reserved")
            .contains("reserved for the producer"));
    }
    assert_eq!(
        std::fs::read_to_string(app.0.join("__exact_code_warnings.json")).unwrap(),
        "[\"the app's own\"]"
    );
}

/// Hermes's `eval` is no constructor, as the web's stand-ins are not
/// (host/web/code-from-strings.test.mjs `EVAL_NEW`, the same answers).
#[test]
fn eval_is_no_constructor_on_hermes() {
    if !exact_js::ENGINE_LINKED {
        return;
    }
    let app = App::new("eval-new");
    app.answer(
        "const thrown = (f: () => unknown) => { try { f(); return 'ran'; } catch (e) { return (e as Error).name; } };",
        "[thrown(() => new (eval as any)('1')), thrown(() => Reflect.construct(eval as any, [])), typeof (eval as any).prototype, eval instanceof Function, count].join(' ')",
    );
    assert_eq!(
        message(&app),
        Value::str("TypeError TypeError undefined true 0")
    );
}

/// What no executor has is refused by every producer, naming the package,
/// its version and the file: a Node built-in, a native addon, a dependency
/// not installed, the DOM, Node's globals, and a computed `require`. Nothing
/// of a package runs in the build's process, refused or not.
#[test]
fn package_code_no_executor_runs_is_refused_by_package_and_file() {
    let app = App::new("refused");
    app.put("packages.d.ts", "declare module 'bad' { const bad: string; export default bad; }\ndeclare module 'absent' { const a: string; export default a; }\n");
    // Each after the sentinel line, so a position is on line 2.
    let cases = [
        (
            "const fs = require('fs');\nmodule.exports = 'x';\n",
            "bad@3.1.0 (index.js) imports Node's built-in 'fs'",
        ),
        (
            "module.exports = require('./build/Release/bad.node');\n",
            "imports bad@3.1.0, a native addon",
        ),
        (
            "module.exports = require('missing');\n",
            "bad@3.1.0 (index.js) imports 'missing', which is not installed",
        ),
        (
            "module.exports = document.title;\n",
            "bad@3.1.0 (index.js:2:18) uses the DOM's 'document'",
        ),
        (
            "module.exports = Buffer.from('x').toString();\n",
            "bad@3.1.0 (index.js:2:18) uses Node's 'Buffer'",
        ),
        (
            "const name = 'x';\nmodule.exports = require('./' + name);\n",
            "bad@3.1.0 (index.js:3:18) loads a module by a computed name",
        ),
        // An unrelated test of the name, or a parameter of that name in another function, guards nothing.
        (
            "typeof document;\nmodule.exports = document.title;\n",
            "bad@3.1.0 (index.js:3:18) uses the DOM's 'document'",
        ),
        (
            "function f(document) { return document; }\nmodule.exports = () => document.title;\n",
            "bad@3.1.0 (index.js:3:24) uses the DOM's 'document'",
        ),
        // Nor does a `try`, or reaching it through the global object.
        (
            "try { module.exports = document.cookie; } catch (e) {}\n",
            "bad@3.1.0 (index.js:2:24) uses the DOM's 'document'",
        ),
        (
            "module.exports = () => globalThis.localStorage.getItem('k');\n",
            "bad@3.1.0 (index.js:2:24) uses the DOM's 'localStorage'",
        ),
        // A local named `Function` is the alias check's: here it holds `process`.
        (
            "const Function = globalThis.process;\nmodule.exports = () => Function.cwd();\n",
            "bad@3.1.0 (index.js:3:24) uses Node's 'process'",
        ),
        // The page's other names for itself.
        (
            "module.exports = () => globalThis.top.document.cookie;\n",
            "bad@3.1.0 (index.js:2:24) uses the DOM's 'top'",
        ),
        // Node's `process` read where its absence throws: bare (`?.` does not
        // catch a name that is not there), or a member of an absent value.
        (
            "module.exports = () => process.env.API;\n",
            "bad@3.1.0 (index.js:2:24) uses Node's 'process'",
        ),
        (
            "module.exports = () => process?.env;\n",
            "bad@3.1.0 (index.js:2:24) uses Node's 'process'",
        ),
        (
            "module.exports = () => globalThis.process.env.API;\n",
            "bad@3.1.0 (index.js:2:24) uses Node's 'process'",
        ),
        (
            "const node = globalThis.process;\nmodule.exports = () => node.env.API;\n",
            "bad@3.1.0 (index.js:3:24) uses Node's 'process'",
        ),
        (
            "module.exports = (name) => import(name);\n",
            "bad@3.1.0 (index.js:2:28) loads code from a string ('import()')",
        ),
        // A setting is app.ts's to read (LLP 1153 D4), not a package function's: a read the bundle keeps,
        // `?.` or not, is refused by its place in the package.
        (
            "export default (() => import.meta.env.API_BASE_URL)();\n",
            "bad@3.1.0 (index.js:2:23) reads import.meta.env, which only app.ts may",
        ),
        (
            "export default (() => String(import.meta?.env))();\n",
            "bad@3.1.0 (index.js:2:30) reads import.meta.env, which only app.ts may",
        ),
    ];
    app.put(
        "node_modules/bad/package.json",
        r#"{"name":"bad","version":"3.1.0"}"#,
    );
    app.put("node_modules/bad/build/Release/bad.node", "not JavaScript");
    app.put(
        "node_modules/bad/fn.js",
        "export const twice = (n) => n * 2;\n",
    );
    let ran = app.0.join("ran");
    app.answer("import bad from 'bad';", "bad + count");
    // A global the check cannot follow, one per line (the first two, `Function.prototype` held or destructured, are
    // warned as every use of `Function` is, not refused): a `const` holding `globalThis.process` (a `for` head's, one whose `globalThis` a later
    // scope shadows, one holding an optional chain from it or from another such `const`) used unguarded; that chain
    // as a parameter's default;
    // a test the check cannot read as `process` being there (a property beside one, `||`, `!`); `Function`
    // beside `export * as Function`, which binds no name in its own module; and a global held through `??`.
    let unfollowed = [
        "const proto = Function.prototype; proto.constructor('return 1');",
        "const { constructor: make } = Function.prototype; make('return 1');",
        "for (const process = globalThis.process; ; ) { process.env.HOME; break; }",
        "const node = globalThis.process; function hide(globalThis) { return node.env.HOME; }",
        "const env = globalThis.process?.env; env.API;",
        "const host = globalThis.process; const vars = host?.env; export const home = () => vars.HOME;",
        "function read(env = globalThis.process?.env) { return env.API; }",
        "const features = { node: typeof process !== 'undefined', enabled: true }; features.enabled ? process.cwd() : '/';",
        "const ok = typeof Object !== 'undefined' || typeof process !== 'undefined'; if (ok) process.env.HOME;",
        "function absent() { const no = typeof process === 'undefined'; if (!no) return; return process.env.HOME; }",
        "export * as Function from './fn.js';",
        "export const made = Function('return 1');",
        "const Kind = globalThis.document?.defaultView?.Element ?? globalThis.Element; Kind.prototype;",
        "export default [make, hide, read, absent];",
    ];
    app.put(
        "node_modules/bad/index.js",
        &format!("{}{}\n", sentinel(&ran), unfollowed.join("\n")),
    );
    let refusals = [
        "4:48) uses Node's 'process'",
        "5:69) uses Node's 'process'",
        "6:38) uses Node's 'process'",
        "7:84) uses Node's 'process'",
        "8:21) uses Node's 'process'",
        "9:94) uses Node's 'process'",
        "10:85) uses Node's 'process'",
        "11:88) uses Node's 'process'",
        "14:59) uses the DOM's 'Element'",
    ];
    for result in every(&app) {
        let error = result.expect_err("unfollowed");
        for why in refusals {
            assert!(
                error.contains(&format!("bad@3.1.0 (index.js:{why}")),
                "{why}: {error}"
            );
        }
        assert_eq!(
            error.matches("bad@3.1.0 (index.js:").count(),
            refusals.len(),
            "{error}"
        );
    }
    assert!(!ran.exists(), "a refused package's code never runs");
    for (code, why) in cases {
        app.put(
            "node_modules/bad/index.js",
            &format!("{}{code}", sentinel(&ran)),
        );
        for result in every(&app) {
            let error = result.expect_err(why);
            assert!(error.contains(why), "{why}: {error}");
        }
        assert!(!ran.exists(), "a refused package's code never runs: {why}");
    }
    // A package that is a native addon by its build file.
    app.put("node_modules/bad/index.js", "module.exports = 'x';\n");
    app.put("node_modules/bad/binding.gyp", "{}");
    for result in every(&app) {
        let error = result.expect_err("binding.gyp");
        assert!(
            error.contains("imports bad@3.1.0, a native addon"),
            "{error}"
        );
    }
    std::fs::remove_file(app.0.join("node_modules/bad/binding.gyp")).unwrap();
    // A direct dependency the app declares but nobody installed.
    app.answer("import absent from 'absent';", "absent + count");
    for result in every(&app) {
        let error = result.expect_err("absent");
        assert!(
            error.contains("app.ts imports 'absent', which is not installed"),
            "{error}"
        );
    }
    // The app's own JavaScript is held to the same.
    app.put(
        "src/helper.js",
        "export const encode = (s) => Buffer.from(s).toString('hex');\n",
    );
    app.answer(
        "import { encode } from './src/helper.js';",
        "encode('a') + count",
    );
    for result in every(&app) {
        let error = result.expect_err("helper");
        assert!(
            error.contains("src/helper.js:1:30 uses Node's 'Buffer'"),
            "{error}"
        );
    }
    // A TypeScript value of the app's own named `Function`, a namespace, an enum or a parameter property, is not the
    // global: no refusal, and no warning. (Nothing in a module, strict code, may bind `eval`.)
    app.answer(
        "namespace Function { export const prototype = 1; } namespace M { enum Function { prototype = 2 } export const p = Function.prototype; } class C { constructor(private Function: { prototype: number }) {} p() { return this.Function.prototype; } }",
        "String(Function.prototype + M.p + new C({ prototype: 3 }).p()) + count",
    );
    for result in every(&app) {
        result.expect("TypeScript locals named Function and eval");
    }
    if exact_js::ENGINE_LINKED {
        let warnings = bake(&app.0, &Tools::default()).unwrap().warnings;
        assert!(
            !warnings.iter().any(|w| w.contains("code from a string")),
            "{warnings:?}"
        );
    }
    // A package's identity names a place in the capture: one that would leave it is refused, writing nothing.
    app.put(
        "node_modules/bad/package.json",
        r#"{"name":"bad","version":"../../../../escaped"}"#,
    );
    app.put("node_modules/bad/index.js", "module.exports = 'x';\n");
    app.answer("import bad from 'bad';", "bad + count");
    for result in every(&app) {
        let error = result.expect_err("identity");
        assert!(
            error.contains("not an npm package name and version"),
            "{error}"
        );
    }
    assert!(!std::env::temp_dir().join("escaped").exists() && !app.0.join("escaped").exists());
}

/// What builds no code and throws nothing without Node is not refused, as
/// Effect's package index reaches it: a module export named `Function`,
/// `Function.prototype`'s named members, `typeof globalThis.Function` and
/// `typeof globalThis.eval`, a read of `globalThis.process` through `?.` that
/// is tested, spread, returned or held whole by a `const` whose uses a `typeof`
/// test guards, and a never-called function's `import.meta?.env`, which the
/// bundle drops. A package's `/// <reference lib>` to an ES library the
/// configuration leaves out loads nothing, however it is spelled, so the app
/// cannot name what that library declares. On Hermes the guarded paths answer
/// as without Node.
#[test]
fn feature_tests_and_names_that_build_no_code_are_not_refused() {
    let app = App::new("feature-tests");
    app.put(
        "node_modules/fine/package.json",
        r#"{"name":"fine","version":"1.0.0","type":"module","types":"./index.d.ts"}"#,
    );
    app.put(
        "node_modules/fine/index.js",
        "export * as Function from './fn.js';\n\
         export { Function as Fn } from './fn.js';\n\
         export * as Config from './config.js';\n\
         export { probe } from './probe.js';\n",
    );
    app.put(
        "node_modules/fine/fn.js",
        "export const Function = { twice: (n) => n * 2 };\n",
    );
    app.put(
        "node_modules/fine/config.js",
        "export const fromEnv = () => ({ ...globalThis.process?.env, ...import.meta?.env });\n",
    );
    app.put(
        "node_modules/fine/probe.js",
        "const bound = (f) => Function.prototype.bind.call(f, undefined);\n\
         const clock = globalThis.process?.hrtime;\n\
         const resource = (() => { try { return globalThis.process?.getBuiltinModule?.('node:async_hooks')?.AsyncResource; } catch { return undefined; } })();\n\
         const cwd = () => {\n\
         \x20 const process = globalThis.process;\n\
         \x20 if (typeof process === 'object' && process !== null && typeof process.cwd === 'function') return process.cwd();\n\
         \x20 return '/';\n\
         };\n\
         const tty = () => {\n\
         \x20 const process = globalThis.process;\n\
         \x20 const stdout = typeof process?.stdout === 'object' && process.stdout !== null;\n\
         \x20 return stdout && process.stdout.isTTY === true;\n\
         };\n\
         const time = () => (typeof clock?.bigint === 'function' ? String(clock.bigint()) : 'none');\n\
         const host = globalThis.process;\n\
         const vars = host?.env;\n\
         const home = () => (typeof vars === 'object' ? vars.HOME : vars?.HOME);\n\
         export const probe = (n) => [Object.keys({ ...globalThis.process?.env }).length, typeof clock, typeof resource, time(), cwd(), tty(),\n\
         \x20 bound(() => n)(), bound instanceof Function, typeof globalThis.Function, typeof globalThis.eval, typeof home()].join(' ');\n",
    );
    // TypeScript reads a directive in the comments before the first token, its attributes in any order, a
    // comment ending at any line terminator (a file written with CR alone, the last).
    let spellings = [
        (
            "/// <reference lib=\"esnext.disposable\" preserve=\"true\" />",
            "\n",
        ),
        ("   /// <reference lib=\"esnext.disposable\" />", "\n"),
        ("/// <reference lib = 'esnext.disposable' />", "\n"),
        (
            "/* a licence */\n/// <reference preserve=\"true\" lib=\"ESNext.Disposable\"/>",
            "\n",
        ),
        ("///<reference lib=\"esnext.disposable\"/>", "\n"),
        (
            "// a licence\r/// <reference lib=\"esnext.disposable\" />",
            "\r",
        ),
    ];
    let mut index = String::new();
    for (i, (spelling, end)) in spellings.iter().enumerate() {
        app.put(
            &format!("node_modules/fine/spelling{i}.d.ts"),
            &format!(
                "{spelling}{end}export declare const release{i}: <A extends Disposable>(a: A) => A;{end}"
            ),
        );
        index += &format!("export * from './spelling{i}';\n");
    }
    app.put(
        "node_modules/fine/index.d.ts",
        &format!("{index}export declare const probe: (n: number) => string;\nexport declare const Fn: {{ twice: (n: number) => number }};\n"),
    );
    app.answer(
        "import { probe, Fn } from 'fine';",
        "probe(count) + ' ' + Fn.twice(count + 1)",
    );
    for result in every(&app) {
        result.unwrap();
    }
    if exact_js::ENGINE_LINKED {
        assert_eq!(
            message(&app),
            Value::str("0 undefined undefined none / false 0 true function function undefined 2")
        );
    }
    // The library stays unloaded for the app's own code.
    app.answer(
        "import { probe } from 'fine';",
        "probe(count) + String(Symbol.dispose)",
    );
    for result in every(&app) {
        let error = result.expect_err("Symbol.dispose");
        assert!(
            error.contains("Property 'dispose' does not exist"),
            "{error}"
        );
    }
}

/// A use the static check cannot see (an alias of the global object) finds
/// the DOM absent on Hermes, as the web's module bindings make it on the page.
#[test]
fn an_aliased_dom_global_is_absent_where_the_module_runs() {
    if !exact_js::ENGINE_LINKED {
        return;
    }
    let app = App::new("alias");
    app.put(
        "node_modules/sly/package.json",
        r#"{"name":"sly","version":"1.0.0"}"#,
    );
    app.put("node_modules/sly/index.js", "const g = globalThis, key = ['docu', 'ment'].join('');\nmodule.exports = () => typeof g[key];\n");
    app.put(
        "packages.d.ts",
        "declare module 'sly' { const sly: () => string; export default sly; }\n",
    );
    app.answer("import sly from 'sly';", "sly() + count");
    for result in every(&app) {
        result.unwrap();
    }
    assert_eq!(message(&app), Value::str("undefined0"));
}

/// No build runs app or package code to learn `appId` and `grants`: they are
/// read from app.ts's syntax, and one the build cannot read so is refused by
/// every producer, naming the fix. A `paths` alias that looks like a package
/// resolves in the capture and in both of the web's later bundles.
#[test]
fn app_facts_are_read_without_running_the_module_and_aliases_replay() {
    let app = App::new("facts");
    app.put("src/helper.ts", "export const helper = 'aliased';\n");
    app.put(
        "tsconfig.json",
        r#"{"compilerOptions":{"paths":{"@app/*":["./src/*"]}}}"#,
    );
    app.put(
        "app.ts",
        "import type { Sources, Answer } from './app.contract.d.ts';\nimport { helper } from '@app/helper';\nconst origin = 'http://127.0.0.1:4400';\nexport const appId = 'test.exact.logic';\nexport const grants = [`net.fetch ${origin}`, 'secret.keep logic.key'].join('\\n');\nconst sources: Sources = { message: ([count]) => helper + count };\nexport const answer: Answer = (source, args, store, storage) => sources[source](args, store, storage);\n",
    );
    for result in every(&app) {
        result.unwrap();
    }
    if exact_js::ENGINE_LINKED {
        assert_eq!(message(&app), Value::str("aliased0"));
    }
    let code = std::fs::read_to_string(app.0.join("app.ts")).unwrap();
    app.put(
        "app.ts",
        &code.replace(
            "export const grants = [",
            "const made = () => 'x';\nexport const grants = made() + [",
        ),
    );
    for result in every(&app) {
        let error = result.expect_err("computed grants");
        assert!(
            error.contains("grants must be a constant the build reads without running the module"),
            "{error}"
        );
    }
}

/// The web's later bundles resolve only what the capture recorded: a
/// recorded import (an absolute one too) to its captured file, the host's
/// injected module as itself, and anything else is refused.
#[test]
fn replay_resolves_recorded_imports_and_refuses_the_rest() {
    let app = App::new("replay");
    let stage = app.0.join("stage");
    std::fs::create_dir_all(stage.join("node_modules/.packages/p@1.0.0")).unwrap();
    std::fs::write(stage.join("app.ts"), "").unwrap();
    std::fs::write(stage.join("node_modules/.packages/p@1.0.0/index.js"), "").unwrap();
    std::fs::write(
        stage.join("__exact_packages.json"),
        r#"{"packages":[],"files":[],"digests":{},"edges":[["app.ts","/abs/p/index.js","import","node_modules/.packages/p@1.0.0/index.js"],["app.ts","./local","import","app.ts"]]}"#,
    )
    .unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let checked = exact_bake::bun()
        .args([
            "-e",
            r#"
import assert from 'node:assert/strict';
import { realpathSync } from 'node:fs';
import { resolve } from 'node:path';
const [config, stage] = process.argv.slice(1);
const { replayCaptured } = await import(config);
const real = realpathSync(stage), app = resolve(real, 'app.ts');
const { rolldown } = replayCaptured(stage, ['/host/ts-fetch.js']);
assert.equal(rolldown.resolveId('/abs/p/index.js', app, { kind: 'import-statement' }), resolve(real, 'node_modules/.packages/p@1.0.0/index.js'));
assert.equal(rolldown.resolveId('./local', app, { kind: 'import-statement' }), app);
assert.equal(rolldown.resolveId('/host/ts-fetch.js', app, { kind: 'import-statement' }), null);
assert.equal(rolldown.resolveId('./elsewhere.js', '/outside/file.js', {}), null);
for (const source of ['/abs/other.js', './unrecorded', 'pkg'])
  assert.throws(() => rolldown.resolveId(source, app, { kind: 'import-statement' }), /which the checked capture did not resolve/, source);
"#,
        ])
        .arg(root.join("js/bake/src/typescript.mjs"))
        .arg(&stage)
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
}

/// A module that may ask a transaction to check foreign keys needs the
/// prelude runtime that honours the option (`compat.rs` `TYPESCRIPT_RUNTIME`
/// 10), as an older one would drop it and run the rebuild enforced. Its
/// options can be data (here a migration's, parsed from JSON, with no
/// `foreignKeys` in the bundle), so the `sqlite.open` grant decides.
#[test]
fn a_module_that_may_check_foreign_keys_needs_the_runtime_that_honours_it() {
    if !exact_js::ENGINE_LINKED {
        return;
    }
    for (grants, needs) in [("sqlite.open app:/data/a.db", 10), ("", 0)] {
        let app = App::new(&format!("foreign-keys-{needs}"));
        app.put(
            "app.ts",
            &format!("import type {{ Sources, Answer }} from './app.contract.d.ts';\nexport const appId = 'test.exact.logic';\nexport const grants = '{grants}';\nconst migration = JSON.parse('{{\"commands\": [], \"options\": {{\"foreign\\\\u004beys\": \"check\"}}}}');\nconst sources: Sources = {{ message: ([count], _store, storage) => storage.sqlite.open('app:/data/a.db').then(db => db.transaction(migration.commands, migration.options)).then(() => String(count), () => String(count)) }};\nexport const answer: Answer = (source, args, store, storage) => sources[source](args, store, storage);\n"),
        );
        let baked = bake(&app.0, &Tools::default()).unwrap();
        assert!(!String::from_utf8_lossy(&baked.bytecode).contains("foreignKeys"));
        let receipt: Json = serde_json::from_str(&baked.receipt).unwrap();
        assert_eq!(receipt["typescriptRuntime"], needs, "{grants}");
    }
}

#[test]
fn fetch_options_need_their_runtime_and_an_ordinary_fetch_does_not() {
    if !exact_js::ENGINE_LINKED {
        return;
    }
    for (options, needs) in [
        ("", 0),
        (", { cache: 'no-store' }", 11),
        (", { cache: 'default' }", 11),
        (", { exactPreload: 'feed' }", 13),
        (", { exactPrefetch: true }", 13),
    ] {
        let app = App::new(&format!("fetch-cache-{needs}"));
        app.put("app.ts", &format!("import type {{ Answer }} from './app.contract.d.ts';\nexport const appId = 'test.exact.logic';\nexport const grants = 'net.fetch https://example.test';\nexport const answer: Answer = () => fetch('https://example.test'{options}).then(r => r.text());\n"));
        let baked = bake(&app.0, &Tools::default()).unwrap();
        let receipt: Json = serde_json::from_str(&baked.receipt).unwrap();
        assert_eq!(receipt["typescriptRuntime"], needs, "{options}");
    }
}
