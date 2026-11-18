use exact_gpu::{color, SurfaceError, Value};
use exact_model_gltf::scene::{AlphaMode, Primitive, Scene};

use crate::arguments::{error, list, string, unit_interval};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Occlusion {
    #[default]
    None,
    Occluder,
    Revealed,
}
#[derive(Clone, Debug, PartialEq)]
pub struct NodeStyle {
    pub name: String,
    pub color: Option<[f32; 4]>,
    pub opacity: f32,
    pub visible: bool,
    pub occlusion: Occlusion,
    pub occluded_opacity: f32,
}
impl Default for NodeStyle {
    fn default() -> Self {
        Self {
            name: String::new(),
            color: None,
            opacity: 1.,
            visible: true,
            occlusion: Occlusion::None,
            occluded_opacity: 0.3,
        }
    }
}
impl NodeStyle {
    pub fn parse_list(value: &Value) -> Result<Vec<Self>, SurfaceError> {
        list(value, "nodes")?
            .iter()
            .enumerate()
            .map(|(i, value)| {
                let name = format!("nodes[{i}]");
                let fields = match value {
                    Value::Record(v) | Value::List(v) => v,
                    _ => return Err(error(&name, "expected a ModelNode record")),
                };
                if fields.len() > 6 {
                    return Err(error(&name, "too many fields"));
                }
                let mut s = Self::default();
                if let Some(v) = fields.first() {
                    s.name = string(v, &format!("{name}.name"))?.into();
                }
                if let Some(v) = fields.get(1) {
                    let field = format!("{name}.color");
                    let text = string(v, &field)?;
                    if !text.is_empty() {
                        let wide = match color::parse(text) {
                            Some(color::Parsed::Legacy(_, w) | color::Parsed::Wide(w)) => w,
                            _ => {
                                return Err(error(
                                    &field,
                                    "expected a CSS colour without platform context",
                                ))
                            }
                        };
                        let rgb = wide.linear_srgb();
                        s.color = Some([
                            rgb[0] as f32,
                            rgb[1] as f32,
                            rgb[2] as f32,
                            wide.alpha as f32,
                        ]);
                    }
                }
                if let Some(v) = fields.get(2) {
                    s.opacity = unit_interval(v, &format!("{name}.opacity"))?;
                }
                if let Some(v) = fields.get(3) {
                    s.visible = match string(v, &format!("{name}.visibility"))? {
                        "visible" => true,
                        "hidden" => false,
                        _ => {
                            return Err(error(
                                &format!("{name}.visibility"),
                                "expected visible or hidden",
                            ))
                        }
                    };
                }
                if let Some(v) = fields.get(4) {
                    s.occlusion = match string(v, &format!("{name}.occlusion"))? {
                        "none" => Occlusion::None,
                        "occluder" => Occlusion::Occluder,
                        "revealed" => Occlusion::Revealed,
                        _ => {
                            return Err(error(
                                &format!("{name}.occlusion"),
                                "expected none, occluder or revealed",
                            ))
                        }
                    };
                }
                if let Some(v) = fields.get(5) {
                    s.occluded_opacity = unit_interval(v, &format!("{name}.occludedOpacity"))?;
                }
                Ok(s)
            })
            .collect()
    }
}

pub fn resolve(scene: &Scene, entries: &[NodeStyle]) -> Vec<NodeStyle> {
    scene
        .nodes
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let mut at = Some(i);
            while let Some(i) = at {
                if let Some(entry) = entries.iter().rev().find(|e| e.name == scene.nodes[i].name) {
                    return entry.clone();
                }
                at = scene.nodes[i].parent;
            }
            NodeStyle::default()
        })
        .collect()
}

#[derive(Clone, Copy, Debug)]
pub struct Effective {
    pub color: [f32; 4],
    pub visible: bool,
    pub occlusion: Occlusion,
    pub occluded_opacity: f32,
}
impl Effective {
    pub fn new(primitive: &Primitive, style: &NodeStyle) -> Self {
        let material = primitive.material;
        let mut color = style.color.unwrap_or(material.color);
        let alpha = if material.alpha_mode == AlphaMode::Blend {
            material.color[3]
        } else {
            1.
        };
        color[3] = if style.occlusion == Occlusion::Occluder {
            1.
        } else {
            alpha * style.opacity
        };
        let visible = style.visible
            && color[3] > 0.
            && (material.alpha_mode != AlphaMode::Mask
                || material.color[3] >= material.alpha_cutoff);
        Self {
            color,
            visible,
            occlusion: style.occlusion,
            occluded_opacity: style.occluded_opacity,
        }
    }
    pub fn opaque(self) -> bool {
        self.color[3] == 1.
    }
}
