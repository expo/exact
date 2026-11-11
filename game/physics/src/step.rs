use crate::{
    math,
    state::{raw, Entry, Live, Synced},
    Announce, Body, BodyKind, BounceCombine, Collider, Physics, Shape, Touch,
};
use exact_game::{Entity, Parent, Transform, World};
use rapier3d::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

pub(crate) fn body_type(kind: BodyKind) -> RigidBodyType {
    match kind {
        BodyKind::Static => RigidBodyType::Fixed,
        BodyKind::Kinematic => RigidBodyType::KinematicPositionBased,
        BodyKind::Dynamic => RigidBodyType::Dynamic,
    }
}
pub(crate) fn collider(c: &Collider, t: Transform, body: Option<&Body>) -> ColliderBuilder {
    try_collider(c, t, body).unwrap_or_else(|e| panic!("{e}"))
}
// Every refusal reachable while building a collider, for a save's rebuilt holes.
pub(crate) fn try_collider(
    c: &Collider,
    t: Transform,
    body: Option<&Body>,
) -> Result<ColliderBuilder, &'static str> {
    math::check(
        c.friction.is_finite() && c.friction >= 0.0 && (0.0..=1.0).contains(&c.bounce),
        "physics: invalid material",
    )?;
    math::check(
        !matches!(c.shape, Shape::Mesh { .. } | Shape::Heightfield { .. })
            || body.is_none_or(|b| b.kind == BodyKind::Static),
        "physics: mesh/heightfield must be static",
    )?;
    math::check(c.offset.is_finite(), "physics: invalid collider offset")?;
    let shape = math::try_shape(&c.shape, t.scale)?;
    let offset = math::try_pose(Transform::at(
        c.offset.x * t.scale.x,
        c.offset.y * t.scale.y,
        c.offset.z * t.scale.z,
    ))?;
    let mass = body.filter(|b| b.mass > 0.0).map(|b| b.mass);
    let rounded = math::rounded_hull_mass(&shape, 1000.0).map(|mut m| {
        if let Some(mass) = mass {
            m.set_mass(mass, true);
        }
        m.transform_by(&offset)
    });
    let shape = if c.offset == exact_game::Vec3::ZERO {
        shape
    } else {
        SharedShape::compound(vec![(offset, shape)])
    };
    // An authored inertia keeps the derived centre of mass and mass, with its
    // principal axes along the body's.
    let inertia = body
        .map(|b| b.inertia)
        .filter(|i| *i != exact_game::Vec3::ZERO);
    let rounded = match inertia {
        Some(i) => {
            let mut m = rounded.unwrap_or_else(|| shape.mass_properties(1000.0));
            if let Some(mass) = mass {
                m.set_mass(mass, false);
            }
            Some(MassProperties::new(m.local_com, m.mass(), math::vector(i)))
        }
        None => rounded,
    };
    let builder = ColliderBuilder::new(shape)
        .friction(c.friction)
        .friction_combine_rule(CoefficientCombineRule::GeometricMean)
        .restitution(c.bounce)
        .restitution_combine_rule(restitution_rule(c))
        .sensor(c.sensor)
        .collision_groups(InteractionGroups::new(
            Group::from_bits_retain(c.layer),
            Group::from_bits_retain(c.mask),
            InteractionTestMode::And,
        ))
        .active_collision_types(collision_types(c.sensor))
        .active_events(ActiveEvents::COLLISION_EVENTS);
    Ok(match (rounded, mass) {
        (Some(m), _) => builder.mass_properties(m),
        (None, Some(mass)) => builder.mass(mass),
        (None, None) => builder.density(1000.0),
    })
}
// Rapier combines a pair by the higher-priority rule of the two, in the order of
// `BounceCombine`'s variants.
fn restitution_rule(c: &Collider) -> CoefficientCombineRule {
    match c.bounce_combine {
        BounceCombine::Average => CoefficientCombineRule::Average,
        BounceCombine::Min => CoefficientCombineRule::Min,
        BounceCombine::Multiply => CoefficientCombineRule::Multiply,
        BounceCombine::Max => CoefficientCombineRule::Max,
    }
}
// The rows one sync visits. Every other row equals its entry by construction:
// the last sync recorded it and nothing has written it since, so visiting it would
// change nothing. Parented colliders follow their ancestors and every body is
// borrowed mutably (Rapier lists it as modified, in entity order) and kinematic
// ones retarget every step, so both are always visited. Static colliders without
// a Body cost nothing. The first sync of a Live in a world (after setup, restore
// or clone) visits every row.
pub(crate) struct Pending {
    rows: Vec<Entity>,
    gone: Vec<Entity>,
    full: bool,
    synced: Synced,
}
pub(crate) fn pending(world: &World, live: &Live) -> Pending {
    let synced = Synced {
        world: world.id(),
        presentation: world.presentation_generation(),
        revisions: [
            world.revision::<Body>(),
            world.revision::<Collider>(),
            world.revision::<Transform>(),
            world.revision::<Parent>(),
        ],
    };
    let since = live
        .synced
        .as_ref()
        .filter(|s| s.world == synced.world && s.presentation == synced.presentation);
    let Some(since) = since else {
        return Pending {
            rows: world
                .query::<Option<&Body>>()
                .with_any::<Body, Collider>()
                .iter()
                .map(|(e, _)| e)
                .collect(),
            gone: live.entries.keys().copied().collect(),
            full: true,
            synced,
        };
    };
    let mut at = BTreeMap::new();
    let r = (since.revisions, synced.revisions);
    let mut note = |e: Entity| {
        at.insert(e.index(), e);
    };
    if r.0[0] != r.1[0] {
        world.changed::<Body>(r.0[0]).for_each(&mut note);
    }
    if r.0[1] != r.1[1] {
        world.changed::<Collider>(r.0[1]).for_each(&mut note);
    }
    if r.0[2] != r.1[2] {
        world.changed::<Transform>(r.0[2]).for_each(&mut note);
    }
    if r.0[3] != r.1[3] {
        world.changed::<Parent>(r.0[3]).for_each(&mut note);
    }
    for e in live.parented.iter().chain(&live.bodies) {
        at.entry(e.index()).or_insert(*e);
    }
    Pending {
        gone: at
            .keys()
            .filter_map(|i| live.slots.get(i).copied())
            .collect(),
        rows: at
            .into_values()
            .filter(|e| world.contains(*e) && (world.has::<Body>(*e) || world.has::<Collider>(*e)))
            .collect(),
        full: false,
        synced,
    }
}
// Two fixed solids never exchange impulses; their contact manifolds were the bulk
// of a static world's snapshot and of each step's pair walk. A sensor still
// reports touching anything, fixed or not.
fn collision_types(sensor: bool) -> ActiveCollisionTypes {
    if sensor {
        ActiveCollisionTypes::all()
    } else {
        ActiveCollisionTypes::all() - ActiveCollisionTypes::FIXED_FIXED
    }
}
pub(crate) fn sync(world: &World, live: &mut Live, pending: &Pending) -> bool {
    let mut moved_kinematic = false;
    let mut wake = false;
    let mut sleeps = Vec::new();
    if pending.full {
        live.parented.clear();
    }
    for &e in &pending.rows {
        if let Some(h) = live.entries.get(&e).and_then(|entry| entry.collider_handle) {
            live.elidable.remove(&h);
        }
        live.digests.remove(&e);
        let b = world.get::<Body>(e);
        let c = world.get::<Collider>(e);
        let t = world.get::<Transform>(e);
        let parent = world.get::<Parent>(e);
        let (b, c, t, parent) = (b.as_deref(), c.as_deref(), t.as_deref(), parent.as_deref());
        if let Some(c) = c {
            assert!(
                c.extra.is_empty() || b.is_some(),
                "physics: extra shapes need a Body"
            );
            assert!(
                c.extra
                    .iter()
                    .all(|x| !matches!(x.shape, Shape::Mesh { .. } | Shape::Heightfield { .. })),
                "physics: an extra shape cannot be a mesh or a heightfield"
            );
        }
        if b.is_some() {
            assert!(
                parent.is_none(),
                "Body `{}` must be a root",
                world.name(e).unwrap_or("unnamed")
            );
        }
        let t = if parent.is_some() {
            math::world_pose(world, e)
        } else {
            t.copied().unwrap_or_default()
        };
        let entry = live.entries.entry(e).or_insert_with(|| Entry {
            entity: e,
            ..Entry::default()
        });
        let r = &mut live.rapier;
        let body_changed = entry.body.as_ref() != b;
        let pose_changed = entry.pose != t;
        if let Some(b) = b {
            assert!(
                b.velocity.is_finite()
                    && b.spin.is_finite()
                    && b.mass.is_finite()
                    && b.mass >= 0.0
                    && b.gravity.is_finite()
                    && b.damping.is_finite()
                    && b.damping >= 0.0
                    && b.spin_damping.is_finite()
                    && b.spin_damping >= 0.0
                    && b.force.is_finite()
                    && b.inertia.is_finite()
                    && (b.inertia == exact_game::Vec3::ZERO || b.inertia.min_element() > 0.0),
                "physics: invalid body"
            );
            if entry.body_handle.is_none() {
                let h = r.bodies.insert(
                    RigidBodyBuilder::new(body_type(b.kind))
                        .pose(math::pose(t))
                        .linvel(math::vector(b.velocity))
                        .angvel(math::vector(b.spin))
                        .gravity_scale(b.gravity)
                        .linear_damping(b.damping)
                        .angular_damping(b.spin_damping)
                        .ccd_enabled(b.ccd)
                        .sleeping(b.asleep),
                );
                let (i, g) = h.into_raw_parts();
                entry.body_handle = Some([i, g]);
                if let Some(ch) = entry.ch() {
                    r.colliders.set_parent(ch, Some(h), &mut r.bodies);
                }
            }
            let rb = &mut r.bodies[entry.bh().unwrap()];
            if body_changed || entry.collider.is_some() != c.is_some() {
                let mass = if c.is_some() {
                    0.0
                } else if b.mass > 0.0 {
                    b.mass
                } else {
                    1.0
                };
                if c.is_none() && b.inertia != exact_game::Vec3::ZERO {
                    rb.set_additional_mass_properties(
                        MassProperties::new(Vector::ZERO, mass, math::vector(b.inertia)),
                        true,
                    );
                } else {
                    rb.set_additional_mass(mass, true);
                }
            }
            if body_changed {
                rb.set_body_type(body_type(b.kind), true);
                rb.set_gravity_scale(b.gravity, true);
                rb.set_linear_damping(b.damping);
                rb.set_angular_damping(b.spin_damping);
                rb.enable_ccd(b.ccd);
                if entry
                    .body
                    .as_ref()
                    .is_none_or(|p| p.velocity != b.velocity || p.spin != b.spin)
                {
                    rb.set_linvel(math::vector(b.velocity), true);
                    rb.set_angvel(math::vector(b.spin), true);
                }
                if entry.body.as_ref().is_none_or(|p| p.asleep != b.asleep) {
                    if b.asleep {
                        sleeps.push((entry.bh().unwrap(), math::pose(t)));
                    } else {
                        rb.wake_up(true);
                    }
                }
            }
            // Rapier keeps a dynamic body's force until it is reset, and saves it;
            // `add_force` ignores other kinds. Both calls do nothing at zero, and
            // either wakes the body when it changes a force.
            if entry
                .body
                .as_ref()
                .is_none_or(|p| p.force != b.force || p.kind != b.kind)
            {
                rb.reset_forces(true);
                rb.add_force(math::vector(b.force), true);
            }
            if b.kind == BodyKind::Kinematic {
                moved_kinematic |= pose_changed;
                rb.set_next_kinematic_position(math::pose(t));
            } else if pose_changed {
                rb.set_position(math::pose(t), true);
            }
        } else if let Some(h) = entry.bh() {
            if let Some(ch) = entry.ch() {
                r.colliders.set_parent(ch, None, &mut r.bodies);
            }
            r.remove_body(h);
            entry.body_handle = None;
            wake = true;
        }
        // The collider's mass properties derive from its shape, offset and scale and
        // the body's mass. They are set only when one of those changes, so a material
        // edit keeps the ones a restored world carries.
        let solid_changed = entry.body.as_ref().map(|b| (b.mass, b.inertia))
            != b.map(|b| (b.mass, b.inertia))
            || entry.pose.scale != t.scale
            || entry.collider.as_ref().map(|p| (&p.shape, p.offset))
                != c.map(|c| (&c.shape, c.offset));
        let collider_changed = solid_changed || entry.collider.as_ref() != c;
        if let Some(c) = c {
            assert!(
                !matches!(c.shape, Shape::Mesh { .. } | Shape::Heightfield { .. })
                    || b.is_none_or(|b| b.kind == BodyKind::Static),
                "physics: mesh/heightfield must be static"
            );
            if entry.collider_handle.is_none() {
                let cb = collider(c, t, b).position(if b.is_some() {
                    Pose::IDENTITY
                } else {
                    math::pose(t)
                });
                let h = r.insert_collider(cb, entry.bh());
                entry.collider_handle = Some(raw(h));
                live.reverse.insert(raw(h), e);
            } else if collider_changed {
                let builder = collider(c, t, b);
                let mass = builder.mass_properties.clone();
                let built = builder.build();
                let co = &mut r.colliders[entry.ch().unwrap()];
                co.set_shape(built.shared_shape().clone());
                co.set_friction(c.friction);
                co.set_restitution(c.bounce);
                co.set_restitution_combine_rule(restitution_rule(c));
                co.set_sensor(c.sensor);
                co.set_active_collision_types(built.active_collision_types());
                co.set_collision_groups(built.collision_groups());
                if solid_changed {
                    match mass {
                        ColliderMassProps::Density(d) => co.set_density(d),
                        ColliderMassProps::Mass(m) => co.set_mass(m),
                        ColliderMassProps::MassProperties(m) => co.set_mass_properties(*m),
                    }
                }
                wake = true;
            }
            if b.is_none() && pose_changed {
                r.colliders[entry.ch().unwrap()].set_position(math::pose(t));
                wake = true;
            }
        } else if let Some(h) = entry.ch() {
            r.remove_collider(h);
            live.removed.push(raw(h));
            entry.collider_handle = None;
            wake = true;
        }
        // Extras are rebuilt whenever the primary is: they are few, and a rebuild
        // keeps their handles in `Collider::extra` order.
        if collider_changed || c.is_none() {
            for h in entry.extra_handles.drain(..) {
                r.remove_collider(ColliderHandle::from_raw_parts(h[0], h[1]));
                live.removed.push(h);
                wake = true;
            }
            if let (Some(c), Some(bh)) = (c, entry.bh()) {
                for x in &c.extra {
                    let cb = collider(&x.collider(), t, None)
                        .density(0.0)
                        .position(Pose::IDENTITY);
                    let h = raw(r.insert_collider(cb, Some(bh)));
                    entry.extra_handles.push(h);
                    live.reverse.insert(h, e);
                }
            }
            if entry.extra_handles.is_empty() {
                live.extended.remove(&e);
            } else {
                live.extended.insert(e);
            }
        }
        if body_changed {
            entry.body = b.cloned();
        }
        if collider_changed {
            entry.collider = c.cloned();
        }
        entry.pose = t;
        if entry.body_handle.is_some() {
            live.bodies.insert(e);
        } else {
            live.bodies.remove(&e);
        }
        if parent.is_some() {
            live.parented.insert(e);
        } else {
            live.parented.remove(&e);
        }
        live.slots.insert(e.index(), e);
    }
    for e in &pending.gone {
        if world.has::<Body>(*e) || world.has::<Collider>(*e) {
            continue;
        }
        let entry = live.entries.remove(e).unwrap();
        live.digests.remove(e);
        if let Some(h) = entry.bh() {
            live.rapier.remove_body(h);
        } else if let Some(h) = entry.ch() {
            live.rapier.remove_collider(h);
        }
        live.removed.extend(entry.colliders());
        live.extended.remove(e);
        live.bodies.remove(e);
        live.parented.remove(e);
        if live.slots.get(&e.index()) == Some(e) {
            live.slots.remove(&e.index());
        }
        wake = true;
    }
    live.synced = Some(pending.synced.clone());
    if wake {
        live.rapier.wake_up_all(true);
    }
    // Rapier sleeps whole islands. `RigidBody::sleep` would flag one body alone:
    // the solver then skips its contacts while the awake island still integrates
    // it. Stop it and count it as at rest where it stands instead, after every
    // wake above; its island sleeps in the next step if every other member is at
    // rest too.
    for (h, pose) in sleeps {
        let rb = &mut live.rapier.bodies[h];
        rb.set_linvel(Vector::ZERO, false);
        rb.set_angvel(AngVector::ZERO, false);
        rb.activation_mut().request_sleep(pose);
    }
    moved_kinematic
}
// Sleeping ticks still execute exactly one Rapier step. Only skip reflection and
// writeback when the ordered comparison proves no component/configuration edits.
fn unchanged_asleep(world: &World, live: &Live, pending: &Pending) -> bool {
    if !live.parented.is_empty() || (pending.full && pending.rows.len() != live.entries.len()) {
        return false;
    }
    for &e in &pending.rows {
        let Some(p) = live.entries.get(&e) else {
            return false;
        };
        if world.has::<Parent>(e)
            || p.body.as_ref() != world.get::<Body>(e).as_deref()
            || p.collider.as_ref() != world.get::<Collider>(e).as_deref()
            || p.pose != world.get::<Transform>(e).map(|t| *t).unwrap_or_default()
        {
            return false;
        }
    }
    if !pending.full
        && pending
            .gone
            .iter()
            .any(|e| pending.rows.binary_search(e).is_err())
    {
        return false;
    }
    live.bodies.iter().all(|e| {
        let b = live.entries[e].body.as_ref().unwrap();
        !((b.kind == BodyKind::Dynamic && !b.asleep)
            || (b.kind == BodyKind::Kinematic
                && (b.velocity != exact_game::Vec3::ZERO || b.spin != exact_game::Vec3::ZERO)))
    })
}
// The Physics resource's solver settings, as Rapier's integration parameters.
// At their defaults they write the values Rapier already holds.
fn configure(p: &Physics, ip: &mut IntegrationParameters, dt: f32) {
    assert!(
        p.solver_iterations >= 1
            && [p.contact_frequency, p.static_contact_frequency]
                .iter()
                .all(|f| f.is_finite() && *f > 0.0)
            // Rapier's contact softness divides by the damping ratio.
            && p.contact_damping_ratio.is_finite()
            && p.contact_damping_ratio > 0.0
            && [p.friction_gravity, p.friction_slip_speed]
                .iter()
                .all(|v| v.is_finite() && *v >= 0.0)
            && (p.friction_gravity == 0.0 || p.coulomb_friction),
        "physics: invalid solver settings"
    );
    // Not saved by Rapier: the resource holds them.
    ip.friction_gravity = p.friction_gravity;
    ip.friction_slip_speed = p.friction_slip_speed;
    ip.friction_up = math::vector(-p.gravity.normalize_or_zero());
    ip.dt = dt;
    ip.num_solver_iterations = p.solver_iterations as usize;
    ip.contact_softness = SpringCoefficients::new(p.contact_frequency, p.contact_damping_ratio);
    ip.static_contact_softness =
        SpringCoefficients::new(p.static_contact_frequency, p.contact_damping_ratio);
    ip.friction_model = if p.coulomb_friction {
        FrictionModel::Coulomb
    } else {
        FrictionModel::Simplified
    };
}
// The entity pairs touching through any collider of the given entities, as their
// collision events last reported it: a contact pair whose last event was a start,
// or an intersecting sensor pair.
fn touching(live: &Live, of: &BTreeMap<Entity, Vec<[u32; 2]>>) -> BTreeSet<(Entity, Entity)> {
    let np = &live.rapier.narrow_phase;
    let mut out = BTreeSet::new();
    for (&e, handles) in of {
        for &[i, g] in handles {
            let h = ColliderHandle::from_raw_parts(i, g);
            let other = |a: ColliderHandle, b: ColliderHandle| if a == h { b } else { a };
            let contacts = np
                .contact_pairs_with(h)
                .filter(|p| p.start_event_emitted())
                .map(|p| other(p.collider1, p.collider2));
            let sensors = np
                .intersection_pairs_with(h)
                .filter(|p| p.2)
                .map(|(a, b, _)| other(a, b));
            for o in contacts.chain(sensors) {
                if let Some(&o) = live.reverse.get(&raw(o)).filter(|o| **o != e) {
                    out.insert((e.min(o), e.max(o)));
                }
            }
        }
    }
    out
}
#[derive(Default)]
struct Events(Mutex<Vec<CollisionEvent>>);
impl EventHandler for Events {
    fn handle_collision_event(
        &self,
        _: &RigidBodySet,
        _: &ColliderSet,
        e: CollisionEvent,
        _: Option<&ContactPair>,
    ) {
        self.0.lock().unwrap().push(e);
    }
    fn handle_contact_force_event(
        &self,
        _: f32,
        _: &RigidBodySet,
        _: &ColliderSet,
        _: &ContactPair,
        _: f32,
    ) {
    }
}
/// Advance one PhysicsPipeline step at exactly `world.dt()`, reflecting game edits
/// and publishing poses, velocities, sleeping state and ordered collision events.
pub fn step(world: &mut World) {
    let _span = world.span("physics::step");
    let physics = world.resource::<Physics>();
    assert!(physics.gravity.is_finite(), "physics: invalid gravity");
    let mut saved = physics.executor.0.borrow_mut();
    saved.dirty = true;
    let live = saved.live();
    let gravity = math::vector(physics.gravity);
    let pending = pending(world, live);
    if live.rapier.gravity == gravity && unchanged_asleep(world, live, &pending) {
        live.synced = Some(pending.synced);
        configure(
            &physics,
            &mut live.rapier.integration_parameters,
            world.dt(),
        );
        live.rapier.step();
        drop(saved);
        drop(physics);
        // The world hash keys a resource's digest by its revision, which only a
        // mutable borrow advances; the executor changed behind a shared one. Every
        // path through a step must end with resource_mut::<Physics>().
        world.resource_mut::<Physics>().events.clear();
        return;
    }
    // A touch through an extra collider is its entity's: an entity pair begins when
    // its first collider pair does and ends with its last. So the events of every
    // entity with extras, before or after this sync, come from comparing the
    // touching entity pairs before and after the step rather than from Rapier's
    // per-collider events.
    let mut shaped: BTreeMap<Entity, Vec<[u32; 2]>> = live
        .extended
        .iter()
        .map(|e| (*e, live.entries[e].colliders().collect()))
        .collect();
    let moved_kinematic = sync(world, live, &pending);
    for e in &live.extended {
        shaped
            .entry(*e)
            .or_insert_with(|| live.entries[e].colliders().collect());
    }
    let before = touching(live, &shaped);
    if live.rapier.gravity != gravity {
        live.rapier.wake_up_all(true);
    }
    live.rapier.gravity = gravity;
    configure(
        &physics,
        &mut live.rapier.integration_parameters,
        world.dt(),
    );
    let collector = Events::default();
    live.rapier.step_with_events(&(), &collector);
    // Rapier detects contacts before integrating next kinematic poses. Refresh
    // collisions at the resulting poses so same-tick sensor transitions survive.
    if moved_kinematic {
        let r = &mut live.rapier;
        for entry in live.bodies.iter().map(|e| &live.entries[e]).filter(|e| {
            e.body
                .as_ref()
                .is_some_and(|b| b.kind == BodyKind::Kinematic)
        }) {
            for h in entry.colliders() {
                let co = &mut r.colliders[ColliderHandle::from_raw_parts(h[0], h[1])];
                co.set_position(*co.position());
            }
        }
        CollisionPipeline::new().step(
            r.integration_parameters.prediction_distance(),
            &mut r.islands,
            &mut r.broad_phase,
            &mut r.narrow_phase,
            &mut r.bodies,
            &mut r.colliders,
            &(),
            &collector,
        );
    }
    let mut events: Vec<_> = collector
        .0
        .into_inner()
        .unwrap()
        .into_iter()
        .filter_map(|event| {
            let &a = live.reverse.get(&raw(event.collider1()))?;
            let &b = live.reverse.get(&raw(event.collider2()))?;
            Some(Touch {
                a: a.min(b),
                b: a.max(b),
                began: event.started(),
            })
        })
        .collect();
    let mut crossed = BTreeSet::new();
    if !shaped.is_empty() {
        let mut transient = BTreeSet::new();
        events.retain(|t| {
            let keep = !shaped.contains_key(&t.a) && !shaped.contains_key(&t.b);
            if !keep && t.began {
                transient.insert((t.a, t.b));
            }
            keep
        });
        for (e, handles) in shaped.iter_mut() {
            *handles = live
                .entries
                .get(e)
                .map(|entry| entry.colliders().collect())
                .unwrap_or_default();
        }
        let after = touching(live, &shaped);
        events.extend(before.symmetric_difference(&after).map(|&(a, b)| Touch {
            a,
            b,
            began: after.contains(&(a, b)),
        }));
        // A pair that begins and ends within the step (a kinematic or CCD body
        // passing through a sensor) touches neither before nor after; Rapier's
        // events are its only record, so report both, the begin first.
        crossed.extend(
            transient
                .into_iter()
                .filter(|p| !before.contains(p) && !after.contains(p)),
        );
        events.extend(
            crossed
                .iter()
                .flat_map(|&(a, b)| [true, false].map(|began| Touch { a, b, began })),
        );
    }
    events.sort_by_key(|e| (e.a, e.b, e.began != crossed.contains(&(e.a, e.b))));
    events.dedup();
    for h in live.removed.drain(..) {
        live.reverse.remove(&h);
        live.elidable.remove(&h);
    }
    for e in &live.bodies {
        live.digests.remove(e);
        let entry = live.entries.get_mut(e).unwrap();
        let handle = entry.bh().unwrap();
        let rb = &live.rapier.bodies[handle];
        let b = entry.body.as_mut().unwrap();
        b.velocity = math::vec3(rb.linvel());
        b.spin = math::vec3(rb.angvel());
        b.asleep = rb.is_sleeping();
        if b.kind == BodyKind::Dynamic {
            entry.pose = math::transform(rb.position(), entry.pose.scale);
        }
    }
    // Transfer the existing entries across the resource lease for writeback.
    // Only differing values are written, so resting bodies leave their rows unchanged.
    let entries = std::mem::take(&mut live.entries);
    let bodies = std::mem::take(&mut live.bodies);
    drop(saved);
    drop(physics);
    let mut awake = false;
    for entry in bodies.iter().map(|e| &entries[e]) {
        let body = entry.body.as_ref().unwrap();
        awake |= body.kind == BodyKind::Dynamic && !body.asleep;
        if world.get::<Transform>(entry.entity).as_deref() != Some(&entry.pose) {
            world.insert(entry.entity, entry.pose);
        }
        if world.get::<Body>(entry.entity).as_deref() != Some(body) {
            world.insert(entry.entity, body.clone());
        }
    }
    for e in &events {
        if world.has::<Announce>(e.a) || world.has::<Announce>(e.b) {
            let name = |e| {
                world
                    .name(e)
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("#{}", e.index()))
            };
            world.log(format!(
                "{} {} × {}",
                if e.began { "touch" } else { "untouch" },
                name(e.a),
                name(e.b)
            ));
        }
    }
    let mut physics = world.resource_mut::<Physics>();
    let live = physics.executor.0.get_mut().live();
    live.entries = entries;
    live.bodies = bodies;
    physics.events = events;
    drop(physics);
    if awake {
        world.busy("physics: awake dynamic bodies");
    }
}
