//! A popover's default surface (LLP 1021; theme2 2026-10-21 19:55): the
//! compiler marks a popover whose author wrote no surface, and the host
//! sends its corner from this platform's metrics, iOS's 30, the Mac none; a
//! menu's is the platform's menu panel's, the Mac's 12, iOS none.
use super::*;
use exact_kernel::style::metrics::Column;
use exact_kernel::MonospaceMeasurer;
use exact_plan::Value;
use exact_runner::DataError;

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

const APP: &str = r#"component A
  view
    column
      column id="bare" popover="auto" padding=12 testId="bare"
        text "Popover text"
      column id="filled" popover="auto" background-color="white" testId="filled"
        text "Filled"
      column id="menu" popover="auto" role="menu" testId="menu"
        button
          text "Item"
      column id="hovered" popover="auto" role="menu" testId="hovered"
        :hover
          background-color="transparent"
        button
          text "Item"
"#;

#[test]
fn a_popover_with_no_surface_of_its_authors_is_sent_the_platforms_corner() {
    let plan = contract::compile(APP).unwrap().encode();
    let (host, _) = Host::boot(
        &plan,
        NoData,
        Box::new(MonospaceMeasurer::default()),
        390.0,
        844.0,
    )
    .unwrap();
    let k = host.runner.kernel();
    let props = |t: &str| host.props_of(&k.node_by_key(k.find_by_test_id(t)[0]).unwrap());
    let bare = props("bare");
    assert_eq!(bare.get("defaultSurface").map(String::as_str), Some("true"));
    // On the Mac, whose column has no corner, none is sent.
    #[cfg(target_os = "macos")]
    assert_eq!(
        bare.get("defaultSurfaceRadius"),
        None,
        "the Mac's system popover draws its own"
    );
    #[cfg(target_os = "macos")]
    assert_eq!(bare.get("defaultSurfaceShadow"), None);
    let filled = props("filled");
    assert_eq!(filled.get("defaultSurface"), None, "its author's surface");
    // A menu is marked too, for the Mac's painted menu (expo/exact#551),
    // and sent the corner of AppKit's menu panel there (`menu-radius`).
    let mark = |t: &str| props(t).get("defaultSurface").cloned();
    assert_eq!(mark("menu").as_deref(), Some("true"));
    #[cfg(target_os = "macos")]
    assert_eq!(
        props("menu")
            .get("defaultSurfaceRadius")
            .map(String::as_str),
        Some("12")
    );
    assert_eq!(props("menu").get("defaultSurfaceShadow"), None);
    assert_eq!(
        mark("hovered"),
        None,
        "a surface in a state is its author's"
    );
    assert_eq!(
        default_surface(Column::Ios),
        Some(("30".into(), "8 24 0.015".into())),
        "iOS's corner and shadow"
    );
    assert_eq!(default_surface(Column::Mac), None);
    assert_eq!(menu_surface(Column::Mac), Some("12".into()));
    assert_eq!(menu_surface(Column::Ios), None, "iOS presents its own menu");
}
