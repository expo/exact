//! `Game::PREPARE`: the model instance and skin buffers sized, once the listed
//! models are resident, for every listed instance drawing at once.
use super::{ModelNode, INSTANCE_WORDS};

impl<const ASSETS: bool> crate::renderer::RendererWithAssets<ASSETS> {
    /// Raise the buffers' reserve to what `list`'s resident models need: each
    /// instance's nodes unmerged, each record with a two-entry look run, and
    /// the larger of its unmerged and merged skins. True when a buffer is
    /// smaller than its reserve, so the next rebatch grows it.
    pub(crate) fn reserve_prepared(&mut self, list: &[(&str, u32)]) -> bool {
        let m = &mut self.models;
        let Some(skin) = &m.skinning else {
            return false;
        };
        let mut need = [0u64; 4];
        for &(name, count) in list {
            let Some(model) = m.loaded.get(name).filter(|u| u.active) else {
                continue;
            };
            let count = u64::from(count);
            need[0] += count * model.nodes.len() as u64 * 3 * INSTANCE_WORDS as u64 * 4;
            // Palette joints and skin jobs of one instance's records.
            let skinned = |nodes: &[ModelNode]| {
                nodes
                    .iter()
                    .filter_map(|n| n.3)
                    .fold((0, 0), |(joints, jobs), t| {
                        (joints + u64::from(skin.joints(t)), jobs + 1)
                    })
            };
            let (a, b) = (skinned(&model.nodes), skinned(&model.merged));
            need[1] += count * a.0.max(b.0) * 64;
            need[3] += count * a.1.max(b.1) * 16;
            if let Some(t) = model.nodes.iter().chain(&model.merged).find_map(|n| n.3) {
                need[2] += count * skin.pose_floats(t) * 4;
            }
        }
        let limits = self.device.limits();
        let limit = limits
            .max_storage_buffer_binding_size
            .min(limits.max_buffer_size);
        let need = need.map(|bytes| bytes.min(limit));
        let [palette, poses, jobs] = skin.capacity();
        let instances = m.instances.as_ref().map_or(0, |b| b.raw.size());
        let grows = need
            .iter()
            .zip([instances, palette, poses, jobs])
            .any(|(need, size)| *need > size);
        m.reserve = need;
        grows
    }
}

/// Model pipeline keys new on a frame at most: `Game::PREPARE`'s warm-up
/// prepares this many each frame, so a long list spreads its compiles.
const WARM_PER_FRAME: usize = 2;
/// What a frame's model pipeline keys share: the shadowed, fogged, filter,
/// cascade, direct and light bits, whether the depth prepass ran and drew
/// tonemapped, and whether anything cast shadows. Each pass and the warm-up
/// make their keys from it, so what is warmed is what draws.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ModelFrame {
    pub forward: usize,
    pub prepass: bool,
    pub direct: bool,
    pub casts: bool,
}
impl ModelFrame {
    /// An opaque or masked material's forward key.
    pub fn forward(
        self,
        m: &super::Material,
        mirrored: bool,
        prepassed: bool,
        opaque: bool,
    ) -> usize {
        self.forward
            + crate::pipeline::PREPASS * usize::from(prepassed)
            + 64 * usize::from(m.untextured)
            + crate::pipeline::MODEL_LAYERED * usize::from(m.layered)
            + 4 * usize::from(m.double_sided)
            + 16 * usize::from(mirrored)
            + 32 * usize::from(opaque)
    }
    /// A blended material's forward key, in the sorted translucent pass.
    pub fn blended(self, m: &super::Material, mirrored: bool) -> usize {
        self.forward
            + 64 * usize::from(m.untextured)
            + crate::pipeline::MODEL_LAYERED * usize::from(m.layered)
            + 4 * usize::from(m.double_sided)
            + 8
            + 16 * usize::from(mirrored)
    }
}
/// A shadow caster's key (sun and local maps alike).
pub(crate) fn caster(m: &super::Material, mirrored: bool, opaque: bool) -> usize {
    4 * usize::from(m.double_sided) + 16 * usize::from(mirrored) + 32 * usize::from(opaque)
}
/// A camera depth prepass key.
pub(crate) fn depth(m: &super::Material, mirrored: bool, opaque: bool, direct: bool) -> usize {
    caster(m, mirrored, opaque) + crate::pipeline::MODEL_DIRECT * usize::from(direct)
}

impl<const ASSETS: bool> crate::renderer::RendererWithAssets<ASSETS> {
    /// Prepare the pipelines `list`'s resident models would draw with on the
    /// last frame (forward, shadow caster, camera depth), up to
    /// `WARM_PER_FRAME` new keys; each key is prepared once per renderer, so a
    /// remade renderer (another quality rung or sample count) prepares again.
    /// Assumes unfaded instances; a custom material's are the game's.
    pub(crate) fn warm_prepared(&mut self, list: &[(&str, u32)]) {
        let Some(frame) = self.model_frame.get() else {
            return;
        };
        if self.pipelines.models.is_none() {
            return;
        }
        // Each (kind, key) the list draws: 0 forward, 1 caster, 2 camera depth.
        let mut keys = std::collections::BTreeSet::new();
        for &(name, _) in list {
            let Some(model) = self.models.loaded.get(name).filter(|u| u.active) else {
                continue;
            };
            for node in model.nodes.iter().chain(&model.merged) {
                if self.models.custom.contains(&node.1) {
                    continue;
                }
                let m = &self.models.materials[node.1 .0];
                let mirrored = node.2.determinant() < 0.;
                if m.alpha == exact_game::asset::AlphaMode::Blend {
                    keys.insert((0, frame.blended(m, mirrored)));
                    continue;
                }
                let opaque = m.alpha == exact_game::asset::AlphaMode::Opaque;
                let prepassed = frame.prepass && node.3.is_none();
                keys.insert((0, frame.forward(m, mirrored, prepassed, opaque)));
                if frame.casts {
                    keys.insert((1, caster(m, mirrored, opaque)));
                }
                if prepassed {
                    keys.insert((2, depth(m, mirrored, opaque, frame.direct)));
                }
            }
        }
        let fresh: Vec<(u8, usize)> = keys
            .into_iter()
            .filter(|k| !self.warmed.contains(k))
            .take(WARM_PER_FRAME)
            .collect();
        for (kind, key) in fresh {
            let _ = match kind {
                0 => self.pipelines.model_variant(&self.device, key, false),
                1 => self.pipelines.model_variant(&self.device, key, true),
                _ => self.pipelines.model_camera_depth(&self.device, key),
            };
            self.warmed.insert((kind, key));
        }
    }
}
