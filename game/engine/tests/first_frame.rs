//! `Game::first_frame`: a new world framed for its canvas before its first
//! tick and its first drawn frame.
use exact_game::*;

#[derive(Default, Args)]
struct Table {
    round: u32,
}
struct Felt;
impl Game for Felt {
    const ID: &'static str = "first-frame";
    type Args = Table;
    fn setup(w: &mut World, _: &Table) {
        w.spawn_named("camera", (Transform::at(0., 2., 3.), Camera::default()));
        w.publish("framed", 0);
    }
    // A wide canvas backs the camera away, as a table overview fitted to the screen.
    fn first_frame(w: &mut World, input: &Input, _: &Table) {
        let size = input.viewport();
        w.require_mut::<Transform>("camera").position.z = 10. * size.x / size.y;
        let framed = w.published("framed").unwrap().as_number().unwrap() as u32;
        w.publish("framed", framed + 1);
    }
    fn tick(_: &mut World, _: &Input, _: &Table) {}
}

fn z(sim: &Sim<Felt>) -> f32 {
    sim.world().require::<Transform>("camera").position.z
}
fn framed(sim: &Sim<Felt>) -> f64 {
    sim.world()
        .published("framed")
        .unwrap()
        .as_number()
        .unwrap()
}

#[test]
fn the_first_size_frames_a_new_world_once_before_its_first_tick() {
    let mut sim = Sim::<Felt>::new(Table::default()).unwrap();
    assert_eq!((z(&sim), framed(&sim)), (3., 0.));
    assert!(sim.viewport(800., 400.));
    assert_eq!((sim.world().tick(), z(&sim), framed(&sim)), (0, 20., 1.));
    // A resize is the ticks' to read.
    assert!(!sim.viewport(400., 400.));
    sim.run(100.);
    assert_eq!((z(&sim), framed(&sim)), (20., 1.));
    // The same size frames the same world.
    let mut again = Sim::<Felt>::new(Table::default()).unwrap();
    again.viewport(800., 400.);
    again.run(100.);
    assert_eq!(again.world().hash(), sim.world().hash());
}

#[test]
fn a_restored_world_keeps_its_framing_and_never_runs_it() {
    let mut sim = Sim::<Felt>::new(Table::default()).unwrap();
    sim.viewport(800., 400.);
    let save = sim.save().unwrap();
    let mut restored = Sim::<Felt>::new(Table::default()).unwrap();
    restored.restore(&save).unwrap();
    assert!(!restored.viewport(300., 600.));
    assert_eq!((z(&restored), framed(&restored)), (20., 1.));
    // Nor does a world saved before it had a size, once restored.
    let early = Sim::<Felt>::new(Table::default()).unwrap().save().unwrap();
    restored.restore(&early).unwrap();
    assert!(!restored.viewport(800., 400.));
    assert_eq!(z(&restored), 3.);
}

#[test]
fn a_world_that_ticked_without_a_canvas_is_not_framed_later() {
    let mut sim = Sim::<Felt>::new(Table::default()).unwrap();
    sim.run(100.);
    assert!(!sim.viewport(800., 400.));
    assert_eq!((z(&sim), framed(&sim)), (3., 0.));
}

#[test]
fn a_restart_frames_the_new_world_with_the_canvas_it_has() {
    let mut sim = Sim::<Felt>::new(Table::default()).unwrap();
    sim.viewport(800., 400.);
    sim.run(100.);
    sim.bind(&[Value::Number(1.)], None).unwrap();
    assert_eq!(
        (sim.world().tick(), z(&sim), framed(&sim)),
        (0, 20., 1.),
        "the restarted world is framed before it is drawn"
    );
}

struct Loading;
impl Game for Loading {
    const ID: &'static str = "first-frame-assets";
    const ASSETS: &'static [&'static str] = &["table.model"];
    type Args = ();
    fn setup(w: &mut World, _: &()) {
        w.spawn_named("camera", (Transform::default(), Camera::default()));
    }
    fn first_frame(w: &mut World, input: &Input, _: &()) {
        w.require_mut::<Transform>("camera").position.x = input.viewport().x;
    }
    fn tick(_: &mut World, _: &Input, _: &()) {}
}

#[test]
fn a_world_waiting_for_its_assets_is_framed_when_setup_runs() {
    let bytes = asset::encode_model(&asset::Model::default());
    let mut sim = Sim::<Loading>::new(()).unwrap();
    // The canvas has its size before the declared assets arrive.
    assert!(!sim.viewport(640., 480.));
    sim.asset("table.model", Some(&bytes)).unwrap();
    let camera = sim.world().require::<Transform>("camera").position.x;
    assert_eq!((sim.world().tick(), camera), (0, 640.));
}
