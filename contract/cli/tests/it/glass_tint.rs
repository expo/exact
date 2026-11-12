//! A glass material's tint and its interactivity (`-exact-glass-tint`,
//! `glassInteractive`): the author's word over the platform's untinted glass,
//! interactive when pressable.

use exact_kernel::{Color, ColorValue, Kernel, PropId, PropValue, StyleId};
use exact_plan::Value;
use exact_runner::{DataError, DataSource, Runner};

struct NoData;

impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
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
