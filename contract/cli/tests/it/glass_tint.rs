//! Glass materials and groups carried by classes, and a material's tint
//! (`-exact-glass-tint`) and interactivity (`glassInteractive`).

use exact_kernel::{Color, ColorValue, Kernel, PropId, PropValue, StyleId};
use exact_plan::Value;
use exact_runner::{DataError, DataSource, Runner};

struct NoData;

impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

#[test]
fn material_classes_produce_the_same_props_as_direct_attributes() {
    let plan = contract::compile(
        r#"style Glass
  backgroundMaterial="glass"
style Plain
  backgroundMaterial="none"
style Group
  glassGroup=0
style AutoGroup
  glassGroup="auto"
style Empty
  opacity=1
component App
  state on = false
  action flip
    on = not on
  view
    column
      box testId="flip" press=flip
      box testId="literal" class=Glass
      box testId="literal-direct" backgroundMaterial="glass"
      row testId="group" class=Group
      row testId="group-direct" glassGroup=0
      row testId="auto" class=AutoGroup
      row testId="auto-direct" glassGroup="auto"
      box testId="choice" class=(on ? Glass : Plain)
      box testId="choice-direct" backgroundMaterial=(on ? "glass" : "none")
      row testId="group-choice" class=(on ? Group : AutoGroup)
      row testId="group-choice-direct" glassGroup=(on ? 0 : "auto")
      box testId="own" backgroundMaterial="thin" class=Glass
      box testId="own-choice" class=(on ? Glass : Empty) backgroundMaterial="thin"
      box testId="own-direct" backgroundMaterial="thin"
      row testId="own-group" class=Group glassGroup=12
      row testId="own-group-choice" glassGroup=12 class=(on ? Group : Empty)
      row testId="own-group-direct" glassGroup=12
"#,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let plan = contract::bake(plan, NoData).unwrap();
    let mut r = Runner::boot(
        plan,
        NoData,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let prop = |r: &Runner<NoData>, id: &str, prop: PropId| {
        let k = r.kernel();
        k.node_by_key(k.find_by_test_id(id)[0])
            .unwrap()
            .props
            .get(prop)
            .cloned()
    };
    for on in [false, true, false] {
        for (class, direct, id) in [
            ("literal", "literal-direct", PropId::BackgroundMaterial),
            ("group", "group-direct", PropId::GlassGroup),
            ("auto", "auto-direct", PropId::GlassGroup),
            ("choice", "choice-direct", PropId::BackgroundMaterial),
            ("group-choice", "group-choice-direct", PropId::GlassGroup),
            ("own", "own-direct", PropId::BackgroundMaterial),
            ("own-choice", "own-direct", PropId::BackgroundMaterial),
            ("own-group", "own-group-direct", PropId::GlassGroup),
            ("own-group-choice", "own-group-direct", PropId::GlassGroup),
        ] {
            assert_eq!(
                prop(&r, class, id),
                prop(&r, direct, id),
                "{class}, on={on}"
            );
        }
        assert_eq!(
            prop(&r, "literal", PropId::BackgroundMaterial),
            Some(PropValue::Str("glass".into()))
        );
        assert_eq!(
            prop(&r, "choice", PropId::BackgroundMaterial),
            on.then(|| PropValue::Str("glass".into()))
        );
        assert_eq!(
            prop(&r, "group-choice", PropId::GlassGroup),
            Some(PropValue::Float(if on { 0.0 } else { -1.0 }))
        );
        let k = r.kernel();
        let flip = k.node_by_key(k.find_by_test_id("flip")[0]).unwrap().id;
        r.dispatch(flip, exact_runner::Event::Press).unwrap();
    }
}

#[test]
fn material_classes_check_each_literal_including_an_unselected_arm() {
    for (name, good, bad, reason) in [
        (
            "backgroundMaterial",
            "\"glass\"",
            "\"frosted\"",
            "not a material",
        ),
        ("glassGroup", "8", "-1", "from 0 to 10000"),
        ("glassGroup", "8", "10001", "from 0 to 10000"),
        ("glassGroup", "8", "\"wide\"", "or `\"auto\"`"),
    ] {
        for class in ["Bad", "(on ? Good : Bad)", "(on ? Bad : Good)"] {
            let source = format!(
                "style Good\n  {name}={good}\nstyle Bad\n  {name}={bad}\ncomponent App\n  state on = true\n  view\n    box class={class}\n"
            );
            let error = contract::compile(&source).unwrap_err();
            assert_eq!(error.id, "lower-attr-value", "{source}: {error}");
            assert!(error.message.contains(reason), "{source}: {error}");
        }
    }
}

#[test]
fn conditional_material_props_must_be_present_in_both_styles() {
    for attr in ["backgroundMaterial=\"glass\"", "glassGroup=8"] {
        for class in ["(on ? Material : Empty)", "(on ? Empty : Material)"] {
            let source = format!(
                "style Material\n  {attr}\nstyle Empty\n  opacity=1\ncomponent App\n  state on = true\n  view\n    box class={class}\n"
            );
            let error = contract::compile(&source).unwrap_err();
            assert_eq!(error.id, "lower-style-prop", "{source}: {error}");
            assert!(
                error.message.contains("set it in both or neither"),
                "{error}"
            );
        }
    }
}

#[test]
fn material_classes_keep_the_direct_attributes_context_checks() {
    for (attr, node, error_id) in [
        ("glassGroup=8", "scroll", "lower-glass-group"),
        ("glassGroup=8", "canvas", "lower-glass-group"),
        (
            "glassGroup=8",
            "row backgroundMaterial=\"glass\"",
            "lower-glass-group",
        ),
        (
            "backgroundMaterial=\"glass\" glassGroup=8",
            "row",
            "lower-glass-group",
        ),
        (
            "backgroundMaterial=\"glass\"",
            "button appearance=\"auto\"",
            "lower-button-style-attr",
        ),
        (
            "glassGroup=8",
            "button appearance=\"auto\"",
            "lower-button-style-attr",
        ),
    ] {
        for applied in ["class=Material", attr] {
            let source = format!(
                "style Material\n  {attr}\ncomponent App\n  view\n    {node} {applied}\n      text \"Content\"\n"
            );
            let error = contract::compile(&source).unwrap_err();
            assert_eq!(error.id, error_id, "{source}: {error}");
        }
    }
}

/// The tint is a colour row of the box's own, unset by default; the
/// interactive override is a bool prop, unset by default; a bound tint
/// follows its state.
#[test]
fn a_glass_tint_is_a_colour_row_and_interactive_a_prop() {
    let plan = contract::compile(
        r#"component App
  state dim = false
  action go
    dim = not dim
  view
    column
      box testId="gold" backgroundMaterial="glass" -exact-glass-tint="rgba(232,192,99,0.9)" glassInteractive=false press=go
        text "ROLL"
      box testId="panel" backgroundMaterial="glass-clear" -exact-glass-tint=(dim ? "rgba(10,12,16,0.55)" : "transparent")
        box testId="inside" backgroundMaterial="glass"
      box testId="plain" backgroundMaterial="glass"
"#,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let r = Runner::boot(
        plan,
        NoData,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let k = r.kernel();
    let node = |id: &str| k.node_by_key(k.find_by_test_id(id)[0]).unwrap();
    let tint = |id: &str| {
        let n = node(id);
        n.style
            .written
            .has(StyleId::GlassTint)
            .then_some(n.style.rare.glass_tint)
    };
    assert_eq!(
        tint("gold"),
        Some(ColorValue::Fixed(Color::rgba(232, 192, 99, 230)))
    );
    assert_eq!(
        tint("panel"),
        Some(ColorValue::Fixed(Color::rgba(0, 0, 0, 0)))
    );
    assert_eq!(tint("inside"), None, "not inherited");
    assert_eq!(tint("plain"), None, "unset: untinted glass");
    assert_eq!(
        node("gold").props.get(PropId::GlassInteractive),
        Some(&PropValue::Bool(false))
    );
    assert_eq!(
        node("plain").props.get(PropId::GlassInteractive),
        None,
        "unset: inferred from press"
    );
}

/// A native button's glass is its `buttonStyle`'s, so neither word is taken
/// there; the names other platforms use point at these.
#[test]
fn a_native_button_refuses_them_and_other_names_point_here() {
    for attr in ["-exact-glass-tint=\"red\"", "glassInteractive=true"] {
        let error = contract::compile(&format!(
            "component App\n  action go\n  view\n    button appearance=\"auto\" press=go {attr}\n      text \"Go\"\n"
        ))
        .unwrap_err();
        assert_eq!(error.id, "lower-button-style-attr", "{attr}: {error:?}");
    }
    for (old, new) in [
        ("background-material-tint", "-exact-glass-tint"),
        ("isInteractive", "glassInteractive"),
    ] {
        let error = contract::compile(&format!(
            "component App\n  view\n    box backgroundMaterial=\"glass\" {old}=\"red\"\n"
        ))
        .unwrap_err();
        assert!(error.message.contains(new), "{old}: {error:?}");
    }
}
