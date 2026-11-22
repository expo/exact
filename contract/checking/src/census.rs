//! A source census, not a replacement for the private checking boundary.
//! Count recursive descent and explicit work stacks in the compiler's source.
//! @ref LLP 1172.013 E1; LLP 1172.019 §2 item 4.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use syn::visit::{self, Visit};

#[derive(Default)]
struct Body {
    view: bool,
    looped: bool,
    calls: BTreeSet<String>,
}

impl Body {
    fn call(&mut self, expr: &syn::Expr) {
        let path = match expr {
            syn::Expr::Path(path) => &path.path,
            syn::Expr::Paren(expr) => return self.call(&expr.expr),
            syn::Expr::Group(expr) => return self.call(&expr.expr),
            syn::Expr::Reference(expr) => return self.call(&expr.expr),
            _ => return,
        };
        let segments: Vec<_> = path.segments.iter().map(|s| s.ident.to_string()).collect();
        let shared = segments.windows(2).any(|pair| {
            pair[0] == "Node"
                && matches!(
                    pair[1].as_str(),
                    "walk" | "walk_with" | "walk_reverse" | "walk_events_with"
                )
        });
        if !shared {
            if let Some(name) = segments.last() {
                self.calls.insert(name.clone());
            }
        }
    }
}

impl<'ast> Visit<'ast> for Body {
    fn visit_path(&mut self, path: &'ast syn::Path) {
        self.view |= path.segments.iter().any(|s| s.ident == "Node");
        visit::visit_path(self, path);
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        self.call(&call.func);
        // Function arguments can recurse through iterator adapters or other helpers.
        for arg in &call.args {
            self.call(arg);
        }
        visit::visit_expr_call(self, call);
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        self.calls.insert(call.method.to_string());
        for arg in &call.args {
            self.call(arg);
        }
        self.view |= matches!(
            call.method.to_string().as_str(),
            "child_lists" | "scoped_child_lists" | "child_lists_mut" | "scoped_child_lists_mut"
        );
        visit::visit_expr_method_call(self, call);
    }

    fn visit_expr_while(&mut self, expr: &'ast syn::ExprWhile) {
        self.looped = true;
        visit::visit_expr_while(self, expr);
    }

    fn visit_expr_loop(&mut self, expr: &'ast syn::ExprLoop) {
        self.looped = true;
        visit::visit_expr_loop(self, expr);
    }

    // A nested function owns its own recursion, not its enclosing function's.
    fn visit_item_fn(&mut self, _: &'ast syn::ItemFn) {}
}

struct Function {
    file: PathBuf,
    name: String,
    body: Body,
}

#[derive(Default)]
struct Functions {
    file: PathBuf,
    entries: Vec<Function>,
}

impl Functions {
    fn source(&mut self, path: &Path, source: &str) {
        self.file = path.to_path_buf();
        self.visit_file(
            &syn::parse_file(source).unwrap_or_else(|e| panic!("{}: {e}", path.display())),
        );
    }

    fn add(&mut self, sig: &syn::Signature, block: &syn::Block) {
        let mut body = Body::default();
        body.visit_signature(sig);
        body.visit_block(block);
        self.entries.push(Function {
            file: self.file.clone(),
            name: sig.ident.to_string(),
            body,
        });
    }

    fn walks(&self) -> Vec<&Function> {
        let by_name: BTreeMap<_, Vec<_>> =
            self.entries
                .iter()
                .enumerate()
                .fold(BTreeMap::new(), |mut map, (i, function)| {
                    map.entry(function.name.as_str()).or_default().push(i);
                    map
                });
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(start, function)| {
                let body = &function.body;
                if !body.view {
                    return None;
                }
                let mut pending: Vec<_> = body.calls.iter().map(String::as_str).collect();
                let mut seen = BTreeSet::new();
                let mut recursive = false;
                while let Some(called) = pending.pop() {
                    if let Some(indices) = by_name.get(called) {
                        for &index in indices {
                            if index == start {
                                recursive = true;
                            }
                            if seen.insert(index) {
                                pending.extend(
                                    self.entries[index].body.calls.iter().map(String::as_str),
                                );
                            }
                        }
                    }
                }
                (recursive || body.looped).then_some(function)
            })
            .collect()
    }
}

impl<'ast> Visit<'ast> for Functions {
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        if item.attrs.iter().any(|a| a.path().is_ident("test")) {
            return;
        }
        self.add(&item.sig, &item.block);
        visit::visit_item_fn(self, item);
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        self.add(&item.sig, &item.block);
        visit::visit_impl_item_fn(self, item);
    }

    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if item.ident != "tests" {
            visit::visit_item_mod(self, item);
        }
    }
}

fn census(root: &Path) -> Vec<String> {
    let mut files = vec![(root.to_path_buf(), root.to_path_buf())];
    let mut crates = BTreeMap::<PathBuf, Functions>::new();
    let mut walks = Vec::new();
    while let Some((path, mut crate_root)) = files.pop() {
        if path.is_dir() {
            // Test fixtures and the shared traversal aren't compiler hand walks.
            if matches!(
                path.file_name().and_then(|p| p.to_str()),
                Some("tests" | "corpus" | "target")
            ) {
                continue;
            }
            if path.join("Cargo.toml").is_file() {
                crate_root = path.clone();
            }
            files.extend(
                std::fs::read_dir(&path)
                    .unwrap()
                    .map(|e| (e.unwrap().path(), crate_root.clone())),
            );
        } else if path.extension().is_some_and(|e| e == "rs") {
            let source = std::fs::read_to_string(&path).unwrap();
            crates.entry(crate_root).or_default().source(&path, &source);
        }
    }
    for functions in crates.values() {
        walks.extend(
            functions
                .walks()
                .into_iter()
                .filter(|f| !f.file.ends_with("syntax/src/walk.rs"))
                .map(|f| {
                    format!(
                        "{}::{}",
                        f.file.strip_prefix(root).unwrap().display(),
                        f.name
                    )
                }),
        );
    }
    walks.sort();
    walks
}

#[test]
fn recursive_view_walks_do_not_grow() {
    let here = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let measured = std::env::var_os("CHECKING_CENSUS_ROOT");
    let root = measured.as_deref().map(Path::new).unwrap_or(here);
    let walks = census(root);
    // Conservative candidates include transformations and same-name call edges.
    // LLP 1172.019 §2 item 4 requires this ceiling to fall as checks migrate.
    const CEILING: usize = 246;
    eprintln!(
        "{} compiler hand walks (ceiling {CEILING}):\n{}",
        walks.len(),
        walks.join("\n")
    );
    assert!(
        walks.len() <= CEILING,
        "new view descent must use contract-checking's traversal"
    );
}

#[test]
fn census_finds_cross_file_recursion_within_each_crate() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("checking-census-{}-{unique}", std::process::id()));
    for (path, source) in [
        ("lower/Cargo.toml", ""),
        (
            "lower/src/sharing.rs",
            "impl Lowerer { fn nodes(&mut self, nodes: &[Node]) { for n in nodes { self.node(n); } } }",
        ),
        (
            "lower/src/lib.rs",
            "impl Lowerer { fn node(&mut self, n: &Node) { self.nodes(n.children()); } }",
        ),
        ("other/Cargo.toml", ""),
        (
            "other/src/lib.rs",
            "impl Lowerer { fn node(&mut self, n: &Node) { self.nodes(n.children()); } }",
        ),
    ] {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }
    let walks = census(&root);
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(
        walks,
        ["lower/src/lib.rs::node", "lower/src/sharing.rs::nodes"]
            .map(|path| path.replace('/', std::path::MAIN_SEPARATOR_STR))
    );
}

#[test]
fn census_finds_direct_mutual_and_iterative_walks_but_not_shared_traversal() {
    let file = syn::parse_file(r#"
        fn direct(nodes: &[Node]) { for n in nodes { direct(n.children()); } }
        fn first(nodes: &[Node]) { second(nodes); }
        fn second(nodes: &[Node]) { first(nodes); }
        fn stacked(nodes: &[Node]) { let mut stack = nodes.to_vec(); while let Some(n) = stack.pop() { stack.extend(n.children()); } }
        fn shared(nodes: &[Node]) { for n in Node::walk(nodes) { check(n); } }
        fn walk(nodes: &[Node]) { for n in Node::walk(nodes) { check(n); } }
        fn walk_with(nodes: &[Node]) { Node::walk_with(nodes, (), |_, _| [Some(()), Some(())]); }
        fn walk_reverse(nodes: &[Node]) { for n in Node::walk_reverse(nodes) { check(n); } }
        fn walk_events_with(nodes: &[Node]) { Node::walk_events_with(nodes, (), |_, _| [Some(()), Some(())]); }
    "#).unwrap();
    let mut functions = Functions::default();
    functions.visit_file(&file);
    assert_eq!(
        functions
            .walks()
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>(),
        ["direct", "first", "second", "stacked"]
    );
}

#[test]
fn census_finds_callback_recursion_but_not_shared_traversal() {
    let file = syn::parse_file(
        r#"
        fn pointed(n: &Node) -> bool { n.children().any(pointed) }
        fn every(n: &Node) -> bool { n.children().all(every) }
        fn each(n: &Node) { n.children().for_each(each); }
        fn found(n: &Node) -> Option<String> { n.children().find_map(found) }
        fn mapped(n: &Node) -> Vec<String> { n.children().flat_map(mapped).collect() }
        fn qualified(n: &Node) -> bool { n.children().any(crate::qualified) }
        fn associated(n: &Node) -> bool { Iterator::any(n.children(), associated) }
        fn parenthesized(n: &Node) -> bool { n.children().any((parenthesized)) }
        fn borrowed(n: &Node) -> bool { n.children().any(&borrowed) }
        fn first(n: &Node) -> bool { n.children().any(second) }
        fn second(n: &Node) -> bool { n.children().all(first) }
        fn leaf(n: &Node) -> bool { n.tag() == "div" }
        fn nonrecursive(n: &Node) -> bool { n.children().any(leaf) }
        fn walk(nodes: &[Node]) { nodes.iter().map(Node::walk); }
        fn walk_with(nodes: &[Node]) { pass(nodes, Node::walk_with); }
    "#,
    )
    .unwrap();
    let mut functions = Functions::default();
    functions.visit_file(&file);
    assert_eq!(
        functions
            .walks()
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>(),
        [
            "pointed",
            "every",
            "each",
            "found",
            "mapped",
            "qualified",
            "associated",
            "parenthesized",
            "borrowed",
            "first",
            "second"
        ]
    );
}
