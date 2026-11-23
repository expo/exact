//! LLP 1013.001 D1, D4: a route's entrance and exit are CSS's
//! `::view-transition-new { animation }` and `::view-transition-old`'s, on a
//! route that is pushed or presented full screen, in a `style` it takes or
//! under the route itself. Each lowers to its row with the keyframes resolved;
//! `none` sets the row (the author's cut) where no part leaves it unset (the
//! platform's transition). The shorthand alone, literal arms, an animation
//! that ends and keyframes that move a picture; nowhere but on a route.

use crate::source::{boxed, NoData, Source};
use exact_kernel::{Kernel, StyleId};
use exact_runner::{Event, Runner};

const KEYFRAMES: &str = "keyframes fade-in
  from opacity=0
keyframes fade-out
  to opacity=0
keyframes enter-end
  from opacity=0 translate=\"10% 0\"
keyframes spin
  to rotate=\"10deg\" scale=0.9
keyframes tint
  to background-color=\"red\"
";

/// A navigation root over `routes`; `STYLES` precede the app and `ROUTE`
/// holds the attributes and body of the route `testId="route"`.
const SCREEN: &str = "STYLESroutes nav
  home \"/\"
component App
  state still = false
  action calm
    still = true
  view
    main navigationKey=`${top(nav).id}` navigationBack=\"back\"
      for e in stack(nav) key=e.id
        column navigationKey=`${e.id}` testId=\"route\" ROUTE
          button press=calm testId=\"calm\"
            text \"Calm\"
";

fn screen(styles: &str, route: &str) -> String {
    SCREEN
        .replace("STYLES", &format!("{KEYFRAMES}{styles}"))
        .replace("ROUTE", route)
}

fn boot(source: &str) -> Runner<Source> {
    let plan = contract::compile(source).unwrap_or_else(|e| panic!("{e}"));
    Runner::boot(
        plan,
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

/// The route's entrance and exit: each `None` when unset, else the CSS of
/// its animations (`none` for none) and the keyframes each carries.
type Part = Option<(String, Vec<usize>)>;

fn parts(r: &Runner<Source>) -> (Part, Part) {
    let key = r.kernel().find_by_test_id("route")[0];
    let style = &r.kernel().node_by_key(key).unwrap().style;
    let part = |row, list: &exact_motion::Animations| {
        style.mask.has(row).then(|| {
            (
                list.css(),
                list.0.iter().map(|a| a.keyframes.0.len()).collect(),
            )
        })
    };
    (
        part(StyleId::ViewTransitionNew, &style.rare.view_transition_new),
        part(StyleId::ViewTransitionOld, &style.rare.view_transition_old),
    )
}

#[test]
fn a_routes_parts_lower_to_its_rows_with_their_keyframes() {
    let fades = "style Fades
  ::view-transition-new
    animation=\"fade-in 180ms ease-out both\"
  ::view-transition-old
    animation=\"fade-out 150ms ease-in both\"
";
    let r = boot(&screen(fades, "class=Fades"));
    let (new, old) = parts(&r);
    let (css, frames) = new.expect("the entrance is set");
    assert!(
        css.starts_with("0.18s ease-out") && css.ends_with("fade-in"),
        "{css}"
    );
    assert!(
        frames[0] > 0,
        "its keyframes resolved, as `animation`'s are"
    );
    assert!(old.expect("the exit is set").0.ends_with("fade-out"));

    // The route's own part wins over its class's; `none` is set, and empty.
    let own = "class=Fades\n          ::view-transition-new\n            animation=\"none\"";
    let (new, old) = parts(&boot(&screen(fades, own)));
    assert_eq!(new, Some(("none".to_string(), vec![])), "the author's cut");
    assert!(old.is_some(), "the class's exit stands");

    // No part: the platform's transition.
    assert_eq!(parts(&boot(&screen("", ""))), (None, None));

    // A full-screen route takes them, and a choice of literals, whose unset
    // side gives the platform's transition back.
    let chosen = "navigationPresentation=\"fullscreen\"
          ::view-transition-new
            animation=(still ? none : \"enter-end 250ms ease-out both\")
          ::view-transition-old
            animation=(still ? \"none\" : \"spin 200ms\")";
    let mut r = boot(&screen("", chosen));
    let (new, old) = parts(&r);
    assert!(new.unwrap().0.ends_with("enter-end"));
    assert!(old.unwrap().0.ends_with("spin"));
    let calm = r.kernel().find_by_test_id("calm")[0];
    let calm = r.kernel().node_by_key(calm).unwrap().id;
    r.dispatch(calm, Event::Press).unwrap();
    let (new, old) = parts(&r);
    assert_eq!(new, None, "an unset arm: the platform's transition");
    assert_eq!(
        old,
        Some(("none".to_string(), vec![])),
        "a `none` arm: a cut"
    );
}

#[test]
fn a_part_is_refused_where_it_cannot_play_and_names_what_to_write() {
    let route = |body: &str| screen("", body);
    let part =
        |value: &str| format!("\n          ::view-transition-new\n            animation={value}");
    let cases: Vec<(String, &str, &str)> = vec![
        // D1: the shorthand alone, literal, ending, a picture's keyframes.
        (
            route("\n          ::view-transition-new\n            animation-duration=\"180ms\""),
            "lower-unknown-attr",
            "shorthand alone",
        ),
        (
            route("\n          ::view-transition-old\n            opacity=0"),
            "lower-unknown-attr",
            "admits only `animation`",
        ),
        (
            route(&part("`fade-in ${180}ms`")),
            "lower-route-part",
            "built at run time",
        ),
        (
            route(&part("\"fade-in 1s infinite\"")),
            "lower-route-part",
            "must end",
        ),
        (
            route(&part("(still ? \"fade-in 1s paused\" : \"none\")")),
            "lower-route-part",
            "must end",
        ),
        (route(&part("\"tint 1s\"")), "lower-route-part", "`opacity`, `translate`, `scale` and `rotate`"),
        (route(&part("\"missing 1s\"")), "lower-animation-name", "no `keyframes missing`"),
        // D4: a sheet's motion and a zoom are the platform's.
        (
            route(&format!("navigationPresentation=\"modal\"{}", part("\"fade-in 1s\""))),
            "lower-route-part",
            "sheet",
        ),
        (
            route(&format!("navigationSource=\"thumb\"{}", part("\"fade-in 1s\""))),
            "lower-route-part",
            "navigationSource",
        ),
        // Nothing but a route takes them.
        (
            screen(
                "style Fades\n  ::view-transition-new\n    animation=\"fade-in 1s\"\n",
                "\n          column class=Fades",
            ),
            "lower-route-part",
            "sharedElement",
        ),
        // The other View Transitions parts and names.
        (
            route("\n          ::view-transition-group\n            animation=\"fade-in 1s\""),
            "syntax-part",
            "nothing to style",
        ),
        (route("view-transition-name=\"hero\""), "lower-unknown-attr", "sharedElement"),
        (
            route("\n          ::view-transition-new\n            animation=\"fade-in 1s\"\n          ::view-transition-new\n            animation=\"none\""),
            "syntax-duplicate-attr",
            "::view-transition-new",
        ),
    ];
    for (source, refusal, says) in cases {
        let e = contract::compile(&source).unwrap_err();
        assert_eq!(e.id, refusal, "{source}: {}", e.message);
        assert!(e.message.contains(says), "{source}: {}", e.message);
    }
    // A computed `navigationSource` is the run time's to decide (D2).
    let computed = format!(
        "navigationSource=(still ? \"thumb\" : \"\"){}",
        part("\"fade-in 1s\"")
    );
    contract::compile(&route(&computed)).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn spatial_keyframes_on_route_pictures_name_the_unsupported_hosts() {
    let source = screen("keyframes fold\n  to rotate=\"-90deg x\"\nstyle Fold\n  ::view-transition-new\n    animation=\"fold 1s linear both\"\n", "class=Fold");
    let e = contract::compile(&source).unwrap_err();
    assert_eq!(e.id, "lower-route-part");
    assert!(
        e.message.contains("Apple or painted hosts"),
        "{}",
        e.message
    );
    assert!(e.message.contains("box inside the route"), "{}", e.message);
}
