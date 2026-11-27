use crate::{Body, Collider};
use bincode::Options;
use exact_game::{
    data::{DataError, Reader, Writer},
    Data, Entity, Transform,
};
use rapier3d::{
    pipeline::{FillHoles, PhysicsWorld},
    prelude::*,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

// Bump when Rapier, its serde representation, or bincode options change.
// v3 writes each static collider that its entry rebuilds bit-exactly as a hole.
const SNAPSHOT: &[u8] = b"EXPHYS\0\x03";

#[derive(Clone, Debug, Default)]
pub(crate) struct Entry {
    pub entity: Entity,
    pub body_handle: Option<[u32; 2]>,
    pub collider_handle: Option<[u32; 2]>,
    pub body: Option<Body>,
    pub collider: Option<Collider>,
    pub pose: Transform,
    // The handles of `Collider::extra`, in its order; written only when present.
    pub extra_handles: Vec<[u32; 2]>,
}
crate::types::record!(Entry {
    entity,
    body_handle,
    collider_handle,
    body,
    collider,
    pose,
} named { extra_handles });
impl Entry {
    pub fn bh(&self) -> Option<RigidBodyHandle> {
        self.body_handle
            .map(|h| RigidBodyHandle::from_raw_parts(h[0], h[1]))
    }
    pub fn ch(&self) -> Option<ColliderHandle> {
        self.collider_handle
            .map(|h| ColliderHandle::from_raw_parts(h[0], h[1]))
    }
    // The primary collider's handle, then the extras'.
    pub fn colliders(&self) -> impl Iterator<Item = [u32; 2]> + '_ {
        self.collider_handle
            .iter()
            .chain(&self.extra_handles)
            .copied()
    }
}
// The world and write revisions the last sync observed; derived, never saved.
#[derive(Clone)]
pub(crate) struct Synced {
    pub world: exact_game::WorldId,
    pub presentation: u64,
    pub revisions: [u64; 4],
}
pub(crate) struct Live {
    pub rapier: PhysicsWorld,
    pub entries: BTreeMap<Entity, Entry>,
    pub reverse: BTreeMap<[u32; 2], Entity>,
    // Derived from entries: entities holding a Rapier body, entities that were
    // parented at the last sync, and each index's entry.
    pub bodies: BTreeSet<Entity>,
    pub parented: BTreeSet<Entity>,
    // Entities whose collider has extras.
    pub extended: BTreeSet<Entity>,
    pub slots: BTreeMap<u32, Entity>,
    // Collider handles removed this step, unmapped once its events are named.
    pub removed: Vec<[u32; 2]>,
    pub synced: Option<Synced>,
    // Static colliders verified equal to their entry's rebuild since last edited;
    // sync invalidates only colliders it touches.
    pub elidable: BTreeSet<[u32; 2]>,
    // Each entry's hash, kept until sync or writeback next touches the entry.
    pub digests: BTreeMap<Entity, u64>,
}
impl Live {
    fn new(rapier: PhysicsWorld, entries: BTreeMap<Entity, Entry>) -> Self {
        Self {
            reverse: entries
                .values()
                .flat_map(|e| e.colliders().map(|h| (h, e.entity)))
                .collect(),
            extended: entries
                .values()
                .filter(|e| !e.extra_handles.is_empty())
                .map(|e| e.entity)
                .collect(),
            bodies: entries
                .values()
                .filter(|e| e.body_handle.is_some())
                .map(|e| e.entity)
                .collect(),
            parented: BTreeSet::new(),
            slots: entries.keys().map(|e| (e.index(), *e)).collect(),
            removed: Vec::new(),
            synced: None,
            elidable: BTreeSet::new(),
            digests: BTreeMap::new(),
            rapier,
            entries,
        }
    }
}
impl Default for Live {
    fn default() -> Self {
        let mut rapier = PhysicsWorld::default();
        rapier
            .integration_parameters
            .normalized_allowed_linear_error = 0.0001;
        Self::new(rapier, BTreeMap::new())
    }
}
#[derive(Default, Data)]
pub(crate) struct Saved {
    bytes: Vec<u8>,
    entries: Vec<Entry>,
    #[data(skip)]
    pub live: Option<Live>,
    #[data(skip)]
    pub dirty: bool,
    // The snapshot bytes' hash, and whether `entries` lags the live entries.
    #[data(skip)]
    bytes_hash: Option<u64>,
    #[data(skip)]
    entries_stale: bool,
}
impl Saved {
    pub fn live(&mut self) -> &mut Live {
        self.live.get_or_insert_with(Live::default)
    }
    fn decode(&mut self) -> Result<(), DataError> {
        if self.bytes.is_empty() {
            if !self.entries.is_empty() {
                return Err(DataError::new("physics: saved entries without a snapshot"));
            }
            return Ok(());
        }
        let payload = self.bytes.strip_prefix(SNAPSHOT).ok_or_else(|| {
            DataError::new(format!(
                "physics: expected EXPHYS v3 (v1/v2 snapshots predate static-collider rebuilds; start a new world); saw {:02x?}",
                &self.bytes[..self.bytes.len().min(8)]
            ))
        })?;
        let mut entries = BTreeMap::new();
        let mut body_handles = BTreeSet::new();
        let mut collider_handles = BTreeSet::new();
        let mut owners = BTreeMap::new();
        for entry in &self.entries {
            if entries.insert(entry.entity, entry).is_some() {
                return Err(DataError::new("physics: duplicate saved entry"));
            }
            if let Some(h) = entry.body_handle {
                if !body_handles.insert(h) {
                    return Err(DataError::new("physics: duplicate saved body handle"));
                }
            }
            if let Some(h) = entry.collider_handle {
                if !collider_handles.insert(h) {
                    return Err(DataError::new("physics: duplicate saved collider handle"));
                }
                owners.insert(h, entry);
            }
            for &h in &entry.extra_handles {
                if !collider_handles.insert(h) {
                    return Err(DataError::new("physics: duplicate saved collider handle"));
                }
            }
        }
        // A hole over a missing or invalid entry fails the read by name; nothing
        // here may panic, since release builds abort.
        let mut refused = None;
        // A filled hole is its entry's rebuild by construction: elidable without
        // re-verifying, so a restore does not re-serialize every static collider.
        let mut filled = BTreeSet::new();
        let fill = |h: ColliderHandle| {
            let rebuilt = owners
                .get(&raw(h))
                .ok_or("physics: a collider hole has no entry")
                .and_then(|e| rebuild(e));
            filled.insert(raw(h));
            rebuilt.map_err(|e| refused = Some(e)).ok()
        };
        let rapier = bincode::DefaultOptions::new()
            .with_limit(payload.len() as u64)
            .deserialize_seed(FillHoles(fill), payload)
            .map_err(|e| match refused {
                Some(why) => DataError::new(format!("physics: invalid snapshot: {why}")),
                None => DataError::new(format!("physics: invalid snapshot: {e}")),
            })?;
        for entry in &self.entries {
            if entry.body_handle.is_some() != entry.body.is_some() {
                return Err(DataError::new(
                    "physics: mismatched body handle and component",
                ));
            }
            if entry.collider_handle.is_some() != entry.collider.is_some() {
                return Err(DataError::new(
                    "physics: mismatched collider handle and component",
                ));
            }
            let expected_extras = match (&entry.body, &entry.collider) {
                (Some(_), Some(c)) => c.extra.len(),
                (None, Some(c)) if !c.extra.is_empty() => {
                    return Err(DataError::new("physics: extra shapes need a Body"));
                }
                _ => 0,
            };
            if entry.extra_handles.len() != expected_extras {
                return Err(DataError::new("physics: mismatched extra collider handles"));
            }
        }
        for &h in &body_handles {
            if !rapier
                .bodies
                .contains(RigidBodyHandle::from_raw_parts(h[0], h[1]))
            {
                return Err(DataError::new("physics: dangling saved body handle"));
            }
        }
        for &h in &collider_handles {
            if !rapier
                .colliders
                .contains(ColliderHandle::from_raw_parts(h[0], h[1]))
            {
                return Err(DataError::new("physics: dangling saved collider handle"));
            }
        }
        let entries = entries.into_iter().map(|(k, e)| (k, e.clone())).collect();
        let mut live = Live::new(rapier, entries);
        live.elidable = filled;
        self.live = Some(live);
        Ok(())
    }
    fn refresh(&mut self) -> usize {
        if self.dirty {
            if let Some(live) = &mut self.live {
                verify(live);
                self.bytes.clear();
                self.bytes.extend_from_slice(SNAPSHOT);
                let elidable = &live.elidable;
                bincode::DefaultOptions::new()
                    .serialize_into(
                        &mut self.bytes,
                        &live.rapier.with_holes(|h, _| elidable.contains(&raw(h))),
                    )
                    .expect("physics: snapshot serialization");
                self.bytes_hash = None;
                self.entries_stale = true;
            }
            self.dirty = false;
        }
        self.bytes.len()
    }
    // A save (or a clone) needs the entries themselves; a hash needs only digests.
    fn entries(&mut self) {
        if std::mem::take(&mut self.entries_stale) {
            if let Some(live) = &self.live {
                self.entries = live.entries.values().cloned().collect();
            }
        }
    }
    // What a hash reads in place of the saved content: the snapshot bytes' hash and
    // each entry's hash in entity order. Both are functions of the saved content
    // alone, recomputed identically after a load; only touched entries are rehashed.
    fn digest(&mut self) -> u64 {
        let bytes = *self
            .bytes_hash
            .get_or_insert_with(|| exact_game::hash::of(&self.bytes));
        let mut h = exact_game::hash::Hasher::default();
        bytes.write(&mut h);
        match &mut self.live {
            Some(live) => {
                (live.entries.len() as u64).write(&mut h);
                for (e, entry) in &live.entries {
                    let d = live
                        .digests
                        .entry(*e)
                        .or_insert_with(|| exact_game::hash::of(entry));
                    d.write(&mut h);
                }
            }
            None => {
                (self.entries.len() as u64).write(&mut h);
                for entry in &self.entries {
                    exact_game::hash::of(entry).write(&mut h);
                }
            }
        }
        h.finish()
    }
}
#[derive(Default)]
pub(crate) struct Executor(
    pub RefCell<Saved>,
    pub RefCell<Option<crate::queries::Cached>>,
);
impl Executor {
    pub fn refresh(&self) -> usize {
        self.0.borrow_mut().refresh()
    }
}
impl Clone for Executor {
    fn clone(&self) -> Self {
        let mut s = self.0.borrow_mut();
        s.refresh();
        s.entries();
        let mut saved = Saved {
            bytes: s.bytes.clone(),
            entries: s.entries.clone(),
            ..Saved::default()
        };
        saved.decode().expect("physics: own snapshot must decode");
        Self(RefCell::new(saved), RefCell::new(None))
    }
}
impl std::fmt::Debug for Executor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Executor")
            .field("snapshot_bytes", &self.0.borrow().bytes.len())
            .finish_non_exhaustive()
    }
}
impl Data for Executor {
    fn write(&self, w: &mut dyn Writer) {
        let mut s = self.0.borrow_mut();
        s.refresh();
        if w.digests() {
            s.digest().write(w);
        } else {
            s.entries();
            s.write(w);
        }
    }
    fn read(&mut self, r: &mut dyn Reader) -> Result<(), DataError> {
        let mut next = Saved::default();
        next.read(r)?;
        next.decode()?;
        *self.0.get_mut() = next;
        *self.1.get_mut() = None;
        Ok(())
    }
}
// A static collider exactly as sync builds it, settled as after a step. Only a
// collider without a Body is rebuilt; its entry is the component it was built from.
fn rebuild(e: &Entry) -> Result<rapier3d::prelude::Collider, &'static str> {
    let c = e
        .collider
        .as_ref()
        .filter(|c| e.body.is_none() && c.extra.is_empty())
        .ok_or("physics: a collider hole's entry is not a static collider")?;
    Ok(crate::step::try_collider(c, e.pose, None)?
        .position(crate::math::try_pose(e.pose)?)
        .build()
        .settled())
}
// Mark each static collider whose rebuild is byte-identical to the live one, so
// the snapshot carries its entry (its components) but not Rapier's copy.
fn verify(live: &mut Live) {
    let bytes = |c: &rapier3d::prelude::Collider| bincode::DefaultOptions::new().serialize(c);
    for (h, e) in &live.reverse {
        if live.elidable.contains(h) {
            continue;
        }
        let entry = &live.entries[e];
        let co = &live.rapier.colliders[ColliderHandle::from_raw_parts(h[0], h[1])];
        let same = |fresh: rapier3d::prelude::Collider| {
            bytes(co).is_ok_and(|live| bytes(&fresh).is_ok_and(|fresh| fresh == live))
        };
        if rebuild(entry).is_ok_and(same) {
            live.elidable.insert(*h);
        }
    }
}
pub(crate) fn raw(handle: ColliderHandle) -> [u32; 2] {
    let (i, g) = handle.into_raw_parts();
    [i, g]
}

#[cfg(test)]
mod tests {
    use super::*;
    use exact_game::bin;

    #[test]
    fn restored_static_colliders_stay_elidable_after_a_moving_step() {
        use crate::{CapsuleController, Physics, Shape};
        use exact_game::{Vec3, World};

        let mut w = World::new(60, 0);
        crate::register(&mut w);
        w.spawn((
            Transform::at(0.0, -0.5, 0.0),
            Collider {
                shape: Shape::Box {
                    half: Vec3::new(20.0, 0.5, 20.0),
                },
                ..Collider::default()
            },
        ));
        for i in 0..8 {
            w.spawn((
                Transform::at(i as f32 * 2.0, 0.0, 5.0),
                Collider {
                    shape: Shape::Cylinder {
                        radius: 0.35,
                        height: 6.0,
                    },
                    offset: Vec3::Y * 3.0,
                    ..Collider::default()
                },
            ));
        }
        w.spawn_named(
            "player",
            (Transform::at(0.0, 1.0, 0.0), CapsuleController::default()),
        );
        let advance = |w: &mut World| {
            crate::capsule(w, "player").step(Vec3::X);
            crate::step(w);
        };
        advance(&mut w);
        let mut restored = World::new(60, 0);
        crate::register(&mut restored);
        restored.load(&w.save()).unwrap();
        let elidable = |w: &World| {
            w.resource::<Physics>()
                .executor
                .0
                .borrow_mut()
                .live()
                .elidable
                .clone()
        };
        let rebuilt = elidable(&restored);
        assert_eq!(rebuilt.len(), 9);
        for _ in 0..3 {
            advance(&mut w);
            advance(&mut restored);
            assert_eq!(elidable(&restored), rebuilt);
            assert_eq!(restored.hash(), w.hash());
            assert_eq!(restored.save(), w.save());
        }
    }

    #[test]
    fn static_collider_edits_invalidate_their_verified_rebuilds() {
        use crate::Physics;
        use exact_game::{Vec3, World};

        let mut w = World::new(60, 0);
        crate::register(&mut w);
        let entities: Vec<_> = (0..7)
            .map(|i| w.spawn((Transform::at(i as f32 * 3.0, 0.0, 0.0), Collider::default())))
            .collect();
        crate::step(&mut w);
        w.load(&w.save()).unwrap();
        let handle = {
            let p = w.resource::<Physics>();
            let mut saved = p.executor.0.borrow_mut();
            assert_eq!(saved.live().elidable.len(), entities.len());
            saved.live().entries[&entities[0]].collider_handle.unwrap()
        };
        // A write without a value change still visits the row during sync.
        w.get_mut::<Transform>(entities[0]).unwrap().position += Vec3::ZERO;
        w.get_mut::<Transform>(entities[1]).unwrap().position.y += 1.0;
        w.get_mut::<Transform>(entities[2]).unwrap().scale *= 2.0;
        w.get_mut::<Collider>(entities[3]).unwrap().friction = 0.75;
        w.insert(entities[4], Body::default());
        w.remove::<Collider>(entities[5]);
        w.despawn(entities[6]);
        crate::step(&mut w);
        {
            let p = w.resource::<Physics>();
            let mut saved = p.executor.0.borrow_mut();
            assert_eq!(saved.live().elidable, BTreeSet::from([handle]));
        }
        for remove_body in [false, true] {
            if remove_body {
                w.remove::<Body>(entities[4]);
                crate::step(&mut w);
            }
            let p = w.resource::<Physics>();
            let mut saved = p.executor.0.borrow_mut();
            saved.refresh();
            let cached = saved.bytes.clone();
            saved.live().elidable.clear();
            saved.dirty = true;
            saved.refresh();
            assert_eq!(saved.bytes, cached);
        }
    }

    #[test]
    fn snapshot_refusal_names_expected_format_and_seen_bytes() {
        let mut saved = Saved {
            bytes: b"random!!".to_vec(),
            ..Saved::default()
        };
        let error = saved.decode().unwrap_err().to_string();
        assert!(
            error.contains("EXPHYS v3") && error.contains("start a new world"),
            "{error}"
        );
        assert!(error.contains(&format!("{:02x?}", saved.bytes)), "{error}");
    }

    // A save is input: a hole over an entry that cannot be rebuilt is refused by
    // name during the read (release builds abort on panic, so nothing may panic).
    #[test]
    fn holes_over_invalid_entries_are_refused_by_name() {
        use crate::{Collider, Shape};
        use exact_game::{Quat, Vec3, World};
        let mut w = World::new(60, 0);
        crate::register(&mut w);
        w.spawn((Transform::default(), Collider::default()));
        crate::step(&mut w);
        let physics = w.resource::<crate::Physics>();
        let mut good = physics.executor.0.borrow_mut();
        good.refresh();
        good.entries();
        let (bytes, entries) = (good.bytes.clone(), good.entries.clone());
        let shape = |shape| Collider {
            shape,
            ..Collider::default()
        };
        let cases: Vec<(Option<Collider>, Transform, &str)> = vec![
            (
                Some(Collider {
                    bounce: 2.0,
                    ..Collider::default()
                }),
                Transform::default(),
                "invalid material",
            ),
            (
                Some(Collider::default()),
                Transform {
                    rotation: Quat::from_xyzw(0.0, 0.0, 0.0, 2.0),
                    ..Transform::default()
                },
                "invalid pose",
            ),
            (
                Some(shape(Shape::Capsule {
                    radius: 1.0,
                    height: 1.0,
                })),
                Transform::default(),
                "twice its radius",
            ),
            (
                Some(shape(Shape::Heightfield {
                    rows: 3,
                    cols: 3,
                    heights: vec![0.0; 4],
                    scale: Vec3::ONE,
                })),
                Transform::default(),
                "invalid heightfield",
            ),
            (
                Some(shape(Shape::Mesh {
                    vertices: vec![Vec3::ZERO; 3],
                    indices: vec![[0, 1, 7]],
                })),
                Transform::default(),
                "invalid mesh",
            ),
            (
                Some(shape(Shape::ConvexHull {
                    points: vec![Vec3::X, Vec3::Y, Vec3::Z, Vec3::new(0.5, 0.5, 0.0)],
                    round: 0.0,
                })),
                Transform::default(),
                "lie in one plane",
            ),
            (
                Some(shape(Shape::ConvexHull {
                    points: vec![Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z],
                    round: 0.1,
                })),
                Transform {
                    scale: Vec3::new(1.0, 2.0, 1.0),
                    ..Transform::default()
                },
                "uniform scale",
            ),
            (
                Some(shape(Shape::Box {
                    half: Vec3::splat(1e30),
                })),
                Transform {
                    scale: Vec3::splat(1e30),
                    ..Transform::default()
                },
                "overflows",
            ),
            (None, Transform::default(), "not a static collider"),
        ];
        for (collider, pose, why) in cases {
            let mut entries = entries.clone();
            entries[0].collider = collider;
            entries[0].pose = pose;
            let mut saved = Saved {
                bytes: bytes.clone(),
                entries,
                ..Saved::default()
            };
            let error = saved.decode().unwrap_err().to_string();
            assert!(error.contains(why), "{why}: {error}");
        }
        let mut saved = Saved {
            bytes,
            entries,
            ..Saved::default()
        };
        saved.decode().unwrap();
    }

    // Hull input is game data or a save: degenerate, duplicated, nearly flat or
    // out-of-range points are built or refused by name, never panic in Parry.
    #[test]
    fn convex_hulls_are_built_or_refused_by_name() {
        use crate::Shape;
        use exact_game::Vec3;
        let refused = |points: Vec<Vec3>, round: f32, scale: Vec3| {
            crate::math::try_shape(&Shape::ConvexHull { points, round }, scale).err()
        };
        let one = Vec3::ONE;
        let tetra = vec![Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z];
        // Thick enough for the plane test, but Parry builds it without volume.
        let sliver = vec![
            Vec3::new(0.00093715615, 0.00030066163, 5.729107e-8),
            Vec3::new(0.0007612446, 0.0008923049, 7.49151e-8),
            Vec3::new(0.0003021846, 0.00058248325, 3.7442636e-9),
            Vec3::new(0.00066891545, 0.0004720252, 1.1845339e-7),
        ];
        assert_eq!(refused(tetra.clone(), 0.0, Vec3::new(1.0, 2.0, 3.0)), None);
        assert_eq!(refused(tetra.clone(), 0.1, one), None);
        for (points, round, why) in [
            (tetra[..3].to_vec(), 0.0, "invalid convex hull"),
            (vec![Vec3::X; 256], 0.0, "invalid convex hull"),
            (
                vec![Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::NAN],
                0.0,
                "invalid",
            ),
            (tetra.clone(), -0.1, "invalid convex hull"),
            (tetra.clone(), f32::INFINITY, "invalid convex hull"),
            (vec![Vec3::X; 8], 0.0, "one plane"),
            (
                (0..9).map(|i| Vec3::X * i as f32).collect(),
                0.0,
                "one plane",
            ),
            (sliver.clone(), 0.0, "one plane"),
            (sliver, 0.01, "one plane"),
        ] {
            let error = refused(points, round, one).expect(why);
            assert!(error.contains(why), "{why}: {error}");
        }
        // A fixed LCG: nearly flat slabs, duplicated corners, integer grids
        // (many coplanar and collinear subsets) and extreme magnitudes.
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = move || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((seed >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
        };
        for case in 0..2000 {
            let n = 4 + case % 60;
            let flat = [0.0, 1e-7, 1e-5, 2e-4, 1e-3, 1.0][case % 6];
            let size = [1e-4, 1.0, 1e4][case / 5 % 3];
            let points: Vec<Vec3> = (0..n)
                .map(|i| {
                    let p = Vec3::new(next(), next(), next() * flat);
                    let p = if case % 7 == 0 { (p * 3.0).round() } else { p };
                    if i % 3 == 0 && case % 2 == 0 {
                        Vec3::new(1.0, 1.0, 0.0) * size
                    } else {
                        p * size
                    }
                })
                .collect();
            match crate::math::try_shape(&Shape::ConvexHull { points, round: 0.0 }, one) {
                Ok(hull) => {
                    let mass = hull.mass_properties(1.0).mass();
                    assert!(mass.is_finite() && mass > 0.0, "case {case}: mass {mass}");
                }
                Err(error) => assert!(error.starts_with("physics: "), "{error}"),
            }
        }
    }

    // A rounded cube's mass and inertia are those of the cube swept by the ball,
    // with a density or an authored mass, at creation and after the collider
    // changes, offset or not.
    #[test]
    fn a_rounded_hull_takes_the_mass_and_inertia_of_its_rounded_solid() {
        use crate::{Physics, Shape};
        use exact_game::{Vec3, World};
        use std::f64::consts::PI;
        // Volume and inertia about an axis through the centre at unit density of
        // the cube [-a, a]³ swept by a ball of radius r: the cube, six face slabs,
        // twelve quarter cylinders along the edges and eight eighth balls.
        let solid = |a: f64, r: f64| {
            let disk = |a: f64| a * a * PI * r * r / 4.0 + 2.0 * a * r.powi(3) / 3.0;
            let disk = |a: f64| disk(a) + PI * r.powi(4) / 16.0;
            let volume = 8.0 * a.powi(3)
                + 24.0 * a * a * r
                + 6.0 * PI * a * r * r
                + 4.0 * PI * r.powi(3) / 3.0;
            let inertia = 16.0 * a.powi(5) / 3.0
                + 16.0 * a.powi(4) * r / 3.0
                + 4.0
                    * (4.0 * a * a * ((a + r).powi(3) - a.powi(3)) / 3.0
                        + 4.0 * a.powi(4) * r / 3.0)
                + 4.0 * 2.0 * a * 2.0 * disk(a)
                + 8.0 * (2.0 * a.powi(3) / 3.0 * PI * r * r / 4.0 + 2.0 * a * disk(a))
                + 8.0
                    * 2.0
                    * (a * a * PI * r.powi(3) / 6.0
                        + 2.0 * a * PI * r.powi(4) / 16.0
                        + PI * r.powi(5) / 30.0);
            (volume, inertia)
        };
        let cube = |round: f32, offset: Vec3| Collider {
            shape: Shape::ConvexHull {
                points: (0..8)
                    .map(|i| {
                        Vec3::new(
                            [-0.5, 0.5][i & 1],
                            [-0.5, 0.5][i >> 1 & 1],
                            [-0.5, 0.5][i >> 2],
                        )
                    })
                    .collect(),
                round,
            },
            offset,
            ..Collider::default()
        };
        let close = |seen: f32, want: f64, what: &str| {
            let error = (f64::from(seen) - want).abs() / want;
            assert!(error < 0.005, "{what}: {seen} against {want}");
        };
        for mass in [0.0, 2.0] {
            let mut w = World::new(60, 0);
            crate::register(&mut w);
            let e = w.spawn((
                Transform::default(),
                Body {
                    gravity: 0.0,
                    mass,
                    ..Body::default()
                },
                cube(0.1, Vec3::ZERO),
            ));
            for (round, offset) in [(0.1, Vec3::ZERO), (0.25, Vec3::new(0.0, 1.0, 0.0))] {
                *w.get_mut::<Collider>(e).unwrap() = cube(round, offset);
                crate::step(&mut w);
                let p = w.resource::<Physics>();
                let mut saved = p.executor.0.borrow_mut();
                let live = saved.live();
                let h = live.entries[&e].bh().unwrap();
                let m = live.rapier.bodies[h].mass_properties().local_mprops;
                let (volume, inertia) = solid(0.5, round.into());
                let want = if mass > 0.0 {
                    mass.into()
                } else {
                    1000.0 * volume
                };
                let what = format!("mass {mass}, round {round}");
                close(m.mass(), want, &what);
                for i in m.principal_inertia().to_array() {
                    close(i, inertia * want / volume, &what);
                }
                let com = crate::math::vec3(m.local_com);
                assert!((com - offset).length() < 1e-5, "{what}: {com}");
            }
            // A world saved with other mass properties, as an older build computed
            // them, keeps them through a restore and an edit of the material alone.
            let inertia = |w: &World| {
                let p = w.resource::<Physics>();
                let mut saved = p.executor.0.borrow_mut();
                let live = saved.live();
                let h = live.entries[&e].bh().unwrap();
                live.rapier.bodies[h]
                    .mass_properties()
                    .local_mprops
                    .principal_inertia()
            };
            {
                let p = w.resource::<Physics>();
                let mut saved = p.executor.0.borrow_mut();
                saved.dirty = true;
                let live = saved.live();
                let ch = live.entries[&e].ch().unwrap();
                live.rapier.colliders[ch].set_mass(2.0);
            }
            crate::step(&mut w);
            let stored = inertia(&w);
            let mut restored = World::new(60, 0);
            crate::register(&mut restored);
            restored.load(&w.save()).unwrap();
            restored.get_mut::<Collider>(e).unwrap().friction = 0.9;
            crate::step(&mut restored);
            assert_eq!(inertia(&restored), stored, "mass {mass}");
        }
    }

    // An authored inertia replaces the derived one along the body's axes and keeps
    // the derived mass and centre of mass, with a collider or without one.
    #[test]
    fn an_authored_inertia_replaces_the_derived_one() {
        use crate::{Physics, Shape};
        use exact_game::{Vec3, World};
        let inertia = Vec3::new(1.0, 2.0, 3.0);
        let offset = Vec3::new(0.0, 0.25, 0.0);
        let collider = Collider {
            shape: Shape::Box {
                half: Vec3::new(0.5, 0.1, 0.2),
            },
            offset,
            ..Collider::default()
        };
        for (mass, collider, com) in [
            (0.0, Some(collider.clone()), offset),
            (3.0, Some(collider), offset),
            (3.0, None, Vec3::ZERO),
        ] {
            let mut w = World::new(60, 0);
            crate::register(&mut w);
            let body = Body {
                gravity: 0.0,
                mass,
                inertia,
                ..Body::default()
            };
            let e = w.spawn((Transform::default(), body));
            if let Some(c) = collider.clone() {
                w.insert(e, c);
            }
            crate::step(&mut w);
            let p = w.resource::<Physics>();
            let mut saved = p.executor.0.borrow_mut();
            let live = saved.live();
            let rb = &live.rapier.bodies[live.entries[&e].bh().unwrap()];
            let m = rb.mass_properties().local_mprops;
            let want = match (mass, collider.is_some()) {
                (0.0, true) => 1000.0 * 1.0 * 0.2 * 0.4,
                _ => mass,
            };
            assert!((m.mass() - want).abs() < 1e-3, "{mass}: {}", m.mass());
            assert_eq!(crate::math::vec3(m.principal_inertia()), inertia);
            assert_eq!(crate::math::vec3(m.local_com), com);
            assert_eq!(
                m.principal_inertia_local_frame,
                rapier3d::math::Rotation::IDENTITY
            );
        }
    }

    // The solver settings are written only when set, reach Rapier's integration
    // parameters at the next step, and survive a save; a world loaded from a save
    // that omits them takes the defaults.
    #[test]
    fn solver_settings_are_saved_only_when_set_and_reach_rapier() {
        use crate::Physics;
        use exact_game::{json, World};
        use rapier3d::dynamics::FrictionModel;
        let mut w = World::new(60, 0);
        crate::register(&mut w);
        w.spawn((Transform::default(), Body::default(), Collider::default()));
        crate::step(&mut w);
        let plain = w.save();
        let text = json::to_string(&*w.resource::<Physics>()).unwrap();
        for name in [
            "solver_iterations",
            "contact_frequency",
            "coulomb_friction",
            "friction_",
        ] {
            assert!(!text.contains(name), "{text}");
        }
        {
            let mut p = w.resource_mut::<Physics>();
            p.solver_iterations = 8;
            p.contact_frequency = 20.0;
            p.static_contact_frequency = 40.0;
            p.contact_damping_ratio = 5.0;
            p.coulomb_friction = true;
            p.friction_gravity = 50.0;
            p.friction_slip_speed = 0.2;
        }
        crate::step(&mut w);
        let check = |w: &World| {
            let p = w.resource::<Physics>();
            let mut saved = p.executor.0.borrow_mut();
            let ip = saved.live().rapier.integration_parameters;
            assert_eq!(ip.num_solver_iterations, 8);
            assert_eq!(ip.contact_softness.natural_frequency, 20.0);
            assert_eq!(ip.static_contact_softness.natural_frequency, 40.0);
            assert_eq!(ip.static_contact_softness.damping_ratio, 5.0);
            assert_eq!(ip.friction_model, FrictionModel::Coulomb);
            assert_eq!((ip.friction_gravity, ip.friction_slip_speed), (50.0, 0.2));
        };
        check(&w);
        let text = json::to_string(&*w.resource::<Physics>()).unwrap();
        assert!(
            text.contains("\"solver_iterations\":8") && text.contains("\"coulomb_friction\":true"),
            "{text}"
        );
        let mut restored = World::new(60, 0);
        crate::register(&mut restored);
        restored.load(&w.save()).unwrap();
        assert_eq!(restored.hash(), w.hash());
        assert_eq!(restored.resource::<Physics>().solver_iterations, 8);
        crate::step(&mut w);
        crate::step(&mut restored);
        assert_eq!(restored.hash(), w.hash());
        check(&restored);
        restored.load(&plain).unwrap();
        let p = restored.resource::<Physics>();
        assert_eq!((p.solver_iterations, p.coulomb_friction), (4, false));
    }

    #[test]
    fn stale_physics_is_refused_during_read() {
        for bytes in [
            b"old rapier snapshot".to_vec(),
            b"EXPHYS\0\x01broken".to_vec(),
            b"EXPHYS\0\x02broken".to_vec(),
            b"EXPHYS\0\x03broken".to_vec(),
        ] {
            let saved = Saved {
                bytes,
                ..Saved::default()
            };
            let result = bin::from_slice::<Executor>(&bin::to_vec(&saved));
            assert!(result.is_err(), "stale physics accepted by Data::read");
            assert!(result.unwrap_err().to_string().contains("physics"));
        }
    }

    #[test]
    fn unvalidated_handles_entries_and_solver_settings_refuse_on_load() {
        use crate::{ExtraShape, Physics};
        use exact_game::{Vec3, World};

        let mut empty_world = World::new(60, 0);
        crate::register(&mut empty_world);
        crate::step(&mut empty_world);
        let empty_bytes = {
            let p = empty_world.resource::<Physics>();
            let mut s = p.executor.0.borrow_mut();
            s.refresh();
            s.bytes.clone()
        };

        let mut one_world = World::new(60, 0);
        crate::register(&mut one_world);
        let e1 = one_world.spawn((Transform::default(), Body::default(), Collider::default()));
        let e2 = one_world.spawn(Transform::default());
        crate::step(&mut one_world);
        let (one_bytes, valid_entry) = {
            let p = one_world.resource::<Physics>();
            let mut s = p.executor.0.borrow_mut();
            s.refresh();
            s.entries();
            (s.bytes.clone(), s.entries[0].clone())
        };

        let load_saved = |bytes: Vec<u8>, entries: Vec<Entry>| {
            let mut w = World::new(60, 0);
            crate::register(&mut w);
            {
                let p = w.resource::<Physics>();
                let mut s = p.executor.0.borrow_mut();
                s.bytes = bytes;
                s.entries = entries;
                s.live = None;
                s.dirty = false;
                s.entries_stale = false;
            }
            let save = w.save();
            let mut restored = World::new(60, 0);
            crate::register(&mut restored);
            restored.load(&save)
        };

        // Non-empty entries with empty snapshot bytes.
        assert!(load_saved(Vec::new(), vec![valid_entry.clone()]).is_err());
        // Dangling body handle.
        assert!(load_saved(
            empty_bytes.clone(),
            vec![Entry {
                entity: e1,
                body_handle: Some([999, 0]),
                body: Some(Body::default()),
                ..Entry::default()
            }]
        )
        .is_err());
        // Dangling collider handle.
        assert!(load_saved(
            empty_bytes.clone(),
            vec![Entry {
                entity: e1,
                collider_handle: Some([999, 0]),
                collider: Some(Collider::default()),
                ..Entry::default()
            }]
        )
        .is_err());
        // Dangling extra collider handle.
        assert!(load_saved(
            one_bytes.clone(),
            vec![Entry {
                collider: Some(Collider {
                    extra: vec![ExtraShape::default()],
                    ..Collider::default()
                }),
                extra_handles: vec![[999, 0]],
                ..valid_entry.clone()
            }]
        )
        .is_err());
        // body_handle present with body: None.
        assert!(load_saved(
            one_bytes.clone(),
            vec![Entry {
                body: None,
                ..valid_entry.clone()
            }]
        )
        .is_err());
        // collider_handle present with collider: None.
        assert!(load_saved(
            one_bytes.clone(),
            vec![Entry {
                collider: None,
                ..valid_entry.clone()
            }]
        )
        .is_err());
        // Duplicate entity entry.
        assert!(load_saved(
            one_bytes.clone(),
            vec![valid_entry.clone(), valid_entry.clone()]
        )
        .is_err());
        // Duplicate body/collider handle across distinct entities.
        assert!(load_saved(
            one_bytes,
            vec![
                valid_entry.clone(),
                Entry {
                    entity: e2,
                    ..valid_entry
                }
            ]
        )
        .is_err());

        // Invalid Physics solver settings and gravity.
        let invalid_settings: Vec<Box<dyn Fn(&mut Physics)>> = vec![
            Box::new(|p| p.solver_iterations = 0),
            Box::new(|p| p.gravity = Vec3::NAN),
            Box::new(|p| p.contact_frequency = 0.0),
            Box::new(|p| p.static_contact_frequency = -1.0),
            Box::new(|p| p.contact_damping_ratio = 0.0),
            Box::new(|p| {
                p.coulomb_friction = false;
                p.friction_gravity = 1.0;
            }),
            Box::new(|p| p.friction_slip_speed = -0.5),
        ];
        for mutate in invalid_settings {
            let mut w = World::new(60, 0);
            crate::register(&mut w);
            mutate(&mut w.resource_mut::<Physics>());
            let save = w.save();
            let mut restored = World::new(60, 0);
            crate::register(&mut restored);
            assert!(restored.load(&save).is_err());
        }
    }
}

#[cfg(test)]
#[path = "../tests/continuation/mod.rs"]
mod continuation;
