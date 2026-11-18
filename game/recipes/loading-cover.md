# Recipe: a loading cover

Keep the canvas mounted and show a cover over it while `!hud.presented`.
The simulation runs under the cover. If a render hook needs to prepare its
first complete picture, keep `Needs::PENDING` set until that picture is ready
to draw (LLP 1182.000 D7).

```contract
shape Hud
  presented: bool

component Screen
  state paused = false
  resource hud = exactSurface("world") as shape Hud
  view
    canvas surface=world(paused=paused) position="relative" width="100%" height="100%"
      if !hud.presented
        column position="absolute" top=0 right=0 bottom=0 left=0 align-items="center" justify-content="center" background-color="-exact-background"
          text "Loading…"
```

`presented` latches when the host hands the first complete frame of the current
world to the compositor. It starts false and resets after a restore, a restart,
a lost GPU device or a call to `w.invalidate_presentation()`. The world and host
own those resets. Track readiness for later generations of hook content in the
hook itself.

## Hold

Bind `paused=(paused || !hud.presented)` to stop ticks while the cover is up.
Use this variant only when all three conditions hold.

- Preparation can finish while the world is paused. Keep the default cover if
  ticks supply work that preparation needs.
- Losing input during the hold is acceptable. A tap during the hold loses its
  press and release edges. The same tap can reach a tick on a host that presents
  sooner.
- The app accepts that those two runs produce different saves. The save holds
  the current arguments. A timed bind advances the simulation with the old
  arguments before applying the new ones.

This fragment checks the tap on both sides of the pause.

```rust
use exact_game::*;

#[derive(Default, Args)]
struct Options {
    #[live]
    paused: bool,
}

#[derive(Default, Resource)]
struct Edges {
    presses: u32,
    releases: u32,
}

struct GameWithPause;
impl Game for GameWithPause {
    const ID: &'static str = "recipe-loading-cover";
    type Args = Options;

    fn actions() -> Actions {
        Actions::new().button("jump", &["Space"])
    }

    fn setup(w: &mut World, _: &Options) {
        w.insert_resource(Edges::default());
    }

    fn paused(args: &Options) -> bool {
        args.paused
    }

    fn tick(w: &mut World, input: &Input, _: &Options) {
        let mut edges = w.resource_mut::<Edges>();
        edges.presses += u32::from(input.pressed("jump"));
        edges.releases += u32::from(input.released("jump"));
    }
}

let edges = |sim: &Sim<GameWithPause>| {
    let edges = sim.world().resource::<Edges>();
    (edges.presses, edges.releases)
};
let mut sim = Sim::<GameWithPause>::new(Options { paused: true }).unwrap();
sim.tap("Space");
assert_eq!(sim.run(20.), 0, "the hold stops ticks");
sim.bind(&[Value::Bool(false)], None).unwrap();
assert!(sim.run(20.) > 0);
assert_eq!(edges(&sim), (0, 0), "the paused tap's edges are gone");

sim.tap("Space");
sim.run(20.);
assert_eq!(edges(&sim), (1, 1), "the unpaused tap reaches a tick");
```
