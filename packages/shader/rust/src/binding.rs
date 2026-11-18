use exact_gpu::{color, SurfaceError, Value};
use exact_shader_uniforms::{Builtins, Layout, Value as UniformValue};

#[derive(Clone, Default, PartialEq)]
pub(crate) struct Binding {
    pub src: String,
    pub max_fps: u32,
    pub play: Option<bool>,
    pub values: Vec<(String, Value)>,
}

impl Binding {
    pub fn read(inputs: &[Value]) -> Result<Self, SurfaceError> {
        let src = match inputs.first() {
            None => "",
            Some(v) => v
                .as_str()
                .ok_or_else(|| SurfaceError("src: expected an asset path".into()))?,
        };
        if !src.is_empty()
            && !src
                .strip_prefix("assets/")
                .is_some_and(exact_gpu::asset_name)
        {
            return Err(SurfaceError("src: expected assets/<path>".into()));
        }
        let max_fps = match inputs.get(1) {
            None => 0,
            Some(Value::Number(n))
                if n.is_finite() && n.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(n) =>
            {
                *n as u32
            }
            _ => {
                return Err(SurfaceError(
                    "maxFps: expected a whole number from 0 to 4294967295".into(),
                ))
            }
        };
        let play = match inputs.get(2) {
            None | Some(Value::Unit) => None,
            Some(v) if v.as_str() == Some("running") => Some(true),
            Some(v) if v.as_str() == Some("paused") => Some(false),
            _ => {
                return Err(SurfaceError(
                    "animationPlayState: expected running or paused".into(),
                ))
            }
        };
        let mut values = Vec::new();
        if let Some(rest) = inputs.get(3) {
            let Value::List(pairs) = rest else {
                return Err(SurfaceError("expected named uniform arguments".into()));
            };
            for pair in pairs.iter() {
                let Value::List(pair) = pair else {
                    return Err(SurfaceError(
                        "expected [name, value] uniform argument".into(),
                    ));
                };
                let [name, value] = pair.as_ref() else {
                    return Err(SurfaceError(
                        "expected [name, value] uniform argument".into(),
                    ));
                };
                let name = name
                    .as_str()
                    .ok_or_else(|| SurfaceError("expected a uniform name".into()))?;
                values.push((name.to_string(), value.clone()));
            }
        }
        Ok(Self {
            src: src.to_string(),
            max_fps,
            play,
            values,
        })
    }

    pub fn pack(&self, layout: &Layout, builtins: Builtins) -> Result<Vec<u8>, SurfaceError> {
        let lists: Vec<Vec<_>> = self
            .values
            .iter()
            .map(|(_, value)| match value {
                Value::List(items) => items.iter().map(scalar).collect(),
                _ => Vec::new(),
            })
            .collect();
        let values = self.values.iter().zip(&lists).map(|((name, value), list)| {
                let value = if !layout.members.iter().any(|member| member.name == *name) {
                    UniformValue::Null
                } else if matches!(value, Value::List(_)) {
                UniformValue::List(list)
            } else if let Some(text) = value.as_str() {
                // The same parser as CSS paint, retaining its unrounded colour (LLP 1181 D8).
                let wide = match color::parse(text) {
                    Some(color::Parsed::Legacy(_, w) | color::Parsed::Wide(w)) => w,
                    _ => return Err(SurfaceError(format!("uniform argument `{name}`: `{text}` is not an sRGB colour; system colours are unavailable"))),
                };
                let [r, g, b] = wide.linear_srgb().map(|v| color::linear_to_srgb(v).clamp(0.0, 1.0));
                UniformValue::Color([r, g, b, wide.alpha])
            } else {
                scalar(value)
            };
            Ok((name.as_str(), value))
        }).collect::<Result<Vec<_>, _>>()?;
        layout
            .pack(&values, builtins)
            .map_err(|e| SurfaceError(e.to_string()))
    }
}

fn scalar(value: &Value) -> UniformValue<'_> {
    match value {
        Value::Number(n) => UniformValue::Number(*n),
        Value::Bool(b) => UniformValue::Bool(*b),
        _ => UniformValue::Null,
    }
}
