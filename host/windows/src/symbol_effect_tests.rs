//! LLP 1077 D12 on the Windows host's painter: a symbol's
//! `-exact-symbol-effect` played as the web's stand-ins are
//! (`host/web/navigation/symbol.js`), measured in the painted frame.
use exact_painted::{presenter::PainterChoice, Presenter};
use std::{cell::Cell, rc::Rc};

/// A black heart in a 100 by 100 box at (50, 50) on white, its effect
/// `effect`, and a button that changes `taps`.
fn app(effect: &str, value: bool) -> String {
    let value = if value {
        " symbolEffectValue=`${taps}`"
    } else {
        ""
    };
    format!(
        "component App\n  state taps = 0\n  action tap\n    taps = taps + 1\n  view\n    column width=300 height=200 background-color=\"#ffffff\"\n      image \"symbol:heart-fill\" testId=\"heart\" position=\"absolute\" left=50 top=50 width=100 height=100 font-size=100 color=\"#000000\" -exact-symbol-effect=\"{effect}\"{value}\n      button press=tap testId=\"tap\" position=\"absolute\" left=200 top=150\n        text \"Tap\"\n"
    )
}

struct Drive {
    p: Presenter<()>,
    wall: Rc<Cell<f64>>,
}

impl Drive {
    /// The app on a display clock the test moves, painted once at 0.
    fn boot(source: &str, reduced: bool) -> Self {
        let plan = contract::compile(source).unwrap().encode();
        let (mut p, error) = Presenter::boot_with(
            &plan,
            (),
            (300., 200.),
            1.,
            Default::default(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        if reduced {
            let mut preferences = p.host().runner().viewport().preferences;
            preferences.reduced_motion = true;
            p.set_preferences(preferences);
        }
        let wall = Rc::new(Cell::new(0.));
        let read = wall.clone();
        p.set_live_clock(Some(Rc::new(move || read.get())));
        let mut d = Drive { p, wall };
        d.frame(0.);
        d
    }

    /// A display frame at `at` ms: the heart's ink width in it, and the
    /// red channel at its centre (255 white, 0 black).
    fn frame(&mut self, at: f64) -> (u32, u8) {
        self.wall.set(at);
        self.p.tick(at);
        assert_eq!(self.p.advance(at), None);
        let pixels = self.p.frame();
        let dark = |x: u32, y: u32| pixels.pixel(x, y).unwrap().red() < 128;
        let columns: Vec<u32> = (0..200).filter(|&x| (0..200).any(|y| dark(x, y))).collect();
        let width = match (columns.first(), columns.last()) {
            (Some(first), Some(last)) => last - first + 1,
            _ => 0,
        };
        (width, pixels.pixel(100, 100).unwrap().red())
    }

    fn tap(&mut self) {
        let k = self.p.host().kernel();
        let id = k.node_by_key(k.find_by_test_id("tap")[0]).unwrap().id;
        self.p.tap(id).unwrap();
        self.p.run_commands(|| ());
    }
}

/// Whether `width` is `scale` times `base`, to within two pixels.
fn scaled(width: u32, base: u32, scale: f32) -> bool {
    (width as f32 - base as f32 * scale).abs() <= 2.0
}

#[test]
fn a_pulse_with_a_value_dims_once_each_time_the_value_changes() {
    let mut d = Drive::boot(&app("pulse", true), false);
    // The value first shown plays nothing.
    assert_eq!(d.frame(1000.).1, 0, "the first value plays nothing");
    d.tap();
    assert_eq!(d.frame(1000.).1, 0);
    // Halfway through its 2 s it is at 30 %: white shows through 70 %.
    let (_, half) = d.frame(2000.);
    assert!((170..=186).contains(&half), "half {half}");
    // Over at 2 s, and the value unchanged plays nothing more.
    assert_eq!(d.frame(3000.).1, 0);
    assert_eq!(d.frame(4000.).1, 0);
    // Each change plays it again.
    d.tap();
    d.frame(4000.);
    assert!((170..=186).contains(&d.frame(5000.).1));
}

#[test]
fn a_bounce_without_a_value_runs_while_set() {
    let mut d = Drive::boot(&app("bounce", false), false);
    let (base, _) = d.frame(0.);
    assert!(base > 60, "a heart is drawn: {base}");
    // Its bounce peaks at 1.2 at 210 ms and dips to 0.9 at 390 ms.
    assert!(scaled(d.frame(210.).0, base, 1.2), "{base}");
    assert!(scaled(d.frame(390.).0, base, 0.9), "{base}");
    assert!(scaled(d.frame(1000.).0, base, 1.0));
    // And again every 1.5 s.
    assert!(scaled(d.frame(1710.).0, base, 1.2));
    assert!(d.p.needs_animation_frame(), "a running effect owes frames");
}

#[test]
fn a_scale_grows_and_holds_while_set() {
    let mut d = Drive::boot(&app("scale", true), false);
    let (base, _) = d.frame(0.);
    assert!(scaled(d.frame(125.).0, base, 1.1));
    assert!(scaled(d.frame(250.).0, base, 1.2));
    assert!(scaled(d.frame(5000.).0, base, 1.2), "it holds");
}

#[test]
fn less_motion_stills_a_bounce_and_a_scale_but_not_a_pulse() {
    for effect in ["bounce", "scale"] {
        let mut d = Drive::boot(&app(effect, false), true);
        let (base, _) = d.frame(0.);
        assert_eq!(d.frame(210.).0, base, "{effect}");
    }
    let mut d = Drive::boot(&app("pulse", false), true);
    let (_, half) = d.frame(900.);
    assert!((170..=186).contains(&half), "a pulse still dims: {half}");
}
