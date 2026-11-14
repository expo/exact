use super::warnings::warned;
use std::path::PathBuf;

fn source(view: &str) -> String {
    let view: String = view.lines().map(|line| format!("    {line}\n")).collect();
    format!("component App\n  state count = 0\n  action keys(k: string)\n    count = count + 1\n  view\n{view}")
}

#[test]
fn composite_roles_without_keys_warn_with_a_repair() {
    for role in ["listbox", "tree", "treegrid", "grid", "toolbar", "menubar"] {
        let src = source(&format!("column role=\"{role}\"\n  button \"One\""));
        assert_eq!(warned(&src), [("lower-role-keys".into(), 6)], "{role}");
        let (_, warnings) = contract::compile_warned(&src);
        let message = &warnings[0].message;
        assert!(
            message.contains(&format!("Exact supplies no keys for `role=\"{role}\"`"))
                && message.contains("each member is its own Tab stop")
                && message.contains("arrows do nothing")
                && message.contains("`key` action on the group or its members"),
            "{message}"
        );
        assert_eq!(
            contract_lower::warning_rank::rank(&warnings[0].id, message),
            0
        );
    }
    let src = source(
        "column role=\"listbox\" id=\"choices\" aria-controls=\"choices\"\n  button \"One\"",
    );
    assert_eq!(warned(&src), [("lower-role-keys".into(), 6)]);
}

#[test]
fn handled_controlled_and_platform_composites_do_not_warn() {
    for view in [
        "column role=\"listbox\" key=keys\n  button \"One\"",
        "column role=\"listbox\" keycapture=keys\n  button \"One\"",
        "column role=\"listbox\"\n  column\n    button \"One\" key=keys",
        "column role=\"listbox\"\n  button \"One\" keycapture=keys",
        "column\n  input aria-controls=\"choices\"\n  column id=\"choices\" role=\"listbox\"\n    button \"One\"",
        "column\n  column id=\"choices\" role=\"listbox\"\n    button \"One\"\n  input aria-controls=\"other choices\"",
        "column role=\"tablist\"\n  button \"One\" role=\"tab\"",
        "row role=\"radiogroup\"\n  button \"One\" role=\"radio\"",
        "column role=\"menu\"\n  button \"One\" role=\"menuitem\"",
        "row role=\"toolbar\" toolbarPlacement=\"window\"\n  button \"One\"",
        "column role=(count == 0 ? \"listbox\" : \"group\")\n  button \"One\"",
    ] {
        assert_eq!(warned(&source(view)), [], "{view}");
    }
}

/// The role that applies is the first token ARIA names, in any case, as the
/// web host reads it (`host/web/src/document.rs` `role_is`); Exact's own keys
/// are for the role written exactly, as its table and the web host's
/// selectors match it.
#[test]
fn the_role_that_applies_is_read_as_aria_reads_it() {
    for role in ["Listbox", "LISTBOX", "listbox group", "nonsense listbox"] {
        let src = source(&format!("column role=\"{role}\"\n  button \"One\""));
        assert_eq!(warned(&src), [("lower-role-keys".into(), 6)], "{role}");
    }
    for role in ["group listbox", "Group", "presentation menu"] {
        let src = source(&format!("column role=\"{role}\"\n  button \"One\""));
        assert_eq!(warned(&src), [], "{role}");
    }
    // A role Exact supplies keys for, written another way, gets none, and the
    // warning says how to write it.
    for role in ["Tablist", "tablist group"] {
        let src = source(&format!(
            "row role=\"{role}\"\n  button \"One\" role=\"tab\""
        ));
        assert_eq!(warned(&src), [("lower-role-keys".into(), 6)], "{role}");
        let (_, warnings) = contract::compile_warned(&src);
        assert!(
            warnings[0]
                .message
                .ends_with("; or write `role=\"tablist\"`, in lower case and alone, for the keys Exact supplies"),
            "{}",
            warnings[0].message
        );
    }
}

/// A use or a slot fill in the group is judged by what it draws: the check runs
/// on the expanded root, where a use is inlined, so a component's own `key`
/// handler counts and a component with none warns. (A use the walk ever meets
/// unexpanded is given the benefit of the doubt.)
#[test]
fn a_group_is_judged_by_what_its_uses_and_slots_draw() {
    let choice = "component Choice\n  props\n    label: string\n  state n = 0\n  action keys(k: string)\n    n = n + 1\n  view\n    button label key=keys\n";
    let src = format!(
        "{}{choice}",
        source("column role=\"listbox\"\n  Choice(label=\"One\")")
    );
    assert_eq!(warned(&src), [], "{src}");
    let slot = format!(
        "{}component Choices\n  slot\n  view\n    column role=\"listbox\"\n      children\n",
        source("Choices()\n  button \"One\" key=keys")
    );
    assert_eq!(warned(&slot), [], "{slot}");
    let plain = format!(
        "{}component Plain\n  view\n    button \"One\"\n",
        source("column role=\"listbox\"\n  Plain()")
    );
    assert_eq!(warned(&plain), [("lower-role-keys".into(), 6)], "{plain}");
}

struct Dir(PathBuf);
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The warnings' ids of an app whose `app.contract` uses `Choices` from
/// `choices.contract`, each file as given.
fn across(name: &str, app: &str, choices: &str) -> Vec<String> {
    let dir = std::env::temp_dir().join(format!("exact-role-keys-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let dir = Dir(dir.canonicalize().unwrap());
    std::fs::write(dir.0.join("choices.contract"), choices).unwrap();
    let path = dir.0.join("app.contract");
    std::fs::write(&path, app).unwrap();
    let (result, warnings) = contract::compile_path_all_warned(&path, false);
    if let Err(all) = result {
        panic!("{all:?}");
    }
    warnings.into_iter().map(|w| w.id).collect()
}

/// A field in one file names the listbox a component in another file draws:
/// the field's popup, whose keys the field handles.
#[test]
fn an_aria_controls_in_another_file_names_the_group() {
    let choices = "component Choices\n  view\n    column id=\"choices\" role=\"listbox\"\n      button \"One\"\n";
    let app = |field: &str| {
        format!("use Choices from \"./choices.contract\"\n\ncomponent App\n  state q = \"\"\n  view\n    column\n      {field}\n      Choices()\n")
    };
    let named = across(
        "named",
        &app("input value=q aria-label=\"Fruit\" aria-controls=\"choices\""),
        choices,
    );
    assert!(!named.iter().any(|id| id == "lower-role-keys"), "{named:?}");
    let other = across(
        "other",
        &app("input value=q aria-label=\"Fruit\" aria-controls=\"elsewhere\""),
        choices,
    );
    assert!(other.iter().any(|id| id == "lower-role-keys"), "{other:?}");
}

/// A group another element names (LLP 1166.000 D5): its literal `id` among the
/// literals an `aria-controls` can evaluate to, or its computed `id` written as
/// an `aria-controls` is. An `aria-controls` the compiler cannot resolve
/// exempts nothing by itself.
#[test]
fn a_group_is_named_by_what_an_aria_controls_can_evaluate_to() {
    let field = "input aria-controls=(count == 0 ? \"a\" : \"b\") aria-label=\"Fruit\"";
    for (id, warns) in [("b", false), ("a", false), ("other", true)] {
        let view =
            format!("column\n  {field}\n  column id=\"{id}\" role=\"listbox\"\n    button \"One\"");
        let expected: Vec<(String, u32)> = if warns {
            vec![("lower-role-keys".into(), 8)]
        } else {
            vec![]
        };
        assert_eq!(warned(&source(&view)), expected, "{view}");
    }
    // Tabs name their panels by a computed `aria-controls`: a listbox they do
    // not name still warns.
    let tabs = source(
        "column\n  row role=\"tablist\"\n    for t in [\"a\", \"b\"] key=t\n      button role=\"tab\" aria-controls=`panel-${t}`\n        text t\n  column id=\"x\" role=\"listbox\"\n    button \"One\"",
    );
    assert_eq!(warned(&tabs), [("lower-role-keys".into(), 11)], "{tabs}");
    // A row's field names its own list by the same expression.
    let rows = source(
        "column\n  for r in [\"a\", \"b\"] key=r\n    row\n      input aria-controls=`options-${r}` aria-label=\"Pick\"\n      column id=`options-${r}` role=\"listbox\"\n        button \"One\"",
    );
    assert_eq!(warned(&rows), [], "{rows}");
}
