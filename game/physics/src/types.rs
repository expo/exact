use exact_game::{Component, Data, Entity, Transform, Vec3};

/// How a collider participates in simulation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Data)]
pub enum BodyKind {
    /// Never moved by physics; zero inverse mass.
    Static,
    /// The game writes its pose; the solver observes its pose delta.
    Kinematic,
    /// Forces and contact impulses move this body.
    #[default]
    Dynamic,
}

/// A rigid body's controls and observed motion. Bodies must be hierarchy roots.
#[derive(Clone, Debug, PartialEq)]
pub struct Body {
    /// Static, game-controlled, or simulated (the default).
    pub kind: BodyKind,
    /// World-space linear velocity in metres per second.
    pub velocity: Vec3,
    /// World-space angular velocity in radians per second.
    pub spin: Vec3,
    /// Kilograms; zero derives mass from volume at 1000 kg/m³.
    pub mass: f32,
    /// Multiplier of the Physics resource's gravity; defaults to one.
    pub gravity: f32,
    /// Linear drag rate in inverse seconds; defaults to zero.
    pub damping: f32,
    /// Angular drag rate in inverse seconds; defaults to zero.
    pub spin_damping: f32,
    /// Whether the whole touching island has gone to sleep. Islands sleep together:
    /// writing true stops the body and counts it as at rest. It sleeps in the next
    /// step only if the rest of its island (the bodies it touches, and those they
    /// touch) is at rest too; otherwise it stays awake, simulated, and reads false
    /// until its island sleeps. A touch from a moving body wakes it, and writing
    /// false wakes its island.
    pub asleep: bool,
    /// Continuous collision detection for a fast dynamic body: each step sweeps
    /// its motion against every other collider, so it cannot pass through a thin
    /// or small one between steps. Two bodies that both set it are not swept
    /// against each other. Without it, a fast body is swept only against static
    /// colliders. Ignored on static and kinematic bodies. Saved, hashed and
    /// serialized only when true, and a read that omits it keeps the current
    /// value: reading a serialized body without CCD into one with CCD leaves CCD
    /// on. To turn it off, write the field.
    pub ccd: bool,
    /// A world-space force at the centre of mass in newtons, applied in every step
    /// until it is written back to zero. Contacts still act on the body, as they do
    /// under gravity. Ignored on static and kinematic bodies. Writing a non-zero
    /// force wakes the body; a body at rest under a constant force may sleep, as
    /// under gravity. Saved, hashed and serialized only when non-zero, with the
    /// omission rule of `ccd`.
    pub force: Vec3,
    /// Principal moments of inertia in kg·m² about the centre of mass, along the
    /// body's local axes. All zero (the default) derives them from the collider;
    /// otherwise all three must be positive. Saved, hashed and serialized only when
    /// set, with the omission rule of `ccd`.
    pub inertia: Vec3,
}
impl Default for Body {
    fn default() -> Self {
        Self {
            kind: BodyKind::Dynamic,
            velocity: Vec3::ZERO,
            spin: Vec3::ZERO,
            mass: 0.0,
            gravity: 1.0,
            damping: 0.0,
            spin_damping: 0.0,
            asleep: false,
            ccd: false,
            force: Vec3::ZERO,
            inertia: Vec3::ZERO,
        }
    }
}
/// The derive's record, plus fields written only when they differ from the type's
/// default, so a value that never sets them keeps the saves and hashes it has.
/// Reading keeps the record rule: an omitted field keeps its current value, which
/// is the default when decoding a new value. The first list is always written; the
/// second is `Body::ccd`, which predates the third; the third is named in the hash
/// as well (`Writer::key`), since a hash does not name fields and two of them
/// written alone could otherwise collide.
macro_rules! record {
    ($ty:ident { $($field:ident),* $(,)? } unnamed { $($bare:ident),* $(,)? } named { $($opt:ident),* $(,)? }) => {
        impl exact_game::Data for $ty {
            const TYPE_ID: &'static str = concat!("table:", module_path!(), "::", stringify!($ty));
            fn set_field(&mut self, field: &str, text: &str) -> Result<(), exact_game::DataError> {
                match field {
                    $(stringify!($field) => exact_game::data::edit_field(&mut self.$field, text),)*
                    $(stringify!($bare) => exact_game::data::edit_field(&mut self.$bare, text),)*
                    $(stringify!($opt) => exact_game::data::edit_field(&mut self.$opt, text),)*
                    _ => Err(exact_game::DataError::new(format!("unknown field {field}"))),
                }
            }
            fn write(&self, w: &mut dyn exact_game::Writer) {
                let default = <$ty as Default>::default();
                w.begin_struct();
                $(
                    w.field(stringify!($field));
                    exact_game::Data::write(&self.$field, w);
                )*
                $(
                    if self.$bare != default.$bare {
                        w.field(stringify!($bare));
                        exact_game::Data::write(&self.$bare, w);
                    }
                )*
                $(
                    if self.$opt != default.$opt {
                        w.key(stringify!($opt));
                        exact_game::Data::write(&self.$opt, w);
                    }
                )*
                w.end_struct();
            }
            fn read(&mut self, r: &mut dyn exact_game::Reader) -> Result<(), exact_game::DataError> {
                let fields = [$(stringify!($field)),*];
                let mut seen = [false; [$(stringify!($field)),*].len()];
                r.begin_struct()?;
                while let Some(field) = r.field()? {
                    if let Some(i) = fields.iter().position(|f| *f == field) {
                        seen[i] = true;
                    }
                    match field.as_str() {
                        $(stringify!($field) => exact_game::Data::read(&mut self.$field, r)
                            .map_err(|e| e.at(stringify!($field)))?,)*
                        $(stringify!($bare) => exact_game::Data::read(&mut self.$bare, r)
                            .map_err(|e| e.at(stringify!($bare)))?,)*
                        $(stringify!($opt) => exact_game::Data::read(&mut self.$opt, r)
                            .map_err(|e| e.at(stringify!($opt)))?,)*
                        _ => r.skip().map_err(|e| e.at(&field))?,
                    }
                }
                if let Some(i) = seen.iter().position(|s| !s).filter(|_| r.complete()) {
                    return Err(exact_game::DataError::new("missing field").at(fields[i]));
                }
                Ok(())
            }
        }
    };
}
record!(Body {
    kind,
    velocity,
    spin,
    mass,
    gravity,
    damping,
    spin_damping,
    asleep,
} unnamed { ccd } named { force, inertia });
pub(crate) use record;
impl Component for Body {
    const NAME: &'static str = "Body";
}

/// Geometry in local metres. Curved shapes require uniform positive scale.
#[derive(Clone, Debug, PartialEq, Data)]
pub enum Shape {
    /// A sphere centered on the entity.
    Sphere {
        /// Positive radius in metres.
        radius: f32,
    },
    /// A Y-axis capsule centered on the entity, including its hemispheres.
    Capsule {
        /// Positive hemisphere radius.
        radius: f32,
        /// Total height, at least twice the radius.
        height: f32,
    },
    /// An oriented box centered on the entity.
    Box {
        /// Positive half-extents along local X, Y, Z.
        half: Vec3,
    },
    /// A Y-axis cylinder; curved geometry requires uniform positive scale.
    Cylinder {
        /// Positive radius.
        radius: f32,
        /// Positive total height.
        height: f32,
    },
    /// Static terrain centered in X/Z; row-major samples (rows along Z, columns X).
    Heightfield {
        /// Number of sample rows, at least two.
        rows: u32,
        /// Number of sample columns, at least two.
        cols: u32,
        /// Finite height samples, rows × cols entries.
        heights: Vec<f32>,
        /// Full X/Z extent and Y height multiplier, positive.
        scale: Vec3,
    },
    /// Static triangle surface, with internal-edge correction.
    Mesh {
        /// Local vertices.
        vertices: Vec<Vec3>,
        /// Triangle vertex indices, counterclockwise from the front.
        indices: Vec<[u32; 3]>,
    },
    /// The convex hull of local points, its surface pushed out by `round` metres.
    /// Any body may use it (dice, rocks). Rounding requires uniform scale.
    ConvexHull {
        /// Four to 255 finite local points that do not all lie in one plane.
        points: Vec<Vec3>,
        /// Rounding radius in metres, zero or more. Mass and inertia are the
        /// rounded solid's, within 0.5% for a cube rounded by half its half-size.
        round: f32,
    },
}
impl Default for Shape {
    fn default() -> Self {
        Self::Box {
            half: Vec3::splat(0.5),
        }
    }
}

/// How a pair combines its two `bounce` values. A pair whose colliders choose
/// different rules takes the one listed later (Rapier's priority: Average, Min,
/// Multiply, Max), so Average applies only when both colliders choose it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Data)]
pub enum BounceCombine {
    /// The mean of the two values.
    Average,
    /// The smaller value.
    Min,
    /// The product of the two values.
    Multiply,
    /// The larger value.
    #[default]
    Max,
}

/// Collision material and filtering; a collider without a Body is static.
#[derive(Clone, Debug, PartialEq)]
pub struct Collider {
    /// Local geometry.
    pub shape: Shape,
    /// Shape displacement in local metres, before Transform scale.
    pub offset: Vec3,
    /// Coulomb coefficient; pair coefficients combine geometrically. Default 0.6.
    pub friction: f32,
    /// Restitution in [0, 1], combined with the other collider's by
    /// `bounce_combine`. Default zero.
    pub bounce: f32,
    /// Report touches but never apply impulses.
    pub sensor: bool,
    /// Membership bits; defaults to bit one.
    pub layer: u32,
    /// Accepted membership bits; defaults to all bits.
    pub mask: u32,
    /// How this collider's pairs combine restitution; defaults to the larger
    /// value. Saved, hashed and serialized only when not the default.
    pub bounce_combine: BounceCombine,
    /// More shapes on the same Body. They add no mass: the Body's mass and
    /// inertia are the primary shape's (or authored). A touch with one reports
    /// this entity. Queries (rays, sweeps, overlaps, capsules) see the primary
    /// shape only, for now. Needs a Body. Saved, hashed and serialized only when
    /// not empty.
    pub extra: Vec<ExtraShape>,
}
impl Default for Collider {
    fn default() -> Self {
        Self {
            shape: Shape::default(),
            offset: Vec3::ZERO,
            friction: 0.6,
            bounce: 0.0,
            sensor: false,
            layer: 1,
            mask: u32::MAX,
            bounce_combine: BounceCombine::Max,
            extra: Vec::new(),
        }
    }
}
record!(Collider {
    shape,
    offset,
    friction,
    bounce,
    sensor,
    layer,
    mask,
} unnamed {} named { bounce_combine, extra });

/// One more shape on a collider's Body (`Collider::extra`), with its own
/// material and filtering, and no mass. It may not be a mesh or a heightfield.
#[derive(Clone, Debug, PartialEq, Data)]
pub struct ExtraShape {
    /// Local geometry.
    pub shape: Shape,
    /// Shape displacement in local metres, before Transform scale.
    pub offset: Vec3,
    /// As `Collider::friction`. Default 0.6.
    pub friction: f32,
    /// As `Collider::bounce`. Default zero.
    pub bounce: f32,
    /// As `Collider::bounce_combine`.
    pub bounce_combine: BounceCombine,
    /// Membership bits; defaults to bit one.
    pub layer: u32,
    /// Accepted membership bits; defaults to all bits.
    pub mask: u32,
}
impl Default for ExtraShape {
    fn default() -> Self {
        let c = Collider::default();
        Self {
            shape: c.shape,
            offset: c.offset,
            friction: c.friction,
            bounce: c.bounce,
            bounce_combine: c.bounce_combine,
            layer: c.layer,
            mask: c.mask,
        }
    }
}
impl ExtraShape {
    // The solid collider this shape is built as.
    pub(crate) fn collider(&self) -> Collider {
        Collider {
            shape: self.shape.clone(),
            offset: self.offset,
            friction: self.friction,
            bounce: self.bounce,
            bounce_combine: self.bounce_combine,
            layer: self.layer,
            mask: self.mask,
            ..Collider::default()
        }
    }
}
impl Component for Collider {
    const NAME: &'static str = "Collider";
}

/// Opt a collider into begin/end journal messages. No per-tick samples are logged.
#[derive(Clone, Copy, Debug, Default, Component)]
pub struct Announce;

/// One begin/end transition, ordered by (a.index, b.index).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Data)]
pub struct Touch {
    /// First entity, possibly despawned for an end event.
    pub a: Entity,
    /// Second entity, possibly despawned for an end event.
    pub b: Entity,
    /// True on begin, false on end.
    pub began: bool,
}

/// World-owned Rapier state, lazily serialized by the engine's Data writer. The
/// solver settings are Rapier's integration parameters, applied at every step and
/// saved, hashed and serialized only when they differ from their defaults.
#[derive(Clone, Debug)]
pub struct Physics {
    /// World-space acceleration; defaults to (0, -9.81, 0) m/s².
    pub gravity: Vec3,
    /// This tick's sorted begin/end transitions, including sensors.
    pub events: Vec<Touch>,
    pub(crate) executor: crate::state::Executor,
    /// Solver substeps per tick, at least one; defaults to Rapier's four.
    pub solver_iterations: u32,
    /// Natural frequency in hertz of the spring that corrects a contact between
    /// two moving bodies; defaults to Rapier's 30.
    pub contact_frequency: f32,
    /// The same for a contact with a fixed body (a static collider, a Static
    /// body); defaults to Rapier's 60.
    pub static_contact_frequency: f32,
    /// Damping ratio of both contact springs, positive; defaults to Rapier's 10.
    pub contact_damping_ratio: f32,
    /// One friction constraint per contact point (Rapier's Coulomb model) instead
    /// of one per group of four plus a twist constraint (its default, faster).
    pub coulomb_friction: bool,
    /// Weight-bounded friction, as cannon-es has it; needs `coulomb_friction`. A
    /// contact point touching when a step starts may take a friction impulse of
    /// friction × `friction_gravity` × the pair's reduced mass × the substep's
    /// duration in each substep. It applies to support contacts, contacts that carried
    /// load last step, and contacts pressed during the solve. A point
    /// lifting off keeps its friction until the step ends, and a flicked box trips
    /// over its edge instead of sliding. Contacts with multibodies keep Coulomb
    /// friction. In m/s²; zero (the default) keeps Coulomb friction everywhere.
    pub friction_gravity: f32,
    /// Below this full contact-point speed in m/s, checked during each solve, a
    /// point keeps Coulomb friction under `friction_gravity`, so resting and
    /// rolling contacts settle flat. Zero applies the weight bound to every
    /// touching, loaded point.
    pub friction_slip_speed: f32,
}
crate::types::record!(Physics {
    gravity,
    events,
    executor,
} unnamed {} named {
    solver_iterations,
    contact_frequency,
    static_contact_frequency,
    contact_damping_ratio,
    coulomb_friction,
    friction_gravity,
    friction_slip_speed,
});
impl exact_game::Resource for Physics {
    const NAME: &'static str = "Physics";
    const AMBIENT: bool = true;
}
impl Default for Physics {
    fn default() -> Self {
        Self {
            gravity: Vec3::new(0.0, -9.81, 0.0),
            events: Vec::new(),
            executor: Default::default(),
            solver_iterations: 4,
            contact_frequency: 30.0,
            static_contact_frequency: 60.0,
            contact_damping_ratio: 10.0,
            coulomb_friction: false,
            friction_gravity: 0.0,
            friction_slip_speed: 0.0,
        }
    }
}
impl Physics {
    /// Refresh the saved Rapier bytes if dirty; return their length.
    /// Save/hash/JSON invoke this automatically through Data, never each tick.
    pub fn refresh_snapshot(&self) -> usize {
        self.executor.refresh()
    }
}

/// A game-controlled upright capsule. The game owns vertical velocity and gravity.
/// Other controllers block its movement unless their layer is outside its mask.
#[derive(Clone, Debug, Component)]
pub struct CapsuleController {
    /// Capsule radius, default 0.3 m.
    pub radius: f32,
    /// Total capsule height, default 1.8 m.
    pub height: f32,
    /// Maximum upward step, default 0.3 m.
    pub step: f32,
    /// Maximum walkable slope, default 50 degrees.
    pub slope_degrees: f32,
    /// Actual horizontal velocity and game-owned vertical velocity, in m/s.
    pub velocity: Vec3,
    /// Whether the last move found walkable ground.
    pub grounded: bool,
    /// Mass budget for pushing dynamic bodies, default 80 kg.
    pub mass: f32,
    /// Last supporting entity, if any.
    pub support: Option<Entity>,
    /// Saved support pose for moving-platform transport.
    pub support_pose: Transform,
}
impl Default for CapsuleController {
    fn default() -> Self {
        Self {
            radius: 0.3,
            height: 1.8,
            step: 0.3,
            slope_degrees: 50.0,
            velocity: Vec3::ZERO,
            grounded: false,
            mass: 80.0,
            support: None,
            support_pose: Transform::default(),
        }
    }
}

/// The first intersection along a ray or translational sweep.
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    /// Collider hit, with ties resolved in entity order.
    pub entity: Entity,
    /// Travel in metres, measured along the normalized direction.
    pub distance: f32,
    /// World-space point on the target surface.
    pub point: Vec3,
    /// Outward target surface normal.
    pub normal: Vec3,
}

impl Collider {
    /// Match a cube, cuboid, sphere, capsule, cylinder or plane's dimensions.
    /// A plane becomes a 1 cm box slab whose top is at Y=0. Ring, torus, cone
    /// and assets require explicit collision geometry and are refused.
    pub fn of(mesh: &exact_game::Mesh) -> Self {
        use exact_game::Mesh;
        mesh.validate().unwrap_or_else(|e| panic!("{e}"));
        let shape = match mesh {
            Mesh::Box { size } => Shape::Box { half: *size * 0.5 },
            Mesh::Sphere { radius } => Shape::Sphere { radius: *radius },
            Mesh::Capsule { radius, height } => Shape::Capsule {
                radius: *radius,
                height: *height,
            },
            Mesh::Cylinder { radius, height } => Shape::Cylinder {
                radius: *radius,
                height: *height,
            },
            Mesh::Plane { width, depth } => Shape::Box {
                half: Vec3::new(width * 0.5, 0.005, depth * 0.5),
            },
            Mesh::Ring { .. } | Mesh::Torus { .. } | Mesh::Cone { .. } => {
                panic!("Collider::of: {mesh:?} needs an authored collision shape")
            }
            Mesh::Asset(name) => {
                panic!("Collider::of: asset {name} needs an authored collision shape")
            }
        };
        Self {
            shape,
            offset: if matches!(mesh, Mesh::Plane { .. }) {
                Vec3::new(0.0, -0.005, 0.0)
            } else {
                Vec3::ZERO
            },
            ..Self::default()
        }
    }
}

impl Shape {
    /// Build a visual mesh and static collision terrain from row-major heights.
    /// Rows run along Z, columns along X; X/Z are centered, Y is scaled height.
    /// The diagonal joins the next row to the next column, matching Rapier.
    pub fn heightfield(
        rows: u32,
        cols: u32,
        heights: Vec<f32>,
        scale: Vec3,
    ) -> Result<(exact_game::asset::MeshData, Self), String> {
        crate::math::heightfield(rows, cols, heights, scale)
    }
}
