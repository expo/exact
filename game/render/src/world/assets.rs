use super::*;
use crate::{
    models::{PartLook, NUMBERED, REPLACE},
    DrawInstance, MaterialId, RENDER_SLOT_BASE,
};
// Geometry, material, mirrored, viewmodel, and the level of detail.
type GroupKey = (MeshId, MaterialId, bool, bool, u8);
#[derive(Default)]
pub(super) struct Assets {
    pub records: Vec<DrawInstance>,
    pub entities: Vec<exact_game::Entity>,
    groups: BTreeMap<GroupKey, Vec<u32>>,
    // Per record, 1 + its first merged part look (0: none), and those looks.
    part_bases: Vec<u32>,
    part_looks: Vec<PartLook>,
    // Per record, its level of detail; and the `ModelLod` entities.
    levels: Vec<u8>,
    lods: Vec<crate::lod::Lod>,
    // Each drawn entity's records and part looks, and its looks' content digest.
    ranges: BTreeMap<u32, Range>,
    digests: BTreeMap<u32, u64>,
    structure: Option<Structure>,
    // What each drawn entity drew at the last rebuild, the assets that is, and
    // the textures whose arrival changes what one draws.
    shown: BTreeMap<u32, Shown>,
    held: std::collections::BTreeSet<String>,
    waiting: std::collections::BTreeSet<String>,
}
/// Texture names by `MaterialData` slot order, `None` keeping the material's.
type Slots = [Option<String>; 5];
/// What an entity draws: its `Mesh`'s model once that can draw, until then
/// the rigid model it drew before (a skin swapped to a model still in flight);
/// and its `MaterialOverride` textures once all of them can draw, until then
/// the ones it drew before (the model's own, the first time).
#[derive(Clone, PartialEq)]
struct Shown {
    entity: exact_game::Entity,
    model: String,
    textures: Vec<(u32, Slots)>,
    // Drawing a prior model in place of the `Mesh`'s: the `ModelLod` levels,
    // which belong to the new model, are not drawn meanwhile.
    fallback: bool,
}
struct Structure {
    models: Vec<(exact_game::Entity, String)>,
    revisions: [u64; 3],
}
struct Range {
    entity: exact_game::Entity,
    first: usize,
    count: usize,
    part_first: usize,
    part_count: usize,
}
impl Assets {
    pub fn reset(&mut self) {
        self.structure = None;
        self.records.clear();
        self.entities.clear();
        // `shown` and `held` stay: a restore or a paranoid sample keeps entity
        // ids, so an entity keeps its look across it, and the next rebuild
        // drops the records of entities that no longer draw.
        self.waiting.clear();
    }
    /// The models drawn at the last rebuild, fallbacks included: what the
    /// engine keeps resident for this renderer (`Sim::hold_assets`).
    pub fn held(&self) -> &std::collections::BTreeSet<String> {
        &self.held
    }
    /// Whether a texture an entity waits for can draw now.
    pub fn arrived(&self, r: &impl Writes) -> bool {
        self.waiting.iter().any(|n| r.texture_ready(n))
    }
    /// What `entity`, whose `Mesh` names `wanted`, draws now; adds the
    /// textures it waits for to `waiting`.
    fn resolve(
        &self,
        w: &World,
        r: &impl Writes,
        entity: exact_game::Entity,
        wanted: &str,
        waiting: &mut std::collections::BTreeSet<String>,
    ) -> Shown {
        let prior = self
            .shown
            .get(&entity.index())
            .filter(|s| s.entity == entity);
        let ready = |set: &[(u32, Slots)]| {
            set.iter()
                .flat_map(|(_, s)| s.iter().flatten())
                .all(|n| r.texture_ready(n))
        };
        let mut textures = Vec::new();
        if let Some(overrides) = w.get::<exact_game::MaterialOverrides>(entity) {
            for o in &overrides.0 {
                let names = o.textures.names();
                if names.iter().any(Option::is_some) {
                    textures.push((o.material, names.map(|n| n.map(str::to_owned))));
                }
            }
        }
        let model_ready = r.model(wanted).is_some();
        if model_ready && ready(&textures) {
            return Shown {
                entity,
                model: wanted.to_owned(),
                textures,
                fallback: false,
            };
        }
        waiting.extend(
            textures
                .iter()
                .flat_map(|(_, s)| s.iter().flatten())
                .filter(|n| !r.texture_ready(n))
                .cloned(),
        );
        // The whole previous look, model and textures, until the new one can
        // draw: a prior model other than the `Mesh`'s must be rigid to stand in.
        let drawable = |p: &Shown| {
            if p.model == wanted {
                model_ready
            } else {
                r.model(&p.model).is_some_and(|d| rigid(&d))
            }
        };
        if let Some(prior) = prior.filter(|p| drawable(p) && ready(&p.textures)) {
            return Shown {
                fallback: prior.model != wanted,
                ..prior.clone()
            };
        }
        // Nothing drawn before: the model's own textures until the set arrives.
        Shown {
            entity,
            model: wanted.to_owned(),
            textures: Vec::new(),
            fallback: false,
        }
    }
    fn hold(&mut self) {
        self.held = (self.shown.values())
            .flat_map(|s| {
                std::iter::once(&s.model)
                    .chain(s.textures.iter().flat_map(|(_, t)| t.iter().flatten()))
            })
            .cloned()
            .collect();
    }

    // `drawn` is the feed's current Mesh+Transform query, with effective
    // visibility already resolved through every living ancestor.
    pub fn batches(
        &mut self,
        w: &World,
        r: &mut impl Writes,
        drawn: &[(exact_game::Entity, bool, bool)],
        batches: &mut Vec<Batch>,
        slots: &mut Vec<u32>,
    ) -> Result<(), RenderError> {
        let revisions = [
            r.assets_revision(),
            w.revision::<ViewModel>(),
            w.revision::<exact_game::ModelLod>(),
        ];
        // Compare borrowed names before building a replacement. Primitive
        // effects can change batches while thousands of model names stay put.
        // A texture an entity waited for arrived: resolve every entity again.
        let arrived = self.arrived(r);
        let unchanged = !arrived
            && self.structure.as_ref().is_some_and(|saved| {
                saved.revisions == revisions
                    && w.query::<(&Mesh, &Transform)>()
                        .iter()
                        .zip(drawn)
                        .filter_map(|((entity, (mesh, _)), &(expected, visible, _))| {
                            debug_assert_eq!(entity, expected);
                            match mesh {
                                Mesh::Asset(name) if visible => Some((entity, name.as_str())),
                                _ => None,
                            }
                        })
                        .eq(saved.models.iter().map(|(e, name)| (*e, name.as_str())))
            });
        if unchanged && self.patch_looks(w, r)? {
            self.append_batches(batches, slots);
            return Ok(());
        }
        if !arrived && self.remove_models(w, drawn, revisions) {
            r.part_looks(&self.part_bases, &self.part_looks);
            r.levels(&self.levels, &self.lods);
            if let Err(error) = r.instances(&self.records) {
                self.structure = None;
                return Err(error);
            }
            self.append_batches(batches, slots);
            return Ok(());
        }
        let structure = Structure {
            models: w
                .query::<(&Mesh, &Transform)>()
                .iter()
                .zip(drawn)
                .filter_map(|((entity, (mesh, _)), &(expected, visible, _))| {
                    debug_assert_eq!(entity, expected);
                    match mesh {
                        Mesh::Asset(name) if visible => Some((entity, name.clone())),
                        _ => None,
                    }
                })
                .collect(),
            revisions,
        };
        self.records.clear();
        self.entities.clear();
        self.part_bases.clear();
        self.part_looks.clear();
        self.levels.clear();
        self.lods.clear();
        self.ranges.clear();
        for group in self.groups.values_mut() {
            group.clear();
        }
        let (mut shown, mut waiting) = (BTreeMap::new(), Default::default());
        for (entity, name) in &structure.models {
            let next = self.resolve(w, r, *entity, name, &mut waiting);
            if self.entity(w, r, &next)? {
                shown.insert(entity.index(), next);
            }
        }
        self.shown = shown;
        self.waiting = waiting;
        self.hold();
        self.digests = look_digests(w);
        r.part_looks(&self.part_bases, &self.part_looks);
        r.levels(&self.levels, &self.lods);
        r.instances(&self.records)?;
        self.structure = Some(structure);
        self.append_batches(batches, slots);
        Ok(())
    }

    /// Visibility changes and despawns often only remove models. Their survivors
    /// retain the same draw records; compact them without resolving every node,
    /// material and LOD again. All eligibility checks precede any mutation.
    fn remove_models(
        &mut self,
        w: &World,
        drawn: &[(exact_game::Entity, bool, bool)],
        revisions: [u64; 3],
    ) -> bool {
        let Some(structure) = &mut self.structure else {
            return false;
        };
        if structure.revisions != revisions {
            return false;
        }
        let mut keep = Vec::new();
        let mut at = 0;
        for ((entity, (mesh, _)), &(expected, visible, _)) in
            w.query::<(&Mesh, &Transform)>().iter().zip(drawn)
        {
            debug_assert_eq!(entity, expected);
            let Mesh::Asset(name) = mesh else { continue };
            if !visible {
                continue;
            }
            while at < structure.models.len() && structure.models[at].0 != entity {
                at += 1;
            }
            if at == structure.models.len() || structure.models[at].1 != *name {
                return false;
            }
            keep.push(at);
            at += 1;
        }
        let digests = look_digests(w);
        if keep.iter().any(|&i| {
            let slot = structure.models[i].0.index();
            digests.get(&slot) != self.digests.get(&slot)
        }) {
            return false;
        }
        // Both lists follow entity order. Walk them together instead of looking
        // up each surviving range and entity again in the tree.
        let mut kept = keep
            .iter()
            .map(|&i| structure.models[i].0.index())
            .peekable();
        self.ranges.retain(|slot, _| {
            while kept.peek().is_some_and(|next| next < slot) {
                kept.next();
            }
            kept.peek() == Some(slot)
        });
        let mut remap = vec![u32::MAX; self.records.len()];
        let (mut records, mut parts) = (0, 0);
        self.entities.clear();
        for range in self.ranges.values_mut() {
            self.entities.push(range.entity);
            for offset in 0..range.count {
                let old = range.first + offset;
                let new = records + offset;
                remap[old] = new as u32;
                self.records[new] = self.records[old].clone();
                let base = self.part_bases[old];
                let part = base & !(REPLACE | NUMBERED);
                self.part_bases[new] = (base & (REPLACE | NUMBERED))
                    | if part == 0 {
                        0
                    } else {
                        part - range.part_first as u32 + parts as u32
                    };
            }
            self.levels
                .copy_within(range.first..range.first + range.count, records);
            self.part_looks
                .copy_within(range.part_first..range.part_first + range.part_count, parts);
            range.first = records;
            range.part_first = parts;
            records += range.count;
            parts += range.part_count;
        }
        self.records.truncate(records);
        self.part_bases.truncate(records);
        self.levels.truncate(records);
        self.part_looks.truncate(parts);
        for group in self.groups.values_mut() {
            group.retain_mut(|slot| {
                let record = remap[(*slot - RENDER_SLOT_BASE) as usize];
                if record == u32::MAX {
                    false
                } else {
                    *slot = RENDER_SLOT_BASE + record;
                    true
                }
            });
        }
        self.lods.retain_mut(|lod| {
            let record = remap[lod.record as usize];
            if record == u32::MAX {
                false
            } else {
                lod.record = record;
                true
            }
        });
        let mut index = 0;
        let mut keep = keep.into_iter().peekable();
        structure.models.retain(|_| {
            let retained = keep.peek() == Some(&index);
            if retained {
                keep.next();
            }
            index += 1;
            retained
        });
        self.digests = digests;
        let ranges = &self.ranges;
        self.shown.retain(|slot, _| ranges.contains_key(slot));
        self.hold();
        true
    }

    fn append_batches(&self, batches: &mut Vec<Batch>, slots: &mut Vec<u32>) {
        for (&(mesh, _, _, viewmodel, level), list) in &self.groups {
            if list.is_empty() {
                continue;
            }
            let start = slots.len() as u32;
            slots.extend(list);
            let mut batch = Batch::new(mesh, start..slots.len() as u32);
            batch.level = level;
            batches.push(if viewmodel { batch.viewmodel() } else { batch });
        }
    }
    /// Write changed `NodeMaterials`/`MaterialOverrides` content into the
    /// existing records in place: a pulsing glow costs its own entities, not
    /// a rebatch. False when a change needs one (a look on a part that had
    /// none, so the part-look table changes shape).
    pub fn patch_looks(&mut self, w: &World, r: &mut impl Writes) -> Result<bool, RenderError> {
        let digests = look_digests(w);
        let changed: Vec<u32> = (digests.keys().chain(self.digests.keys()))
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .filter(|slot| digests.get(slot) != self.digests.get(slot))
            .collect();
        for slot in changed {
            let Some(range) = self.ranges.get(&slot) else {
                continue;
            };
            let (entity, first, count) = (range.entity, range.first, range.count);
            let (part_first, part_count) = (range.part_first, range.part_count);
            let Some(Mesh::Asset(name)) = w.get::<Mesh>(entity).as_deref().cloned() else {
                return Ok(false);
            };
            let mut waiting = std::mem::take(&mut self.waiting);
            let next = self.resolve(w, r, entity, &name, &mut waiting);
            self.waiting = waiting;
            let mut fresh = Assets::default();
            fresh.entity(w, r, &next)?;
            let same = fresh.records.len() == count
                && fresh.part_looks.len() == part_count
                && fresh
                    .records
                    .iter()
                    .zip(&self.records[first..])
                    .all(|(a, b)| {
                        (a.geometry, a.material, a.skin, a.local)
                            == (b.geometry, b.material, b.skin, b.local)
                    })
                && fresh
                    .part_bases
                    .iter()
                    .zip(&self.part_bases[first..])
                    .all(|(&a, &b)| {
                        let flags = REPLACE | NUMBERED;
                        let flag = a & flags == b & flags;
                        let (a, b) = (a & !flags, b & !flags);
                        flag && (a == 0) == (b == 0)
                            && (a == 0 || a as usize + part_first == b as usize)
                    });
            if !same {
                return Ok(false);
            }
            for (old, new) in self.records[first..first + count]
                .iter_mut()
                .zip(&fresh.records)
            {
                old.tint = new.tint;
                old.glow = new.glow;
            }
            self.part_looks[part_first..part_first + part_count].copy_from_slice(&fresh.part_looks);
            if self.shown.get(&slot) != Some(&next) {
                self.shown.insert(slot, next);
                self.hold();
            }
            r.patch_looks(
                first,
                &self.records[first..first + count],
                part_first,
                &self.part_looks[part_first..part_first + part_count],
            );
        }
        self.digests = digests;
        Ok(true)
    }
    /// One model entity's records (every level), looks, levels and groups;
    /// whether it drew anything.
    fn entity(
        &mut self,
        w: &World,
        r: &mut impl Writes,
        shown: &Shown,
    ) -> Result<bool, RenderError> {
        let (entity, name) = (shown.entity, shown.model.as_str());
        let part_first = self.part_looks.len();
        let looks = w.get::<exact_game::NodeMaterials>(entity);
        let overrides = w.get::<exact_game::MaterialOverrides>(entity);
        let viewmodel = w.has::<exact_game::ViewModel>(entity);
        // The entity's own model, then each coarser level; the renderer
        // draws one of them per frame.
        let lod = w.get::<exact_game::ModelLod>(entity);
        if let Some(lod) = &lod {
            crate::lod::validate(lod)
                .map_err(|e| RenderError::scene(format!("entity {}: {e}", entity.index())))?;
        }
        // A prior model standing in draws without the new model's levels; its
        // `hide` distance still applies.
        let levels = match &lod {
            Some(lod) if !shown.fallback => &lod.levels[..],
            _ => &[],
        };
        let count = 1 + levels.len();
        let first = self.records.len();
        let mut present = 0u8;
        let mut drawn = false;
        for level in 0..count {
            let model = match level {
                1.. => levels[level - 1].model.as_str(),
                _ => name,
            };
            // The level's materials drawn with this instance's textures.
            let mut variants = Vec::new();
            if !shown.textures.is_empty() {
                let materials = r.model(model).map(|d| d.materials.to_vec());
                for (index, names) in &shown.textures {
                    if let Some(&base) = materials.as_ref().and_then(|m| m.get(*index as usize)) {
                        if let Some(variant) = r.material_variant(base, names) {
                            variants.push((base, variant));
                        }
                    }
                }
            }
            let Some(draws) = r.model(model) else {
                continue;
            };
            let names = draws.names;
            // A custom vertex shader sees node-local positions: never merged.
            let custom = draws.merged.iter().any(|n| draws.custom.contains(&n.1));
            // A material override recolours one material's records: none of
            // them may hold parts of others.
            let several = overrides.is_some() && draws.parts.iter().any(|p| !p.is_empty());
            let (nodes, members) = if draws.merged.is_empty() || custom || several {
                (draws.nodes, None)
            } else {
                (
                    draws.merged,
                    Some((draws.members, draws.starts, draws.parts)),
                )
            };
            // A material override replaces its records' base colour factor:
            // every part of a record shares its material.
            let material_look = |material: MaterialId| {
                let index = draws.materials.iter().position(|&m| m == material)?;
                let o = overrides
                    .as_ref()?
                    .0
                    .iter()
                    .find(|o| o.material as usize == index)?;
                Some((o.color, o.emissive, o.uv_offset))
            };
            drawn |= !nodes.is_empty();
            if !nodes.is_empty() {
                present |= 1 << level;
            }
            for (i, &(geometry, material, local, skin)) in nodes.iter().enumerate() {
                let slot = RENDER_SLOT_BASE + self.records.len() as u32;
                let parts = members.map_or_else(
                    || names.get(i).map_or(&[][..], std::slice::from_ref),
                    |m| &m.0[i][..],
                );
                let find = |node: &String| {
                    looks
                        .as_ref()
                        .and_then(|l| l.0.iter().find(|m| m.node == *node))
                };
                let mut look = None;
                let mut base = 0;
                // A draw of several materials: each part's entry carries its own.
                let materials = members.and_then(|m| m.2.get(i)).map_or(&[][..], |p| &p[..]);
                // The material's UV offset, in each of the record's entries.
                // A custom material's shader reads no look table.
                let offset = material_look(material)
                    .and_then(|l| l.2)
                    .filter(|_| !draws.custom.contains(&material));
                let [u, v] = offset.unwrap_or_default();
                if parts.len() <= 1 {
                    look = parts.first().and_then(find);
                    // One neutral entry for the whole record, holding the offset.
                    if offset.is_some() {
                        base = self.part_looks.len() as u32 + 1;
                        let mut entry = [0.; 18];
                        entry[..4].copy_from_slice(&[1.; 4]);
                        entry[16..].copy_from_slice(&[u, v]);
                        let mut end = [0.; 18];
                        end[7] = f32::from_bits(u32::MAX);
                        self.part_looks.extend([entry, end]);
                    }
                } else if !materials.is_empty()
                    || offset.is_some()
                    || parts.iter().any(|p| find(p).is_some())
                {
                    // A merged draw finds each vertex's part by its vertex range,
                    // or by its number when it has several materials.
                    let starts = &members.unwrap().1[i];
                    base = (self.part_looks.len() as u32 + 1)
                        | if materials.is_empty() { 0 } else { NUMBERED };
                    self.part_looks
                        .extend(
                            parts
                                .iter()
                                .zip(starts)
                                .enumerate()
                                .map(|(k, (p, &start))| {
                                    let (c, e) = find(p)
                                        .map_or(([1.; 4], [0.; 3]), |l| (l.color, l.emissive));
                                    let m = materials.get(k).copied().unwrap_or_default();
                                    [
                                        c[0],
                                        c[1],
                                        c[2],
                                        c[3],
                                        e[0],
                                        e[1],
                                        e[2],
                                        f32::from_bits(start),
                                        m[0],
                                        m[1],
                                        m[2],
                                        m[3],
                                        m[4],
                                        m[5],
                                        m[6],
                                        m[7],
                                        u,
                                        v,
                                    ]
                                }),
                        );
                    let mut end = [0.; 18];
                    end[7] = f32::from_bits(u32::MAX);
                    self.part_looks.push(end);
                }
                let (mut tint, mut glow) =
                    look.map_or(([1.; 4], [0.; 3]), |l| (l.color, l.emissive));
                if let Some((color, g, _)) = material_look(material) {
                    if let Some(c) = color {
                        tint = std::array::from_fn(|i| tint[i] * c[i]);
                        base |= REPLACE;
                    }
                    glow = std::array::from_fn(|i| glow[i] + g[i]);
                }
                let material = variants
                    .iter()
                    .find(|(b, _)| *b == material)
                    .map_or(material, |v| v.1);
                self.part_bases.push(base);
                self.levels.push(level as u8);
                self.records.push(DrawInstance {
                    data: 0,
                    transform: entity.index(),
                    geometry,
                    material,
                    local,
                    skin,
                    tint,
                    glow,
                });
                self.groups
                    .entry((
                        geometry,
                        material,
                        local.determinant() < 0.,
                        viewmodel,
                        level as u8,
                    ))
                    .or_default()
                    .push(slot);
            }
        }
        if let (Some(lod), true) = (&lod, drawn) {
            let mut starts = [0.; crate::lod::MAX_LEVELS];
            for (start, l) in starts[1..].iter_mut().zip(levels) {
                *start = l.distance;
            }
            self.lods.push(crate::lod::Lod {
                slot: entity.index(),
                record: first as u32,
                count: (self.records.len() - first) as u32,
                starts,
                levels: count as u8,
                hide: lod.hide.unwrap_or(f32::INFINITY),
                present,
            });
        }
        if drawn {
            self.entities.push(entity);
            self.ranges.insert(
                entity.index(),
                Range {
                    entity,
                    first,
                    count: self.records.len() - first,
                    part_first,
                    part_count: self.part_looks.len() - part_first,
                },
            );
        }
        Ok(drawn)
    }
}

/// A model without skins or animated parts, whose records draw for any
/// entity: what an entity may keep drawing in place of a model in flight.
fn rigid(draws: &crate::models::Draws<'_>) -> bool {
    draws
        .nodes
        .iter()
        .chain(draws.merged)
        .all(|n| n.3.is_none())
}

#[cfg(test)]
#[path = "assets_tests.rs"]
mod tests;

/// Each entity's `NodeMaterials` and `MaterialOverrides` content, by slot.
fn look_digests(w: &World) -> BTreeMap<u32, u64> {
    use std::hash::{Hash, Hasher};
    let mut hashers: BTreeMap<u32, std::collections::hash_map::DefaultHasher> = BTreeMap::new();
    for (e, looks) in w.query::<&exact_game::NodeMaterials>().iter() {
        let h = hashers.entry(e.index()).or_default();
        e.generation().hash(h);
        for l in &looks.0 {
            l.node.hash(h);
            l.color.map(f32::to_bits).hash(h);
            l.emissive.map(f32::to_bits).hash(h);
        }
    }
    for (e, looks) in w.query::<&exact_game::MaterialOverrides>().iter() {
        let h = hashers.entry(e.index()).or_default();
        u32::MAX.hash(h);
        for l in &looks.0 {
            l.material.hash(h);
            l.color.map(|c| c.map(f32::to_bits)).hash(h);
            l.emissive.map(f32::to_bits).hash(h);
            l.textures.names().hash(h);
            l.uv_offset.map(|o| o.map(f32::to_bits)).hash(h);
        }
    }
    hashers.into_iter().map(|(k, h)| (k, h.finish())).collect()
}

/// Device work is diagnostic only: never part of a save or simulation hash.
#[derive(Clone, Copy, Default)]
pub(crate) struct Work {
    pub texture_uploads: u64,
    pub mesh_uploads: u64,
    pub pipeline_creations: u64,
    pub buffer_reallocations: u64,
}
impl Work {
    pub fn plus(self, old: Self) -> Self {
        Self {
            texture_uploads: self.texture_uploads + old.texture_uploads,
            mesh_uploads: self.mesh_uploads + old.mesh_uploads,
            pipeline_creations: self.pipeline_creations + old.pipeline_creations,
            buffer_reallocations: self.buffer_reallocations + old.buffer_reallocations,
        }
    }
    pub fn since(self, old: Self) -> Self {
        Self {
            texture_uploads: self.texture_uploads.saturating_sub(old.texture_uploads),
            mesh_uploads: self.mesh_uploads.saturating_sub(old.mesh_uploads),
            pipeline_creations: self
                .pipeline_creations
                .saturating_sub(old.pipeline_creations),
            buffer_reallocations: self
                .buffer_reallocations
                .saturating_sub(old.buffer_reallocations),
        }
    }
    pub fn json(self) -> String {
        format!("{{\"textureUploads\":{},\"meshUploads\":{},\"pipelineCreations\":{},\"modelSkinBufferReallocations\":{}}}",
            self.texture_uploads, self.mesh_uploads, self.pipeline_creations, self.buffer_reallocations)
    }
}
