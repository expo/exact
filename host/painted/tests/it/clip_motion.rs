//! LLP 1003.003 D7 on the paint walk: a box's `clip-path` under `transition`
//! moves on the agent's clock, and one presented clip serves the box's
//! pixels and its hits.

use exact_painted::{presenter::PainterChoice, Presenter};

/// A blue balloon whose tail shrinks from a tip at 48 to its origin at 40
/// over 400 ms.
const BALLOON: &str = "component App\n  state grown = true\n  action flip\n    grown = not grown\n  view\n    column\n      button press=flip testId=\"flip\"\n        text \"Flip\"\n      box testId=\"balloon\" width=120 height=48 background-color=\"#0000ff\" clip-path=(grown ? \"path('M0 0H120V40H0ZM104 40L120 40L120 48Z')\" : \"path('M0 0H120V40H0ZM104 40L120 40L120 40Z')\") transition=\"clip-path 400ms linear\"\n";

#[test]
fn a_moving_clip_paints_and_hits_as_it_shows() {
    let plan = contract::compile(BALLOON).unwrap_or_else(|e| panic!("{e}"));
    let (mut p, error) = Presenter::boot_with(
        &plan.encode(),
        (),
        (200., 200.),
        1.,
        std::env::temp_dir(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let id = |p: &Presenter<()>, name: &str| {
        let k = p.host().kernel();
        k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
    };
    let balloon = id(&p, "balloon");
    let origin = |p: &mut Presenter<()>| {
        let b = p.boxes().iter().find(|b| b.id == balloon).unwrap().clone();
        (b.rect.0, b.rect.1)
    };
    // Whether the point in the balloon shows blue, and whether a press there
    // finds the balloon.
    let at = |p: &mut Presenter<()>, x: f32, y: f32| {
        let (ox, oy) = origin(p);
        let c = p
            .frame()
            .pixel((ox + x) as u32, (oy + y) as u32)
            .unwrap()
            .demultiply();
        let blue = c.alpha() > 200 && c.blue() > 200 && c.red() < 60;
        (blue, p.hit(ox + x, oy + y) == Some(balloon))
    };
    assert_eq!(at(&mut p, 119., 46.), (true, true), "the grown tail");
    p.tap(id(&p, "flip")).unwrap();
    p.tick(200.);
    assert_eq!(
        at(&mut p, 119., 42.),
        (true, true),
        "half way, the tail reaches y 44"
    );
    assert_eq!(at(&mut p, 119., 46.), (false, false), "and no further");
    assert_eq!(at(&mut p, 60., 20.), (true, true), "the body");
    p.tick(500.);
    assert_eq!(
        at(&mut p, 119., 42.),
        (false, false),
        "settled at its origin"
    );
}

/// A value between two arcs whose ends meet draws nothing, which clips the
/// whole box (CSS Masking 1 §5): no pixel and no hit, held here from a
/// quarter to three quarters of the run.
#[test]
fn a_moving_clip_that_draws_nothing_hides_the_box() {
    const ARCS: &str = "component App\n  state flipped = false\n  action flip\n    flipped = not flipped\n  view\n    column\n      button press=flip testId=\"flip\"\n        text \"Flip\"\n      box testId=\"dome\" width=60 height=60 background-color=\"#0000ff\" clip-path=(flipped ? \"path('M50 30 A20 20 0 0 1 10 30')\" : \"path('M10 30 A20 20 0 0 1 50 30')\") transition=\"clip-path 400ms linear(0, .5 25%, .5 75%, 1)\"\n";
    let plan = contract::compile(ARCS).unwrap_or_else(|e| panic!("{e}"));
    let (mut p, error) = Presenter::boot_with(
        &plan.encode(),
        (),
        (200., 200.),
        1.,
        std::env::temp_dir(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let id = |p: &Presenter<()>, name: &str| {
        let k = p.host().kernel();
        k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
    };
    let dome = id(&p, "dome");
    let at = |p: &mut Presenter<()>, x: f32, y: f32| {
        let b = p.boxes().iter().find(|b| b.id == dome).unwrap().clone();
        let (ox, oy) = (b.rect.0, b.rect.1);
        let c = p
            .frame()
            .pixel((ox + x) as u32, (oy + y) as u32)
            .unwrap()
            .demultiply();
        (
            c.alpha() > 200 && c.blue() > 200 && c.red() < 60,
            p.hit(ox + x, oy + y) == Some(dome),
        )
    };
    assert_eq!(at(&mut p, 30., 20.), (true, true), "inside the dome");
    p.tap(id(&p, "flip")).unwrap();
    p.tick(200.);
    assert_eq!(
        at(&mut p, 30., 20.),
        (false, false),
        "half way the clip draws nothing"
    );
    assert_eq!(at(&mut p, 30., 45.), (false, false), "nowhere");
}
