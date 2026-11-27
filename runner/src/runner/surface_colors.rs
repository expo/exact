//! A GPU colour argument is resolved after its canvas's place commits (LLP 1009 D2a).
use super::{DataSource, Runner};
use crate::{Place, SurfaceUpdate};
use exact_kernel::{style::roles, ViewId};
use exact_plan::{ThemeId, Value};
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct Bindings(BTreeMap<ViewId, Binding>);

struct Binding {
    inputs: SurfaceUpdate,
    place: Option<(u32, bool, u64, u64)>,
    delivered: Vec<Value>,
    refused: Option<ThemeId>,
}

impl Bindings {
    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<D: DataSource> Runner<D> {
    pub(super) fn bind_gpu_inputs(&mut self, updates: Vec<SurfaceUpdate>) {
        if self.surface_colors.is_empty() && updates.iter().all(|u| u.colors.is_empty()) {
            self.surfaces.extend(updates);
            return;
        }
        let mut bindings = std::mem::take(&mut self.surface_colors);
        let _roles = self.bind_roles();
        for inputs in updates {
            if inputs.colors.is_empty() {
                bindings.0.remove(&inputs.view);
                self.surfaces.push(inputs);
                continue;
            }
            let view = inputs.view;
            let previous = bindings.0.remove(&view).filter(|b| {
                b.inputs.name == inputs.name
                    && b.inputs.names == inputs.names
                    && b.inputs.mode == inputs.mode
            });
            let (delivered, refused) = previous
                .map(|b| (b.delivered, b.refused))
                .unwrap_or_default();
            let mut binding = Binding {
                inputs,
                place: None,
                delivered,
                refused,
            };
            if self.resolve_surface(&mut binding) {
                bindings.0.insert(view, binding);
            }
        }
        bindings
            .0
            .retain(|_, binding| self.resolve_surface(binding));
        self.surface_colors = bindings;
    }

    /// Re-evaluate colour slots when the host reports new system colours.
    pub fn refresh_surface_colors(&mut self) {
        if self.surface_colors.is_empty() {
            return;
        }
        self.bind_gpu_inputs(Vec::new());
    }

    fn resolve_surface(&mut self, binding: &mut Binding) -> bool {
        let view = binding.inputs.view;
        let Some(node) = self.kernel.node(view) else {
            return false;
        };
        let dark = node.color_scheme_dark().unwrap_or_else(|| self.page_dark());
        let at = (
            self.tuple_at(view),
            dark,
            self.design_moves,
            roles::generation(),
        );
        if binding.place == Some(at) {
            return true;
        }
        let mut values = binding.inputs.values.clone();
        for &(i, token) in &binding.inputs.colors {
            let name = self
                .plan
                .str(self.plan.theme[token.0 as usize].name)
                .to_owned();
            let Some(c) = self.forma().color(&name, Place::Node(view)) else {
                if binding.refused != Some(token) {
                    self.log(format!(
                        "canvas {view:?}: surface `{}` refused colour `{name}`",
                        binding.inputs.name
                    ));
                }
                binding.refused = Some(token);
                binding.place = None;
                binding.delivered.clear();
                return true;
            };
            values[i] = Value::record(
                c.linear_rgba(dark)
                    .map(|c| Value::Number(f64::from(c)))
                    .to_vec(),
            );
        }
        binding.place = Some(at);
        binding.refused = None;
        if values != binding.delivered {
            binding.delivered.clone_from(&values);
            let mut update = binding.inputs.clone();
            update.values = values;
            self.surfaces.push(update);
        }
        true
    }
}
