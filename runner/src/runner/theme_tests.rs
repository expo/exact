//! A Markdown node's faces in the theme index (LLP 1045.000 D2, LLP 1121
//! D7): read again when a pin above them, the mode or a token moves; kept
//! as they were when a read again finds a colour the first read took is
//! none, and never for one the first read refused; and gone from the index
//! when they read no token, are cleared, or their node is unmounted.
//! Textarea decorations also keep paint in the index while their field lives.

use crate::{DataError, DataSource, Event, Runner};
use exact_kernel::{Kernel, PropId, ViewId};
use exact_plan::Value;

struct NoData;

impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

/// A design with a token whose slice the `dark-style` axis picks, and one
/// that does not vary.
const MODES: &str = "forma t\n\naxes\n  dark-style = black | dim  under scheme=dark\n\ntheme\n  ink = light-dark(#111111, #eeeeee)\n    when dark-style=dim: light-dark(#111111, #cccccc)\n  brand = #c8102e\n";

/// `contract`'s plan beside the design.
fn plan(name: &str, contract: &str) -> exact_plan::Plan {
    plan_with_design(name, contract, MODES)
}

fn plan_with_design(name: &str, contract: &str, forma: &str) -> exact_plan::Plan {
    let dir = crate::TempDir(
        std::env::temp_dir().join(format!("exact2-faces-{name}-{}", std::process::id())),
    );
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("app.forma"), forma).unwrap();
    std::fs::write(dir.join("app.contract"), contract).unwrap();
    let plan = contract::compile_path(&dir.join("app.contract")).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    plan
}

fn boot(plan: exact_plan::Plan) -> Runner<NoData> {
    Runner::boot(
        plan,
        NoData,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

fn view(r: &Runner<NoData>, test: &str) -> Option<ViewId> {
    let k = r.kernel();
    let key = *k.find_by_test_id(test).first()?;
    Some(k.node_by_key(key).unwrap().id)
}

fn press(r: &mut Runner<NoData>, test: &str) {
    let id = view(r, test).unwrap();
    r.dispatch(id, Event::Press).unwrap();
}

const DECORATED_FIELD: &str = r#"decoration painted color="ink"
component App
  state shown = false
  derive marks = TextDecorations(value="abcd", spans=[TextDecorationSpan(name="painted", start=0, end=4)])
  action toggle
    shown = not shown
  view
    column
      button "Toggle" press=toggle testId="toggle"
      if shown
        column
          textarea value="abcd" decorations=marks testId="editor"
"#;

fn decoration_styles(r: &Runner<NoData>, field: ViewId) -> &str {
    r.kernel
        .node(field)
        .unwrap()
        .props
        .str(PropId::DecorationStyles)
        .unwrap()
}

#[test]
fn decoration_colours_use_the_runners_roles() {
    use exact_kernel::style::roles;
    use exact_kernel::{Color, ColorValue};

    let mut first = boot(plan_with_design(
        "decoration-roles",
        DECORATED_FIELD,
        "forma t\n\naxes\n  palette = paper | night\n\ntheme\n  ink = CanvasText\n    when palette=night: #eeeeee\n",
    ));
    let second = boot(plan("decoration-other", DECORATED_FIELD));
    let canvas = roles::role("CanvasText").unwrap();
    first
        .roles()
        .set(&[(canvas, ColorValue::Fixed(Color::parse("#ff0060").unwrap()))]);
    second
        .roles()
        .set(&[(canvas, ColorValue::Fixed(Color::parse("#0060ff").unwrap()))]);

    press(&mut first, "toggle");
    let field = view(&first, "editor").unwrap();
    assert!(decoration_styles(&first, field).contains("#ff0060ff"));

    let _other = second.bind_roles();
    first.set_mode("palette", "night").unwrap();
    assert!(decoration_styles(&first, field).contains("#eeeeeeff"));
    first.set_mode("palette", "paper").unwrap();
    assert!(decoration_styles(&first, field).contains("#ff0060ff"));
    first.set_token("ink", "#3300ff", &[]).unwrap();
    assert!(decoration_styles(&first, field).contains("#3300ffff"));
    first.set_token("ink", "CanvasText", &[]).unwrap();
    assert!(decoration_styles(&first, field).contains("#ff0060ff"));
    assert!(std::rc::Rc::ptr_eq(&roles::bound(), second.roles()));
}

#[test]
fn decoration_colours_refresh_when_role_overrides_change() {
    let mut r = boot(plan_with_design(
        "decoration-role-edits",
        DECORATED_FIELD,
        "forma t\n\ntheme\n  ink = #ff0060\n  CanvasText = #010203\n  GrayText = #123456\n",
    ));
    press(&mut r, "toggle");
    let field = view(&r, "editor").unwrap();
    r.set_token("ink", "CanvasText", &[]).unwrap();
    assert!(decoration_styles(&r, field).contains("#010203ff"));
    r.set_token("CanvasText", "#0060ff", &[]).unwrap();
    assert!(decoration_styles(&r, field).contains("#0060ffff"));
    r.set_token("CanvasText", "GrayText", &[]).unwrap();
    assert!(decoration_styles(&r, field).contains("#123456ff"));
    r.set_token("GrayText", "#abcdef", &[]).unwrap();
    assert!(decoration_styles(&r, field).contains("#abcdefff"));
}

#[test]
fn decoration_colours_are_released_when_their_subtree_retires() {
    let mut r = boot(plan_with_design(
        "decoration-retire",
        DECORATED_FIELD,
        "forma t\n\ntheme\n  ink = oklch(60% 0.2 30)\n",
    ));
    press(&mut r, "toggle");
    let field = view(&r, "editor").unwrap();
    assert!(decoration_styles(&r, field).contains("oklch("));
    assert!(r.ids.theme.decorations.contains_key(&field));
    press(&mut r, "toggle");
    assert!(view(&r, "editor").is_none());
    assert!(!r.ids.theme.decorations.contains_key(&field));
}

#[test]
fn decoration_colours_kept_after_a_refusal_are_in_the_document() {
    let mut r = boot(plan_with_design(
        "decoration-document",
        DECORATED_FIELD,
        "forma t\n\ntheme\n  ink = #ff0060\n",
    ));
    press(&mut r, "toggle");
    let field = view(&r, "editor").unwrap();
    assert!(decoration_styles(&r, field).contains("#ff0060ff"));
    r.set_token("ink", "12px", &[]).unwrap();
    let doc = r.document_tree().unwrap();
    assert_eq!(
        doc.node(field).unwrap().props.str(PropId::DecorationStyles),
        Some(decoration_styles(&r, field)),
    );
}

/// The editor's faces, or `None` when its prop is unset.
fn faces(r: &Runner<NoData>) -> Option<serde_json::Value> {
    let k = r.kernel();
    let node = k.node_by_key(k.find_by_test_id("editor")[0]).unwrap();
    let text = node.props.str(PropId::AtomFaces)?;
    Some(serde_json::from_str(text).unwrap())
}

fn paint(r: &Runner<NoData>, key: &str) -> String {
    faces(r).unwrap()[0][key].as_str().unwrap().to_string()
}

/// The views whose faces the index keeps.
fn indexed(r: &Runner<NoData>) -> Vec<ViewId> {
    r.ids.theme.faces().map(|(view, _)| view).collect()
}

const FACE: &str =
    "AtomFace(source=\"@a\", label=\"\", icon=\"\", title=\"\", direction=\"\", ariaLabel=\"\"";

/// A pin above the node that moves reads its faces again in the slice the
/// pin makes, though nothing they read changed (D7, as a row's text).
#[test]
fn a_pin_above_reads_a_faces_colour_again() {
    let mut r = boot(plan(
        "pin",
        &format!("atom mention prefix=\"@\"\ncomponent App\n  state dim = false\n  derive faces = [{FACE}, color=\"ink\", backgroundColor=\"\")]\n  action flip\n    dim = not dim\n  view\n    column\n      button press=flip testId=\"flip\"\n        text \"flip\"\n      column -exact-mode=(dim ? \"dark-style=dim\" : \"dark-style=black\")\n        textarea value=\"@a \" markup=\"markdown\" atoms=\"mention\" atomFaces=faces testId=\"editor\"\n"),
    ));
    assert_eq!(paint(&r, "color"), "light-dark(#111111ff, #eeeeeeff)");
    press(&mut r, "flip");
    assert_eq!(paint(&r, "color"), "light-dark(#111111ff, #ccccccff)");
    press(&mut r, "flip");
    assert_eq!(paint(&r, "color"), "light-dark(#111111ff, #eeeeeeff)");
}

/// An app whose editor's faces paint `color` and `backgroundColor`, with a
/// button that switches the app's mode to the token's dim slice.
fn painted(name: &str, color: &str, background: &str, head: &str) -> Runner<NoData> {
    boot(plan(
        name,
        &format!("{head}atom mention prefix=\"@\"\ncomponent App\n  derive faces = [{FACE}, color=\"{color}\", backgroundColor=\"{background}\")]\n  action dim\n    setMode(\"dark-style\", \"dim\")\n  view\n    column\n      button press=dim testId=\"dim\"\n        text \"dim\"\n      textarea value=\"@a \" markup=\"markdown\" atoms=\"mention\" atomFaces=faces testId=\"editor\"\n"),
    ))
}

/// The journal's face lines.
fn said(r: &Runner<NoData>) -> Vec<String> {
    let lines = r
        .journal()
        .filter(|l| l.contains("invalid atomFaces colour"));
    lines.map(str::to_string).collect()
}

/// A token edited to a value that is no colour leaves the faces that read
/// it as they were, as a row its parse refuses keeps its value, and the
/// journal names the face's colour, on that edit and on each read again
/// while it is none (a mode switch); a colour again sets them, in the
/// tuple the node is in now.
#[test]
fn a_token_that_is_no_colour_leaves_the_faces_as_they_were() {
    let mut r = painted("no-colour", "ink", "brand", "");
    let before = faces(&r);
    let (_, set) = r.set_token("brand", "12px", &[]).unwrap();
    assert_eq!(set, 0);
    assert_eq!(faces(&r), before, "the faces stay as they were");
    press(&mut r, "dim");
    assert_eq!(
        faces(&r),
        before,
        "and when the mode moves while it is none"
    );
    let editor = view(&r, "editor").unwrap();
    let kept =
        format!("view {editor}: invalid atomFaces colour \"brand\"; its faces stay as they were");
    let lines = said(&r);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines.iter().all(|l| l.ends_with(&kept)), "{lines:?}");
    let (_, set) = r.set_token("brand", "#00ff00", &[]).unwrap();
    assert_eq!(set, 1);
    assert_eq!(paint(&r, "backgroundColor"), "#00ff00ff");
    assert_eq!(
        paint(&r, "color"),
        "light-dark(#111111ff, #ccccccff)",
        "in the mode's slice"
    );
}

/// A colour the faces' first read refused, said once there, keeps no edit
/// from them: beside a token, a mode switch and a token's edit set the
/// token's colour and leave the refused one empty, with no other line.
/// So for a colour in a profile the app declares (LLP 1100 D3): the first
/// read refuses it, and every read again does, as the first.
#[test]
fn a_refused_colour_beside_a_token_blocks_no_edit() {
    let press_icc =
        "color-profile --press src=\"assets/press.icc\" rendering-intent=\"perceptual\"\n";
    for (name, refused, head) in [
        ("beside", "notacolour", ""),
        ("declared", "color(--press 0.1 0.8 0.2)", press_icc),
    ] {
        let mut r = painted(name, "ink", refused, head);
        let editor = view(&r, "editor").unwrap();
        let first = [format!(
            "view {editor}: invalid atomFaces colour {refused:?}; its kind's paint stands"
        )];
        let ends = |lines: Vec<String>| lines.len() == 1 && lines[0].ends_with(&first[0]);
        assert_eq!(paint(&r, "backgroundColor"), "", "{name}");
        assert!(ends(said(&r)), "{name}: {:?}", said(&r));
        press(&mut r, "dim");
        assert_eq!(
            paint(&r, "color"),
            "light-dark(#111111ff, #ccccccff)",
            "{name}"
        );
        assert_eq!(paint(&r, "backgroundColor"), "", "{name}");
        let (_, set) = r.set_token("ink", "#0000ff", &[]).unwrap();
        assert_eq!(set, 1, "{name}");
        assert_eq!(paint(&r, "color"), "#0000ffff", "{name}");
        assert_eq!(paint(&r, "backgroundColor"), "", "{name}");
        assert!(ends(said(&r)), "{name}: said once: {:?}", said(&r));
    }
}

/// `test`'s faces' `key` colour.
fn paint_of(r: &Runner<NoData>, test: &str, key: &str) -> String {
    let k = r.kernel();
    let node = k.node_by_key(k.find_by_test_id(test)[0]).unwrap();
    let faces: serde_json::Value =
        serde_json::from_str(node.props.str(PropId::AtomFaces).unwrap()).unwrap();
    faces[0][key].as_str().unwrap().to_string()
}

/// What the faces as last set refused decides a read again: a colour they
/// took that is none now keeps them, wherever they were first read. Here
/// B's first read refuses `brand`, a later edit sets B with it green, and
/// the next edit to no colour keeps B green and says so, as it keeps A.
#[test]
fn a_read_again_weighs_the_faces_as_last_set() {
    let mut r = boot(plan(
        "as-set",
        &format!("atom mention prefix=\"@\"\ncomponent App\n  state shown = false\n  derive faces = [{FACE}, color=\"ink\", backgroundColor=\"brand\")]\n  action show\n    shown = true\n  view\n    column\n      button press=show testId=\"show\"\n        text \"show\"\n      textarea value=\"@a \" markup=\"markdown\" atoms=\"mention\" atomFaces=faces testId=\"a\"\n      if shown\n        textarea value=\"@a \" markup=\"markdown\" atoms=\"mention\" atomFaces=faces testId=\"b\"\n"),
    ));
    r.set_token("brand", "12px", &[]).unwrap();
    press(&mut r, "show");
    assert_eq!(
        paint_of(&r, "b", "backgroundColor"),
        "",
        "B's first read refuses it"
    );
    let (_, set) = r.set_token("brand", "#00ff00", &[]).unwrap();
    assert_eq!(set, 2);
    for test in ["a", "b"] {
        assert_eq!(paint_of(&r, test, "backgroundColor"), "#00ff00ff", "{test}");
    }
    let before = said(&r).len();
    let (_, set) = r.set_token("brand", "12px", &[]).unwrap();
    assert_eq!(set, 0);
    for test in ["a", "b"] {
        assert_eq!(
            paint_of(&r, test, "backgroundColor"),
            "#00ff00ff",
            "{test} stays"
        );
    }
    let lines = said(&r);
    assert_eq!(lines.len(), before + 2, "{lines:?}");
    for test in ["a", "b"] {
        let line = format!(
            "view {}: invalid atomFaces colour \"brand\"; its faces stay as they were",
            view(&r, test).unwrap()
        );
        assert!(
            lines[before..].iter().any(|l| l.ends_with(&line)),
            "{test}: {lines:?}"
        );
    }
}

/// Faces whose colours read no token now, here a colour from state, leave
/// the index, and a token's edit sets nothing; reading one again puts them
/// back.
#[test]
fn faces_that_read_no_token_leave_the_index() {
    let mut r = boot(plan(
        "plain",
        &format!("atom mention prefix=\"@\"\ncomponent App\n  state c = \"ink\"\n  derive faces = [{FACE}, color=c, backgroundColor=\"\")]\n  action plain\n    c = \"#ff0000\"\n  action token\n    c = \"ink\"\n  view\n    column\n      button press=plain testId=\"plain\"\n        text \"plain\"\n      button press=token testId=\"token\"\n        text \"token\"\n      textarea value=\"@a \" markup=\"markdown\" atoms=\"mention\" atomFaces=faces testId=\"editor\"\n"),
    ));
    let editor = view(&r, "editor").unwrap();
    assert_eq!(indexed(&r), [editor]);
    press(&mut r, "plain");
    assert!(indexed(&r).is_empty());
    let (_, set) = r.set_token("ink", "#0000ff", &[]).unwrap();
    assert_eq!(set, 0);
    assert_eq!(paint(&r, "color"), "#ff0000ff");
    press(&mut r, "token");
    assert_eq!(indexed(&r), [editor]);
    assert_eq!(paint(&r, "color"), "#0000ffff");
}

/// An unmounted node's faces are set by no token's edit, and leave the
/// index with the node's other entries (`Ids::retain`, which a commit runs
/// once destroyed views outnumber live ones).
#[test]
fn an_unmounted_nodes_faces_leave_the_index() {
    let mut r = boot(plan(
        "unmount",
        &format!("atom mention prefix=\"@\"\ncomponent App\n  state shown = true\n  derive faces = [{FACE}, color=\"ink\", backgroundColor=\"\")]\n  action hide\n    shown = not shown\n  view\n    column\n      button press=hide testId=\"hide\"\n        text \"hide\"\n      if shown\n        textarea value=\"@a \" markup=\"markdown\" atoms=\"mention\" atomFaces=faces testId=\"editor\"\n"),
    ));
    let editor = view(&r, "editor").unwrap();
    assert_eq!(indexed(&r), [editor]);
    press(&mut r, "hide");
    assert!(view(&r, "editor").is_none());
    let (_, set) = r.set_token("ink", "#0000ff", &[]).unwrap();
    assert_eq!(set, 0, "a retired view is not set");
    let kernel = &r.kernel;
    r.ids.retain(|view| kernel.node(view).is_some());
    assert!(indexed(&r).is_empty());
}

/// Faces the bridge cannot read are cleared, journalled, and leave the
/// index. A compiled plan's records always read; this one's `broken` is
/// made text, as a plan from another binary's compiler might hold.
#[test]
fn cleared_faces_leave_the_index() {
    let mut plan = plan(
        "clear",
        &format!("atom mention prefix=\"@\"\ncomponent App\n  state bad = false\n  state broken: AtomFace[] = []\n  state word = \"text\"\n  derive faces = [{FACE}, color=\"ink\", backgroundColor=\"\")]\n  action spoil\n    bad = true\n  view\n    column\n      button press=spoil testId=\"spoil\"\n        text word\n      textarea value=\"@a \" markup=\"markdown\" atoms=\"mention\" atomFaces=(bad ? broken : faces) testId=\"editor\"\n"),
    );
    let slot = |plan: &exact_plan::Plan, name: &str| {
        plan.slots
            .iter()
            .position(|s| plan.str(s.name) == name)
            .unwrap()
    };
    let (broken, word) = (slot(&plan, "broken"), slot(&plan, "word"));
    plan.slots[broken].init = plan.slots[word].init;
    plan.slots[broken].ty = plan.slots[word].ty;
    let mut r = boot(plan);
    let editor = view(&r, "editor").unwrap();
    assert_eq!(indexed(&r), [editor]);
    press(&mut r, "spoil");
    assert_eq!(faces(&r), None, "cleared");
    assert!(
        r.journal()
            .any(|l| l.contains(&format!("view {editor}: invalid atomFaces value"))),
        "{:?}",
        r.journal().collect::<Vec<_>>()
    );
    assert!(indexed(&r).is_empty());
    let (_, set) = r.set_token("ink", "#0000ff", &[]).unwrap();
    assert_eq!(set, 0);
    assert_eq!(faces(&r), None);
}

#[test]
fn shared_indexing_keeps_theme_flips_and_token_edits_local() {
    let program = shared("shared-plan");
    let boot = || boot_shared(&program);
    let (mut first, mut second) = (boot(), boot());
    for _ in 0..3 {
        first.set_mode("dark-style", "dim").unwrap();
        assert_eq!(paint(&first, "color"), "light-dark(#111111ff, #ccccccff)");
        assert_eq!(paint(&second, "color"), "light-dark(#111111ff, #eeeeeeff)");
        first.set_mode("dark-style", "black").unwrap();
        assert_eq!(paint(&first, "color"), paint(&second, "color"));
    }
    first.set_token("brand", "#00ff00", &[]).unwrap();
    second.set_mode("dark-style", "dim").unwrap();
    assert_eq!(paint(&first, "backgroundColor"), "#00ff00ff");
    assert_eq!(paint(&second, "backgroundColor"), "#c8102eff");
    assert_eq!(paint(&boot(), "backgroundColor"), "#c8102eff");
}

#[test]
fn a_runner_booted_after_an_edited_one_is_dropped_starts_from_its_tokens() {
    let program = shared("shared-plan-dropped");
    // Each boot clones the retained program's plan, so the clone's buffers
    // are free to land where the dropped edited runner's were.
    for _ in 0..8 {
        let mut edited = boot_shared(&program);
        edited.set_token("brand", "#00ff00", &[]).unwrap();
        assert_eq!(paint(&edited, "backgroundColor"), "#00ff00ff");
        drop(edited);
        assert_eq!(
            paint(&boot_shared(&program), "backgroundColor"),
            "#c8102eff"
        );
    }
}

/// A program, retained by the test, whose faces read `ink` and `brand`.
fn shared(name: &str) -> crate::Program {
    crate::Program::new(plan(
        name,
        &format!("atom mention prefix=\"@\"\ncomponent App\n  derive faces = [{FACE}, color=\"ink\", backgroundColor=\"brand\")]\n  view\n    column\n      textarea value=\"@a \" markup=\"markdown\" atoms=\"mention\" atomFaces=faces testId=\"editor\"\n"),
    ))
}

fn boot_shared(program: &crate::Program) -> Runner<NoData> {
    Runner::boot_at(
        program,
        NoData,
        Kernel::with_monospace(),
        Default::default(),
        "/",
        1000.0,
        Default::default(),
    )
    .unwrap()
}

#[test]
fn a_refused_retheme_restores_rows_and_looks_before_retry() {
    use exact_kernel::StyleId;

    let mut r = boot(plan(
        "refused-retheme",
        r#"style Painted
  color = "ink"
  :hover
    background-color = "ink"

component App
  state dim = false
  state count = 0
  action change(newCount: number)
    dim = true
    count = newCount
  view
    column -exact-mode=(dim ? "dark-style=dim" : "dark-style=black")
      row role="button" class=Painted testId="painted"
        text "Painted"
      if dim
        Candidate(count=count)
component Candidate
  props
    count: number
  state ticks = 0
  task tick when true key=(1 / (count - 1))
    every(10, bump)
  action bump
    ticks = ticks + 1
  view
    text `${ticks}`
"#,
    ));
    assert!(r.transactional_render());
    let id = view(&r, "painted").unwrap();
    let before = r.tuple_at(id);
    let tuples = |r: &Runner<NoData>| {
        (
            r.ids.theme.row(id, StyleId::TextColor).unwrap().tuple,
            r.ids.theme.look(id).unwrap().tuple,
        )
    };
    assert_eq!(tuples(&r), (before, before));

    assert!(r.act("change", vec![Value::Number(1.)]).is_err());
    assert!(!r.is_poisoned());
    assert_eq!(r.tuple_at(id), before);
    assert_eq!(tuples(&r), (before, before));

    r.act("change", vec![Value::Number(2.)]).unwrap();
    assert_eq!(view(&r, "painted"), Some(id));
    let after = r.tuple_at(id);
    assert_ne!(after, before);
    assert_eq!(tuples(&r), (after, after));
}

#[test]
fn an_unresolved_surface_retries_at_the_same_place() {
    let mut runner = boot(plan(
        "surface-colour-retry",
        "component App\n  view\n    canvas surface=board(color(\"brand\"))\n",
    ));
    let initial = runner.take_surface_updates();
    assert_eq!(initial.len(), 1);
    runner.set_token("brand", "not-a-colour", &[]).unwrap();
    assert!(runner.take_surface_updates().is_empty());
    runner.refresh_surface_colors();
    assert!(crate::bridge::edit_token(
        &runner.plan,
        "brand",
        "#c8102e",
        &[]
    ));
    runner.refresh_surface_colors();
    let recovered = runner.take_surface_updates();
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].arguments_json(), initial[0].arguments_json());
    assert_eq!(
        runner
            .journal()
            .filter(|line| line.contains("board")
                && line.contains("brand")
                && line.contains("refused"))
            .count(),
        1
    );
    runner.refresh_surface_colors();
    assert!(runner.take_surface_updates().is_empty());
}
