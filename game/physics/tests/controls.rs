// Body and collider controls that are saved only when set: a force, an authored
// inertia, average restitution.
use exact_game::{bin, hash, json, Entity, Transform, Vec3, World};
use exact_game_physics::{self as physics, Body, BounceCombine, Collider, ExtraShape, Shape};

fn world() -> World {
    let mut w = World::new(60, 0);
    physics::register(&mut w);
    w
}
fn restored(w: &World) -> World {
    let mut r = world();
    r.load(&w.save()).unwrap();
    r
}
fn ball(w: &mut World, x: f32, body: Body) -> Entity {
    w.spawn((
        Transform::at(x, 0.0, 0.0),
        body,
        Collider {
            shape: Shape::Sphere { radius: 0.5 },
            ..Collider::default()
        },
    ))
}

// A force accelerates its body by F/m every tick until it is written back to zero,
// contacts still act on the body it pushes, and a restored world continues with
// the same force to the bit.
#[test]
fn center_force_persists_until_cleared_and_replays() {
    let mut w = world();
    let free = Body {
        gravity: 0.0,
        mass: 2.0,
        ..Body::default()
    };
    let pushed = ball(
        &mut w,
        0.0,
        Body {
            force: Vec3::new(4.0, 0.0, 0.0),
            ..free.clone()
        },
    );
    let other = ball(&mut w, 1.5, free.clone());
    let speed = |w: &World, e| w.get::<Body>(e).unwrap().velocity.x;
    for _ in 0..30 {
        physics::step(&mut w);
    }
    assert_eq!(physics::mass(&w, pushed), Some(2.0));
    // 2 m/s² for half a second; the balls have not met.
    assert!(
        (speed(&w, pushed) - 1.0).abs() < 1e-4,
        "{}",
        speed(&w, pushed)
    );
    assert_eq!(speed(&w, other), 0.0);
    let mut r = restored(&w);
    assert_eq!(r.hash(), w.hash());
    for tick in 0..90 {
        physics::step(&mut w);
        physics::step(&mut r);
        assert_eq!(r.hash(), w.hash(), "tick {tick}");
    }
    assert_eq!(
        r.get::<Body>(pushed).unwrap().force,
        Vec3::new(4.0, 0.0, 0.0)
    );
    // They touched: the force moves both, so their momentum is the force's impulse.
    let momentum = 2.0 * (speed(&w, pushed) + speed(&w, other));
    assert!(speed(&w, other) > 0.0);
    assert!((momentum - 4.0 * 2.0).abs() < 1e-3, "{momentum}");
    w.get_mut::<Body>(pushed).unwrap().force = Vec3::ZERO;
    physics::step(&mut w);
    let total = speed(&w, pushed) + speed(&w, other);
    for _ in 0..30 {
        physics::step(&mut w);
    }
    let after = speed(&w, pushed) + speed(&w, other);
    assert!((after - total).abs() < 1e-5, "{total} then {after}");
}

// The new fields are written only when set (so existing saves and hashes keep their
// bytes), round-trip through JSON, binary and a world save when set, keep their
// value through a patch that omits them, and enter the hash under their names.
#[test]
fn optional_force_and_inertia_round_trip() {
    let set = Body {
        force: Vec3::new(1.0, 2.0, 3.0),
        inertia: Vec3::new(0.5, 0.25, 0.125),
        ..Body::default()
    };
    let plain = json::to_string(&Body::default()).unwrap();
    assert!(
        !plain.contains("force") && !plain.contains("inertia"),
        "{plain}"
    );
    let text = json::to_string(&set).unwrap();
    assert!(
        text.contains("\"force\":[1.0,2.0,3.0]") && text.contains("\"inertia\":[0.5,0.25,0.125]"),
        "{text}"
    );
    assert_eq!(json::from_str::<Body>(&text).unwrap(), set);
    assert_eq!(bin::from_slice::<Body>(&bin::to_vec(&set)).unwrap(), set);
    let mut patched = set.clone();
    json::read_into(r#"{"damping":0.1}"#, &mut patched).unwrap();
    assert_eq!((patched.force, patched.inertia), (set.force, set.inertia));
    let force = Body {
        force: Vec3::ONE,
        ..Body::default()
    };
    let inertia = Body {
        inertia: Vec3::ONE,
        ..Body::default()
    };
    assert_ne!(hash::of(&force), hash::of(&inertia));
    let average = Collider {
        bounce_combine: BounceCombine::Average,
        ..Collider::default()
    };
    assert!(!json::to_string(&Collider::default())
        .unwrap()
        .contains("bounce_combine"));
    assert!(json::to_string(&average)
        .unwrap()
        .contains("\"bounce_combine\":{\"Average\""));
    assert_eq!(
        bin::from_slice::<Collider>(&bin::to_vec(&average)).unwrap(),
        average
    );
    for (body, collider) in [(Body::default(), Collider::default()), (set, average)] {
        let mut w = world();
        let e = w.spawn((Transform::default(), body.clone(), collider.clone()));
        physics::step(&mut w);
        let mut r = restored(&w);
        assert_eq!(r.get::<Body>(e).unwrap().force, body.force);
        assert_eq!(r.get::<Body>(e).unwrap().inertia, body.inertia);
        assert_eq!(*r.get::<Collider>(e).unwrap(), collider);
        assert_eq!(r.hash(), w.hash());
        physics::step(&mut w);
        physics::step(&mut r);
        assert_eq!(r.hash(), w.hash());
    }
}

// A drop's rebound speed over its impact speed is the pair's restitution: the
// larger of the two, or their average when both colliders ask for it.
#[test]
fn average_bounce_needs_both_colliders() {
    for (ball_average, floor_average, want) in [
        (false, false, 0.42),
        (true, false, 0.42),
        (false, true, 0.42),
        (true, true, 0.21),
    ] {
        let combine = |average| {
            if average {
                BounceCombine::Average
            } else {
                BounceCombine::Max
            }
        };
        let mut w = World::new(120, 0);
        physics::register(&mut w);
        w.spawn((
            Transform::at(0.0, -0.5, 0.0),
            Collider {
                shape: Shape::Box {
                    half: Vec3::new(5.0, 0.5, 5.0),
                },
                bounce_combine: combine(floor_average),
                ..Collider::default()
            },
        ));
        let e = w.spawn((
            Transform::at(0.0, 1.0, 0.0),
            Body::default(),
            Collider {
                shape: Shape::Sphere { radius: 0.1 },
                bounce: 0.42,
                bounce_combine: combine(ball_average),
                ..Collider::default()
            },
        ));
        let (mut impact, mut rebound) = (0.0f32, 0.0f32);
        for _ in 0..120 {
            physics::step(&mut w);
            let v = w.get::<Body>(e).unwrap().velocity.y;
            if rebound == 0.0 {
                impact = impact.min(v);
            }
            rebound = rebound.max(v);
        }
        let ratio = rebound / -impact;
        assert!(
            (ratio - want).abs() < 0.02,
            "{ball_average} {floor_average}: {ratio}"
        );
    }
}

// A die's extra shape meets other dice while its primary meets the table: each
// filters by its own layers, adds no mass, is invisible to queries, reports its
// touches as the die (one begin and one end per entity pair, however many of its
// shapes touch), is saved and restored with the world, and goes when removed.
#[test]
fn extra_colliders_share_the_body_and_filter_by_their_own_layers() {
    let mut w = world();
    let floor = w.spawn((
        Transform::at(0.0, -0.5, 0.0),
        Collider {
            shape: Shape::Box {
                half: Vec3::new(20.0, 0.5, 20.0),
            },
            friction: 0.0,
            layer: 3,
            ..Collider::default()
        },
    ));
    let die = w.spawn((
        Transform::at(0.0, 0.5, 0.0),
        Body {
            velocity: Vec3::X * 2.0,
            ..Body::default()
        },
        Collider {
            shape: Shape::Sphere { radius: 0.5 },
            friction: 0.0,
            mask: 1,
            extra: vec![ExtraShape {
                shape: Shape::Box {
                    half: Vec3::splat(0.4),
                },
                offset: Vec3::new(0.0, -0.1, 0.0),
                friction: 0.0,
                layer: 2,
                mask: 2,
                ..ExtraShape::default()
            }],
            ..Collider::default()
        },
    ));
    let other = w.spawn((
        Transform::at(1.5, 0.2, 0.0),
        Body::default(),
        Collider {
            shape: Shape::Box {
                half: Vec3::splat(0.2),
            },
            friction: 0.0,
            layer: 2,
            mask: 2,
            ..Collider::default()
        },
    ));
    let x = |w: &World, e| w.get::<Transform>(e).unwrap().position.x;
    let pair = |a: Entity, b: Entity| (a.min(b), a.max(b));
    let mut log = Vec::new();
    let tick = |w: &mut World, log: &mut Vec<(usize, Entity, Entity, bool)>, n: usize| {
        physics::step(w);
        log.extend(physics::events(w).iter().map(|t| (n, t.a, t.b, t.began)));
    };
    for n in 0..20 {
        tick(&mut w, &mut log, n);
    }
    // The primary sphere's mass alone.
    let sphere = 1000.0 * 4.0 / 3.0 * std::f32::consts::PI * 0.125;
    let mass = physics::mass(&w, die).unwrap();
    assert!((mass - sphere).abs() < 1e-3 * sphere, "{mass}");
    // A ray through the extra box alone, beside the sphere, misses the die.
    let low = Vec3::new(x(&w, die) + 0.39, 0.05, 0.39);
    let hit = physics::raycast(&w, low + Vec3::Y, -Vec3::Y, 2.0, u32::MAX).unwrap();
    assert_eq!(hit.entity, floor);
    let mut r = restored(&w);
    let mut replay = Vec::new();
    for n in 20..60 {
        tick(&mut w, &mut log, n);
        tick(&mut r, &mut replay, n);
        assert_eq!(r.hash(), w.hash(), "tick {n}");
    }
    assert_eq!(
        replay,
        log.iter()
            .filter(|t| t.0 >= 20)
            .copied()
            .collect::<Vec<_>>()
    );
    assert!(x(&w, other) > 1.6, "the box pushed the other die");
    // Sphere and box both rest on the floor: one begin, at the first step.
    let (a, b) = pair(die, floor);
    assert_eq!(log.iter().filter(|t| (t.1, t.2) == (a, b)).count(), 1);
    assert!(log.contains(&(0, a, b, true)), "{log:?}");
    // The other die is met by the box, not the sphere, which reaches it first.
    let (a, b) = pair(die, other);
    let met = log.iter().find(|t| (t.1, t.2) == (a, b)).expect("met").0;
    assert!(2.0 * (met + 1) as f32 / 60.0 > 0.85, "met at tick {met}");
    // Removing the extras ends that touch; the sphere passes through the other
    // die, stopped where it stands.
    w.get_mut::<Collider>(die).unwrap().extra.clear();
    w.get_mut::<Body>(other).unwrap().velocity = Vec3::ZERO;
    tick(&mut w, &mut log, 60);
    assert_eq!(*log.last().unwrap(), (60, a, b, false));
    for n in 61..120 {
        tick(&mut w, &mut log, n);
    }
    assert!(x(&w, die) > x(&w, other) + 0.5, "passed through");
    let json = exact_game::json::to_string(&*w.get::<Collider>(die).unwrap()).unwrap();
    assert!(!json.contains("extra"), "{json}");
}

#[test]
fn extra_shapes_are_refused_without_a_body_and_as_meshes() {
    let mesh = Shape::Mesh {
        vertices: vec![Vec3::ZERO, Vec3::X, Vec3::Z],
        indices: vec![[0, 1, 2]],
    };
    let extra = |shape| Collider {
        extra: vec![ExtraShape {
            shape,
            ..ExtraShape::default()
        }],
        ..Collider::default()
    };
    for (body, collider) in [(false, extra(Shape::default())), (true, extra(mesh))] {
        let mut w = world();
        let e = w.spawn((Transform::default(), collider));
        if body {
            w.insert(e, Body::default());
        }
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| physics::step(&mut w)));
        assert!(result.is_err());
    }
}

// A body resting on the floor is flung up and along in one tick. Coulomb friction
// leaves the lifting contact nothing to hold it (its normal impulse is zero), so
// the body does not spin. Weight-bounded friction keeps each touching point's
// bound mu g m dt for the whole step. A ball has one point at its radius r below
// the centre: the slide loses mu g dt and the spin is mu g m dt r / (2/5 m r²).
// A box of side s has four, half the side below the centre: at most 4 mu g dt off
// the slide and a spin of 12 mu g dt / s (4 rad/s here). Rapier solves both
// tangent directions of a point together, so at a corner part of the bound goes
// across the slide (cancelling between corners) and the box gets about 94% of it.
#[test]
fn weight_friction_acts_on_a_contact_lifting_off() {
    let (mu, g, hz) = (0.5f32, 80.0f32, 120.0f32);
    let lift = |shape: Shape, weight: bool| {
        let mut w = World::new(hz as u32, 0);
        physics::register(&mut w);
        {
            let mut p = w.resource_mut::<physics::Physics>();
            p.coulomb_friction = true;
            p.friction_gravity = if weight { g } else { 0.0 };
            p.friction_slip_speed = 0.1;
        }
        w.spawn((
            Transform::at(0.0, -0.5, 0.0),
            Collider {
                shape: Shape::Box {
                    half: Vec3::new(20.0, 0.5, 20.0),
                },
                friction: mu,
                ..Collider::default()
            },
        ));
        let e = w.spawn((
            Transform::at(0.0, 0.5, 0.0),
            Body::default(),
            Collider {
                shape,
                friction: mu,
                ..Collider::default()
            },
        ));
        for _ in 0..60 {
            physics::step(&mut w);
        }
        let rest = w.get::<Body>(e).unwrap().spin;
        assert!(rest.length() < 1e-3, "settled spin {rest}");
        w.get_mut::<Body>(e).unwrap().velocity = Vec3::new(6.0, 3.0, 0.0);
        physics::step(&mut w);
        let b = w.get::<Body>(e).unwrap().clone();
        assert!(b.spin.x.abs() < 1e-3 && b.spin.y.abs() < 1e-3, "{}", b.spin);
        (6.0 - b.velocity.x, -b.spin.z)
    };
    let ball = Shape::Sphere { radius: 0.5 };
    for shape in [ball.clone(), Shape::default()] {
        let (slowed, spin) = lift(shape, false);
        assert!(slowed.abs() < 1e-3 && spin.abs() < 1e-3, "{slowed} {spin}");
    }
    let (slowed, spin) = lift(ball, true);
    let bound = mu * g / hz;
    assert!((slowed - bound).abs() < 1e-4, "ball slowed {slowed}");
    assert!((spin - bound / 0.4 / 0.5).abs() < 1e-4, "ball spin {spin}");
    let (slowed, spin) = lift(Shape::default(), true);
    let most = 12.0 * bound;
    assert!(
        spin <= most && spin > 0.9 * most,
        "box spin {spin} for at most {most}"
    );
    assert!(
        slowed <= 4.0 * bound && slowed > 0.9 * 4.0 * bound,
        "box slowed {slowed}"
    );
}

// A fast CCD body crossing a thin sensor within one tick touches it neither before
// nor after the step; Rapier's events are the only record. An entity with extras
// (here one that meets nothing) reports that begin and then that end in the tick,
// the events an entity without extras reports.
#[test]
fn a_body_with_extras_reports_a_touch_within_one_step() {
    let run = |extra: bool| {
        let mut w = world();
        let sensor = w.spawn((
            Transform::at(1.0, 0.0, 0.0),
            Collider {
                shape: Shape::Box {
                    half: Vec3::new(0.01, 1.0, 1.0),
                },
                sensor: true,
                ..Collider::default()
            },
        ));
        let bullet = w.spawn((
            Transform::default(),
            Body {
                velocity: Vec3::X * 120.0,
                gravity: 0.0,
                ccd: true,
                ..Body::default()
            },
            Collider {
                shape: Shape::Sphere { radius: 0.05 },
                extra: if extra {
                    vec![ExtraShape {
                        mask: 0,
                        ..ExtraShape::default()
                    }]
                } else {
                    Vec::new()
                },
                ..Collider::default()
            },
        ));
        let mut log = Vec::new();
        for n in 0..5 {
            physics::step(&mut w);
            log.extend(physics::events(&w).iter().map(|t| (n, t.a, t.b, t.began)));
        }
        assert!(w.get::<Transform>(bullet).unwrap().position.x > 1.0);
        (log, sensor, bullet)
    };
    let (plain, sensor, bullet) = run(false);
    let (a, b) = (sensor.min(bullet), sensor.max(bullet));
    let (shaped, ..) = run(true);
    let n = shaped.first().expect("a touch").0;
    assert_eq!(shaped, vec![(n, a, b, true), (n, a, b, false)]);
    let mut plain = plain;
    plain.sort();
    let mut sorted = shaped;
    sorted.sort();
    assert_eq!(sorted, plain);
}

// A body held up by a force falls once the force is cleared, asleep or not.
#[test]
fn clearing_a_force_wakes_its_body() {
    let mut w = world();
    let e = ball(
        &mut w,
        0.0,
        Body {
            mass: 1.0,
            force: Vec3::new(0.0, 9.81, 0.0),
            ..Body::default()
        },
    );
    for _ in 0..300 {
        physics::step(&mut w);
    }
    assert!(w.get::<Body>(e).unwrap().asleep);
    w.get_mut::<Body>(e).unwrap().force = Vec3::ZERO;
    for _ in 0..30 {
        physics::step(&mut w);
    }
    assert!(w.get::<Transform>(e).unwrap().position.y < -1.0);
}

// Weight-bounded contacts replay exactly after a restore.
#[test]
fn weight_friction_replays_after_a_restore() {
    let mut w = World::new(120, 0);
    physics::register(&mut w);
    {
        let mut p = w.resource_mut::<physics::Physics>();
        p.coulomb_friction = true;
        p.friction_gravity = 9.81;
        p.friction_slip_speed = 0.2;
    }
    w.spawn((
        Transform::at(0.0, -0.5, 0.0),
        Collider {
            shape: Shape::Box {
                half: Vec3::new(20.0, 0.5, 20.0),
            },
            ..Collider::default()
        },
    ));
    for i in 0..3 {
        let f = i as f32;
        w.spawn((
            Transform::at(f * 1.5, 0.3 + 0.2 * f, 0.0),
            Body {
                velocity: Vec3::new(4.0 + f, 0.0, 1.0 - f),
                spin: Vec3::new(0.0, f, 2.0),
                ..Body::default()
            },
            Collider {
                shape: Shape::Box {
                    half: Vec3::splat(0.25),
                },
                ..Collider::default()
            },
        ));
    }
    for _ in 0..20 {
        physics::step(&mut w);
    }
    let mut r = restored(&w);
    for tick in 0..120 {
        physics::step(&mut w);
        physics::step(&mut r);
        assert_eq!(r.hash(), w.hash(), "tick {tick}");
    }
}

// Rapier's contact softness divides by the damping ratio: zero is refused.
#[test]
fn zero_contact_damping_is_refused() {
    let mut w = world();
    w.resource_mut::<physics::Physics>().contact_damping_ratio = 0.0;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| physics::step(&mut w)));
    assert!(result.is_err());
}

// A wall touching a lifted body carries no load. Weight friction must not turn
// its upward motion into a spin, including when gravity points along another axis.
#[test]
fn weight_friction_ignores_unloaded_walls_in_either_gravity_direction() {
    for sideways in [false, true] {
        let rotate = |v: Vec3| {
            if sideways {
                Vec3::new(v.y, -v.x, v.z)
            } else {
                v
            }
        };
        let mut w = World::new(120, 0);
        physics::register(&mut w);
        {
            let mut p = w.resource_mut::<physics::Physics>();
            p.gravity = rotate(Vec3::new(0.0, -9.81, 0.0));
            p.coulomb_friction = true;
            p.friction_gravity = 5040.0;
        }
        w.spawn((
            Transform {
                position: rotate(Vec3::new(-0.5, 0.0, 0.0)),
                ..Transform::default()
            },
            Collider {
                shape: Shape::Box {
                    half: if sideways {
                        Vec3::new(10.0, 0.5, 10.0)
                    } else {
                        Vec3::new(0.5, 10.0, 10.0)
                    },
                },
                ..Collider::default()
            },
        ));
        let e = w.spawn((
            Transform {
                position: rotate(Vec3::new(0.5, 0.0, 0.0)),
                ..Transform::default()
            },
            Body {
                velocity: rotate(Vec3::new(0.0, 3.0, 0.0)),
                gravity: 0.0,
                ..Body::default()
            },
            Collider::default(),
        ));
        for _ in 0..8 {
            physics::step(&mut w);
        }
        let b = w.get::<Body>(e).unwrap();
        assert!(b.spin.length() < 1e-3, "sideways {sideways}: {}", b.spin);
        assert!((b.velocity.length() - 3.0).abs() < 1e-3, "{}", b.velocity);
    }
}
