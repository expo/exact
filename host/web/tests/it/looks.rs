//! LLP 1121 D8 (stage 8 B): the wasm host writes a node's `:hover` and
//! `:focus-visible` looks as page rules (`looks`), again when a token they
//! read moves live; `:active` is the node's press paint (its scale, its
//! background, its absolute opacity). A node with no looks writes none.

use exact_runner::{design, DataError, DataSource, Value};
use exact_web::Host;

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, s: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(s.into()))
    }
}

const VIEW: &str = "component App\n  state n = 0\n  action go\n    n = n + 1\n  view\n    column\n      column testId=\"card\" press=go background-color=\"#ffffff\"\n        :active\n          background-color = \"#0000ff\"\n          opacity = 0.6\n          -exact-press-scale = 0.95\n        :hover\n          border-color = \"#00ff00\"\n        :focus-visible\n          background-color = \"accent.soft\"\n        text \"c\"\n      column testId=\"plain\" press=go\n        text \"p\"\n";

fn plan(dir: &std::path::Path, soft: &str) -> exact_plan::Plan {
    std::fs::write(
        dir.join("app.tamber"),
        format!("tamber t\n\ntheme\n  accent.soft = {soft}\n"),
    )
    .unwrap();
    std::fs::write(dir.join("app.contract"), VIEW).unwrap();
    contract::compile_path(&dir.join("app.contract")).unwrap_or_else(|e| panic!("{e:?}"))
}

/// The op that creates the element with `test_id`, to the next op.
fn created<'a>(batch: &'a str, test_id: &str) -> &'a str {
    let at = batch
        .find(&format!("\"data-testid\":\"{test_id}\""))
        .unwrap_or_else(|| panic!("no {test_id} in {batch}"));
    let start = batch[..at].rfind("{\"op\":").unwrap();
    let end = batch[at..].find("{\"op\":").map_or(batch.len(), |n| at + n);
    &batch[start..end]
}

#[test]
fn a_nodes_looks_are_page_rules_that_follow_a_live_token() {
    let dir = std::env::temp_dir().join(format!("exact-web-looks-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let a = plan(&dir, "#eeeeee");
    let b = plan(&dir, "#ff0000");
    let _ = std::fs::remove_dir_all(&dir);
    let (mut host, boot) = Host::boot(&a.encode(), NoData, Default::default(), "/").unwrap();
    let card = created(&boot, "card");
    assert!(
        card.contains("--exact-press:0.95"),
        "the press's scale: {card}"
    );
    assert!(
        card.contains("--exact-press-opacity-to:0.6"),
        "its absolute opacity: {card}"
    );
    assert!(
        card.contains("--exact-press-background-color:rgba(0,0,255,1)"),
        "{card}"
    );
    let plain = created(&boot, "plain");
    assert!(!plain.contains("--exact-press"), "{plain}");
    let looks: Vec<&str> = boot
        .match_indices("{\"op\":\"looks\"")
        .map(|(at, _)| &boot[at..])
        .collect();
    assert_eq!(looks.len(), 1, "only the card has looks: {boot}");
    let rules = looks[0];
    let hover = rules.find(":hover").expect("a hover rule");
    let focus = rules.find(":focus-visible").expect("a focus rule");
    assert!(hover < focus, "the focus look wins: {rules}");
    assert!(
        !rules.contains("data-pressed"),
        "`:active` is no look: {rules}"
    );
    assert!(
        rules.contains("background-color:rgba(238,238,238,1)!important"),
        "{rules}"
    );
    assert!(
        rules.contains("border-top-color:rgba(0,255,0,1)!important"),
        "{rules}"
    );
    let batch =
        design::batch(&design::shape(&a), &design::tokens(&a), &design::tokens(&b)).unwrap();
    let out = host.apply_live(&batch.json());
    assert!(out.starts_with("{\"live\":\"applied\""), "{out}");
    assert!(
        out.contains("{\"op\":\"looks\""),
        "the look is written again: {out}"
    );
    assert!(
        out.contains("background-color:rgba(255,0,0,1)!important"),
        "{out}"
    );
}

/// A text with a look of its own keeps its box: it never folds into its
/// parent's (LLP 1007.001), where its look would paint nothing.
#[test]
fn a_text_with_a_look_keeps_its_box() {
    use exact_web::document::{checkpoint, digest, project_tree};

    exact_web::link(exact_web_capabilities::ALL);
    for state in [None, Some("hover"), Some("focus-visible")] {
        let mut src = String::from(
            "routes nav\n  home \"/\" render=build\n\ncomponent App\n  view\n    column testId=\"box\"\n      text \"hi\"\n",
        );
        if let Some(state) = state {
            src.push_str(&format!(
                "        :{state}\n          background-color = \"#00ff00\"\n"
            ));
        }
        let plan = contract::compile(&src).unwrap_or_else(|e| panic!("{e:?}"));
        let (mut host, boot) = Host::boot(&plan.encode(), NoData, Default::default(), "/").unwrap();
        let kernel = host.runner().kernel();
        let parent = kernel
            .node_by_key(kernel.find_by_test_id("box")[0])
            .unwrap();
        let (parent, text) = (parent.id, parent.children()[0]);
        let folds = state.is_none();
        let check_batch = |batch: &str| {
            let text_op = batch
                .split("{\"op\":\"create\",")
                .find(|op| op.starts_with(&format!("\"id\":{text},")))
                .unwrap()
                .split("{\"op\":")
                .next()
                .unwrap();
            assert_eq!(text_op.contains("display:contents"), folds, "{text_op}");
            let parent_op = created(batch, "box");
            assert_eq!(parent_op.contains("display:block;"), folds, "{parent_op}");
        };
        check_batch(&boot);
        for (id, display) in [(text, "display:contents;"), (parent, "display:block;")] {
            let parts =
                exact_web::host::template::parts_with(kernel, &plan, id, true, true).unwrap();
            assert_eq!(parts.css.contains(display), folds, "{parts:?}");
        }
        let doc = host.document().unwrap();
        let tree = host.runner_mut().document_tree().unwrap();
        let (direct, _) = project_tree(&tree, host.runner(), Default::default()).unwrap();
        for (path, root) in [("kernel", &doc.root), ("direct", &direct.root)] {
            for (id, display) in [(text, "display:contents;"), (parent, "display:block;")] {
                let at = root.find(&format!(" data-view=\"{id}\"")).unwrap();
                let start = root[..at].rfind('<').unwrap();
                let end = at + root[at..].find('>').unwrap();
                let tag = &root[start..end];
                assert_eq!(tag.contains(display), folds, "{path} {state:?}: {tag}");
            }
        }
        let page = checkpoint(host.runner(), "/");
        let written = digest(&plan.encode(), "/", &page, &doc.root);
        let (_, batch) = Host::boot_checkpoint(
            &plan.encode(),
            NoData,
            &page,
            &written,
            Vec::new(),
            None,
            Default::default(),
            "/",
        )
        .unwrap();
        assert!(batch.contains("\"adopted\":true"), "{batch}");
        check_batch(&batch);
        assert_eq!(batch.contains("{\"op\":\"looks\""), !folds, "{batch}");
    }
}

/// The page's look rules of view `id` in `batch`, if it sent them.
fn looks_of(batch: &str, id: u32) -> Option<&str> {
    let at = batch.find(&format!("{{\"op\":\"looks\",\"id\":{id},"))?;
    let end = batch[at + 1..]
        .find("{\"op\":")
        .map_or(batch.len(), |n| at + 1 + n);
    Some(&batch[at..end])
}

/// LLP 1121 D7, D8: a look's and a press's colour reach a mode or
/// `color-scheme` boundary below their node, which writes the colour around
/// it again and would stop them, as on the JS target. Each is resolved in the
/// boundary's tuple, the nearest last (so it holds over one further out), up
/// to the nearest view that sets its colour; a boundary that sets its own
/// colour takes none, and one whose scheme is bound and unset is still one.
/// The boundary names the view whose colour it writes again
/// (`data-exact-from`), when it writes one again. Its rules follow a live
/// token, the window's modes and its own colour, and a boundary mounted later
/// takes them.
#[test]
fn a_looks_and_a_press_colour_reach_a_boundary_in_the_boundarys_tuple() {
    let view = "component App\n  state shown = false\n  state scheme = \"unset\"\n  state colour = \"unset\"\n  action show\n    shown = true\n    scheme = \"dark\"\n    colour = \"#123456\"\n  view\n    column\n      column testId=\"outer\"\n        :hover\n          color = \"lo\"\n        link href=\"/a\" testId=\"link\" color=\"nav\" press=show\n          :hover\n            color = \"hi\"\n          :active\n            color = \"hi\"\n          column color-scheme=\"dark\" testId=\"dark\"\n            text \"d\"\n          column color-scheme=\"dark\" testId=\"own\" color=\"#0000ff\"\n            text \"o\"\n          column color-scheme=scheme testId=\"bound\" color=colour\n            text \"b\"\n          column testId=\"card\" press=show\n            :hover\n              color = \"lo\"\n            :active\n              color = \"#123123\"\n            column -exact-mode=\"ink=plain\" testId=\"pinned\"\n              text \"p\"\n          if shown\n            column color-scheme=\"dark\" testId=\"later\"\n              text \"l\"\n      link href=\"/b\" color=\"#666666\"\n        :hover\n          color = \"hi\"\n        column color-scheme=\"dark\" testId=\"literal\"\n          text \"x\"\n";
    let dir = std::env::temp_dir().join(format!("exact-web-looks-below-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("app.contract"), view).unwrap();
    let plan = |hi: &str| {
        let design = format!("tamber t\n\naxes\n  ink = tint | plain\n\ntheme\n  nav = light-dark(#666666, #ffffff)\n  hi = {hi}\n    when ink=plain: #00aa00\n  lo = #0000cc\n    when ink=plain: #00cccc\n");
        std::fs::write(dir.join("app.tamber"), design).unwrap();
        contract::compile_path(&dir.join("app.contract")).unwrap_or_else(|e| panic!("{e:?}"))
    };
    let (a, b) = (plan("#cc0000"), plan("#dd0000"));
    let _ = std::fs::remove_dir_all(&dir);
    let (mut host, boot) = Host::boot(&a.encode(), NoData, Default::default(), "/").unwrap();
    let id = |host: &Host<NoData>, test_id: &str| {
        let kernel = host.runner().kernel();
        kernel
            .node_by_key(kernel.find_by_test_id(test_id)[0])
            .unwrap()
            .id
    };
    let [outer, link, card, dark, bound, pinned, literal] = [
        "outer", "link", "card", "dark", "bound", "pinned", "literal",
    ]
    .map(|t| id(&host, t));
    let on = |view: u32, colour: &str| {
        format!("[data-view=\\\"{view}\\\"]{{color:{colour}!important;}}")
    };
    let hovering = |holder: u32| format!("[data-view=\\\"{holder}\\\"]:hover");
    let pressed =
        |holder: u32, colour: &str| format!("--exact-press-color-below-{holder}:{colour};");
    let from = |view: u32, holder: u32| {
        format!("{{\"op\":\"props\",\"id\":{view},\"set\":{{\"data-exact-from\":\"{holder}\"}}")
    };
    let rules = |batch: &str, view: u32| {
        looks_of(batch, view)
            .unwrap_or_else(|| panic!("{view} takes the looks above: {batch}"))
            .to_string()
    };

    // In the boundary's tuple: the scheme boundary's `hi` is the window's, the pin's is `ink=plain`'s.
    let dark_rules = rules(&boot, dark);
    assert!(
        dark_rules.contains(&hovering(link)) && dark_rules.contains(&on(dark, "rgba(204,0,0,1)")),
        "{dark_rules}"
    );
    assert!(
        dark_rules.contains(&pressed(link, "rgba(204,0,0,1)")),
        "{dark_rules}"
    );
    assert!(
        !dark_rules.contains(&hovering(outer)),
        "the link sets its colour, so the look above it stops there: {dark_rules}"
    );
    let pinned_rules = rules(&boot, pinned);
    let (far, near) = (
        pinned_rules.find(&on(pinned, "rgba(0,170,0,1)")),
        pinned_rules.find(&on(pinned, "rgba(0,204,204,1)")),
    );
    assert!(
        far.zip(near).is_some_and(|(f, n)| f < n),
        "the nearest look last, each in ink=plain: {pinned_rules}"
    );
    assert!(pinned_rules.contains(&hovering(card)), "{pinned_rules}");
    assert!(
        pinned_rules.contains(&pressed(card, "initial")),
        "a literal press colour is the pressed view's own: {pinned_rules}"
    );
    assert!(
        pinned_rules.contains(&pressed(link, "rgba(0,170,0,1)")),
        "the press in ink=plain: {pinned_rules}"
    );
    // Bound and unset, it still writes the colour around it again.
    assert!(
        rules(&boot, bound).contains(&on(bound, "rgba(204,0,0,1)")),
        "{boot}"
    );
    for view in [dark, pinned, bound] {
        assert!(
            boot.contains(&from(view, link)),
            "{view} writes the link's colour again: {boot}"
        );
    }
    // Under a literal colour it writes none again: no `data-exact-from`, but the look's token in its tuple.
    assert!(
        rules(&boot, literal).contains(&on(literal, "rgba(204,0,0,1)")),
        "{boot}"
    );
    assert!(
        !boot.contains(&format!("{{\"op\":\"props\",\"id\":{literal},")),
        "{boot}"
    );
    assert!(
        looks_of(&boot, id(&host, "own")).is_none(),
        "its own colour stands: {boot}"
    );

    // A look above moves (a live token), and nothing of the boundary's own: its rule follows.
    let live = design::batch(&design::shape(&a), &design::tokens(&a), &design::tokens(&b)).unwrap();
    let out = host.apply_live(&live.json());
    assert!(
        rules(&out, dark).contains(&on(dark, "rgba(221,0,0,1)")),
        "{out}"
    );
    // The window's modes move: `hi` is `ink=plain`'s.
    let out = host.set_modes(&[("ink", "plain")]);
    assert!(
        rules(&out, dark).contains(&on(dark, "rgba(0,170,0,1)")),
        "{out}"
    );

    // The bound boundary sets its own colour: its rules and `data-exact-from` go. A boundary mounted later takes them.
    let shown = host.dispatch_at(link, exact_runner::Event::Press, 20.0);
    assert!(
        shown.contains(&format!("{{\"op\":\"looks\",\"id\":{bound},\"css\":\"\"}}")),
        "{shown}"
    );
    assert!(
        shown.contains(&format!(
            "{{\"op\":\"props\",\"id\":{bound},\"set\":{{}},\"clear\":[\"data-exact-from\"]}}"
        )),
        "{shown}"
    );
    let later = id(&host, "later");
    assert!(
        rules(&shown, later).contains(&on(later, "rgba(0,170,0,1)")),
        "{shown}"
    );
}
