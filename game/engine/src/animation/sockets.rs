use super::*;

/// A tick-boundary and presentation attachment. Its Transform is the fallback when the target is unavailable.
/// Each attachment chooses its own joint; no bone entities or simulation transform writes.
/// animation::step resolves tick sockets; the renderer follows the displayed joint pose.
#[derive(Default, Clone, Debug, Data)]
pub struct SocketFollow {
    /// Target entity or name, resolved by lowest living slot.
    pub target: crate::FollowTarget,
    /// Imported joint name on the target model.
    pub joint: String,
    /// Joint-local pose; translation in metres, rotation quaternion, unitless scale.
    pub offset: Transform,
}
impl SocketFollow {
    pub fn new(target: impl Into<crate::FollowTarget>, joint: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            joint: joint.into(),
            ..Self::default()
        }
    }
    pub fn offset(mut self, offset: Transform) -> Self {
        self.offset = offset;
        self
    }
}
type MotionEntry = (Entity, Option<String>, Playback);
/// Owned playback snapshot; reading it holds no world/component leases.
#[derive(Default, Clone)]
pub struct Motion(pub(super) Option<Arc<Vec<MotionEntry>>>);
impl Motion {
    /// Apply this tick's model-local displacement after the authored rotation.
    pub fn apply_local(&self, w: &World, name: &str) {
        w.require_mut::<Transform>(name)
            .translate_local(self.root_motion(name));
    }

    fn playback(&self, target: impl Into<crate::FollowTarget>) -> Option<&Playback> {
        let target = target.into();
        self.0
            .as_ref()?
            .iter()
            .find(|(e, name, _)| match &target {
                crate::FollowTarget::Entity(id) => e == id,
                crate::FollowTarget::Name(n) => name.as_ref() == Some(n),
            })
            .map(|(_, _, p)| p)
    }
    pub fn root_motion(&self, target: impl Into<crate::FollowTarget>) -> Vec3 {
        self.playback(target)
            .map_or(Vec3::ZERO, Playback::root_motion)
    }
    pub fn crossed(&self, target: impl Into<crate::FollowTarget>, marker: &str) -> bool {
        self.playback(target).is_some_and(|p| p.crossed(marker))
    }
}
pub(super) struct ModelSockets {
    model: std::sync::Weak<Model>,
    joints: BTreeMap<String, Result<u32, String>>,
}
pub(super) type SocketCache = BTreeMap<String, ModelSockets>;
/// Resolve a joint per loaded rig, including cached named refusals.
pub fn socket_node(w: &World, target: impl crate::Target, joint: &str) -> Result<u32, String> {
    let e = target.entity(w).ok_or("socket target does not exist")?;
    let mesh = w.get::<Mesh>(e).ok_or("socket target has no Mesh")?;
    let Mesh::Asset(name) = &*mesh else {
        return Err("socket target needs Mesh::asset".into());
    };
    let model = w
        .assets
        .models
        .get(name)
        .map(|asset| &asset.model)
        .ok_or_else(|| format!("socket model `{name}` not loaded"))?;
    let mut runtime = runtime(w);
    let cache = &mut runtime.sockets;
    if !cache.get(name).is_some_and(|cached| {
        cached
            .model
            .upgrade()
            .is_some_and(|old| std::sync::Arc::ptr_eq(&old, model))
    }) {
        cache.insert(
            name.clone(),
            ModelSockets {
                model: std::sync::Arc::downgrade(model),
                joints: BTreeMap::new(),
            },
        );
    }
    let joints = &mut cache.get_mut(name).unwrap().joints;
    if let Some(node) = joints.get(joint) {
        return node.clone();
    }
    let node = named_node(model, joint).ok_or_else(|| format!("unknown socket `{joint}`"));
    joints.insert(joint.into(), node.clone());
    node
}
/// Current tick-boundary socket in world space. Call `step`, apply its Motion to
/// the owner Transform, then query sockets. Presentation samples locals at frame alpha.
/// Use [`socket_matrix`] when the rig can produce shear.
pub fn socket(w: &World, target: impl crate::Target, joint: &str) -> Result<Transform, String> {
    let (scale, rotation, position) =
        socket_matrix(w, target, joint)?.to_scale_rotation_translation();
    Ok(Transform {
        scale,
        rotation,
        position,
    })
}
const MAX_ATTACHMENT_DEPTH: usize = 64;

enum SocketError {
    Missing(String),
    Cycle(String),
}
impl SocketError {
    fn into_message(self) -> String {
        match self {
            Self::Missing(m) | Self::Cycle(m) => m,
        }
    }
}

/// Full affine tick-boundary socket, preserving shear from non-uniform ancestors.
pub fn socket_matrix(w: &World, target: impl crate::Target, joint: &str) -> Result<Mat4, String> {
    let e = target
        .entity(w)
        .ok_or_else(|| format!("socket target does not exist: `{}`", target.label()))?;
    let label = || {
        w.name(e)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("#{}", e.index()))
    };
    if socket_stale(w, e) {
        return Err(format!("socket target `{}` has a stale pose; call animation::step(w), apply Motion, then query socket", label()));
    }
    let mut visited = Vec::new();
    compose_socket(w, e, joint, &mut visited).map_err(SocketError::into_message)
}
/// Whether an animated socket needs this tick's animation step. Setup's bind pose is valid.
pub fn socket_stale(w: &World, e: Entity) -> bool {
    if !(w.has::<Animation>(e) || w.has::<Blend>(e) || w.has::<Animator>(e) || w.has::<Layers>(e)) {
        return false;
    }
    let stepped = w.get::<Pose>(e).and_then(|p| p.stepped);
    !(stepped == Some(w.tick())
        || (!w.in_tick
            && ((w.tick() == 0 && stepped.is_none())
                || w.tick()
                    .checked_sub(1)
                    .is_some_and(|tick| stepped == Some(tick)))))
}
fn compose_socket(
    w: &World,
    e: Entity,
    joint: &str,
    visited: &mut Vec<Entity>,
) -> Result<Mat4, SocketError> {
    let label = w.name(e).unwrap_or("unnamed").to_owned();
    let node = socket_node(w, e, joint).map_err(SocketError::Missing)?;
    // Drop row leases before stepping to ancestors so deep chains hold O(1) leases.
    let joint_mat = {
        let pose = w.get::<Pose>(e);
        let mesh = w.get::<Mesh>(e).unwrap();
        let Mesh::Asset(name) = &*mesh else {
            unreachable!()
        };
        let model = w.model(name).ok_or_else(|| {
            SocketError::Missing(format!(
                "socket target `{label}` needs declared model `{name}`"
            ))
        })?;
        let rest;
        let local = if let Some(pose) = &pose {
            if pose.local.len() != model.nodes.len() * 10 {
                return Err(SocketError::Missing(format!(
                    "saved pose on `{label}` does not match model `{name}`"
                )));
            }
            &pose.local
        } else {
            rest = bind_pose(model);
            &rest
        };
        joint_matrix(model, local, node)
    };
    let global = match w.current_global_visited(e, visited) {
        Ok(Some(global)) => global,
        Ok(None) => {
            return Err(SocketError::Missing(format!(
                "socket target `{label}` has no Transform"
            )));
        }
        Err(()) => {
            return Err(SocketError::Cycle(format!(
                "socket target `{label}` has a follower cycle"
            )));
        }
    };
    Ok(Mat4::from(global) * joint_mat)
}

impl Component for SocketFollow {
    const NAME: &'static str = "SocketFollow";
    fn register(w: &mut World) {
        w.register::<Pose>();
        w.attachments = Some(crate::world::Attachments {
            pose: follower_pose,
            validate: validate_attachments,
        });
    }
}
fn follower_pose(w: &World, e: Entity, visited: &mut Vec<Entity>) -> crate::world::AttachmentPose {
    let Some((target, joint, offset)) = (|| {
        let follow = w.get::<SocketFollow>(e)?;
        let target = match &follow.target {
            crate::FollowTarget::Entity(e) => *e,
            crate::FollowTarget::Name(n) => w.named(n)?,
        };
        Some((target, follow.joint.clone(), follow.offset))
    })() else {
        return crate::world::AttachmentPose::None;
    };
    if !w.contains(target) {
        return crate::world::AttachmentPose::None;
    }
    if visited.len() >= MAX_ATTACHMENT_DEPTH || visited.contains(&e) {
        return crate::world::AttachmentPose::Cycle;
    }
    visited.push(e);
    let result = compose_socket(w, target, &joint, visited);
    visited.pop();
    match result {
        Ok(socket) => crate::world::AttachmentPose::Pose(
            crate::Affine3A::from_mat4(socket)
                * crate::Affine3A::from_scale_rotation_translation(
                    offset.scale,
                    offset.rotation,
                    offset.position,
                ),
        ),
        Err(SocketError::Missing(_)) => crate::world::AttachmentPose::None,
        Err(SocketError::Cycle(_)) => crate::world::AttachmentPose::Cycle,
    }
}
fn validate_attachments(w: &World, r: &mut dyn crate::Reader) -> Result<(), crate::DataError> {
    let Some(followers) = w.storage::<SocketFollow>() else {
        return Ok(());
    };
    if followers.is_empty() {
        return Ok(());
    }
    let slots = w.alive_mask.len() * 64;
    let mut marks = Vec::new();
    crate::data::limits::reserve(r, &mut marks, slots)?;
    marks.resize(slots, 0u8);
    let mut starts = Vec::new();
    crate::data::limits::reserve(r, &mut starts, followers.len())?;
    starts.extend(w.query::<&SocketFollow>().iter().map(|(e, _)| e));
    let mut stack: Vec<(Entity, u8)> = Vec::new();
    let socket_target = |u: Entity| -> Option<Entity> {
        let follow = w.get::<SocketFollow>(u)?;
        let target = match &follow.target {
            crate::FollowTarget::Entity(e) => *e,
            crate::FollowTarget::Name(n) => w.named(n)?,
        };
        w.contains(target).then_some(target)
    };
    let parent_target = |u: Entity| -> Option<Entity> {
        w.get::<crate::Parent>(u)
            .map(|p| p.0)
            .filter(|p| w.contains(*p))
    };
    for start in starts {
        let idx = start.index() as usize;
        if marks[idx] != 0 {
            continue;
        }
        marks[idx] = 1;
        crate::data::limits::reserve(r, &mut stack, 1)?;
        stack.push((start, 0));
        while let Some(&mut (u, ref mut edge)) = stack.last_mut() {
            let next = match *edge {
                0 => {
                    *edge = 1;
                    socket_target(u)
                }
                1 => {
                    *edge = 2;
                    parent_target(u)
                }
                _ => {
                    marks[u.index() as usize] = 2;
                    stack.pop();
                    continue;
                }
            };
            if let Some(v) = next {
                let vi = v.index() as usize;
                match marks[vi] {
                    1 => {
                        return Err(crate::DataError::new(format!(
                            "SocketFollow cycle at #{}",
                            v.index()
                        )));
                    }
                    0 => {
                        marks[vi] = 1;
                        crate::data::limits::reserve(r, &mut stack, 1)?;
                        stack.push((v, 0));
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod apply_tests {
    use super::*;
    #[test]
    fn apply_local_uses_authored_rotation_and_keeps_markers() {
        let mut w = World::new(60, 0);
        let e = w.spawn_named("fox", Transform::default());
        let motion = Motion(Some(Arc::new(vec![(
            e,
            Some("fox".into()),
            Playback {
                root_motion: Vec3::X,
                crossed: vec!["step".into()],
                ..Default::default()
            },
        )])));
        w.require_mut::<Transform>("fox").rotation = crate::Quat::from_rotation_y(1.);
        let mut expected = *w.require::<Transform>("fox");
        expected.translate_local(motion.root_motion("fox"));
        motion.apply_local(&w, "fox");
        assert_eq!(w.require::<Transform>("fox").position, expected.position);
        assert!(motion.crossed("fox", "step"));
    }

    #[test]
    fn socket_follow_cycles_terminate_without_overflow_and_refuse_on_load() {
        let mut w = World::new(60, 0);
        w.register::<Transform>()
            .register::<crate::Parent>()
            .register::<Mesh>()
            .register::<SocketFollow>();
        let model = crate::asset::Model {
            nodes: vec![crate::asset::Node {
                name: "j".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        w.assets.declared.insert("rig.model".into());
        w.assets.models.insert("rig.model".into(), model.into());
        let a = w.spawn_named("a", (Transform::at(1., 0., 0.), Mesh::asset("rig.model")));
        let b = w.spawn_named("b", (Transform::at(2., 0., 0.), Mesh::asset("rig.model")));
        w.insert(a, SocketFollow::new(b, "j"));
        w.insert(b, SocketFollow::new(a, "j"));
        // Pad to 25,000 entities so an unguarded O(world.len()) walk would overflow the stack.
        for _ in 0..25_000 {
            w.spawn(());
        }
        assert!(w.global(a).is_none());
        assert!(w.globals().get(a).is_none());
        let err = socket_matrix(&w, a, "j").unwrap_err();
        assert!(err.contains("cycle"), "{err}");
        let save = w.save();
        let mut restored = World::new(60, 0);
        restored
            .register::<Transform>()
            .register::<crate::Parent>()
            .register::<Mesh>()
            .register::<SocketFollow>();
        let load_err = restored.load(&save).unwrap_err();
        assert!(load_err.to_string().contains("cycle"), "{load_err}");

        // Mixed Parent + SocketFollow cycle.
        let mut mixed = World::new(60, 0);
        mixed
            .register::<Transform>()
            .register::<crate::Parent>()
            .register::<Mesh>()
            .register::<SocketFollow>();
        mixed.assets.declared.insert("rig.model".into());
        mixed.assets.models.insert(
            "rig.model".into(),
            crate::asset::Model {
                nodes: vec![crate::asset::Node {
                    name: "j".into(),
                    ..Default::default()
                }],
                ..Default::default()
            }
            .into(),
        );
        let c = mixed.spawn((Transform::at(1., 0., 0.), Mesh::asset("rig.model")));
        let d = mixed.spawn((Transform::at(2., 0., 0.), Mesh::asset("rig.model")));
        mixed.insert(c, crate::Parent(d));
        mixed.insert(d, SocketFollow::new(c, "j"));
        assert!(mixed.global(c).is_none());
        assert!(mixed.global(d).is_none());
        assert!(socket_matrix(&mixed, c, "j").unwrap_err().contains("cycle"));
        assert!(restored.load(&mixed.save()).is_err());
    }
}
