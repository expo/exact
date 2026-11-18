use exact_gpu::{SurfaceError, Value};
use exact_model_gltf::math::{dot, norm, V3};

use crate::{hotspots::Hotspot, styles::NodeStyle};

pub const NAMES: [&str; 13] = [
    "src",
    "cameraControls",
    "cameraOrbit",
    "cameraTarget",
    "fieldOfView",
    "disablePan",
    "disableZoom",
    "interpolationDecay",
    "nodes",
    "hotspots",
    "clipPosition",
    "clipNormal",
    "minHotspotOpacity",
];

pub fn defaults() -> Vec<Value> {
    vec![
        Value::str(""),
        Value::Bool(false),
        Value::str("0deg 75deg 105%"),
        Value::str("auto auto auto"),
        Value::str("auto"),
        Value::Bool(false),
        Value::Bool(false),
        Value::Number(50.),
        Value::List(vec![].into()),
        Value::List(vec![].into()),
        Value::str("auto auto auto"),
        Value::str(""),
        Value::Number(0.25),
    ]
}

pub fn error(name: &str, detail: &str) -> SurfaceError {
    SurfaceError(format!("{name}: {detail}"))
}
pub fn string<'a>(value: &'a Value, name: &str) -> Result<&'a str, SurfaceError> {
    value
        .as_str()
        .ok_or_else(|| error(name, "expected a string"))
}
pub fn number(value: &Value, name: &str) -> Result<f32, SurfaceError> {
    match value {
        Value::Number(n) if n.is_finite() && (*n as f32).is_finite() => Ok(*n as f32),
        _ => Err(error(name, "expected a finite number")),
    }
}
pub fn unit_interval(value: &Value, name: &str) -> Result<f32, SurfaceError> {
    let n = number(value, name)?;
    if (0.0..=1.0).contains(&n) {
        Ok(n)
    } else {
        Err(error(name, "expected 0 to 1"))
    }
}
fn boolean(value: &Value, name: &str) -> Result<bool, SurfaceError> {
    match value {
        Value::Bool(v) => Ok(*v),
        _ => Err(error(name, "expected a boolean")),
    }
}
pub fn list<'a>(value: &'a Value, name: &str) -> Result<&'a [Value], SurfaceError> {
    match value {
        Value::List(v) => Ok(v),
        _ => Err(error(name, "expected a list")),
    }
}

fn quantity(s: &str, units: &[(&str, f32)], name: &str) -> Result<f32, SurfaceError> {
    for &(unit, factor) in units {
        if let Some(v) = s.strip_suffix(unit) {
            if let Ok(n) = v.parse::<f32>() {
                let n = n * factor;
                if n.is_finite() {
                    return Ok(n);
                }
            }
        }
    }
    Err(error(name, "invalid quantity or unit"))
}
pub fn angle(s: &str, name: &str) -> Result<f32, SurfaceError> {
    quantity(
        s,
        &[("deg", std::f32::consts::PI / 180.), ("rad", 1.)],
        name,
    )
}
fn length(s: &str, name: &str) -> Result<f32, SurfaceError> {
    quantity(s, &[("mm", 0.001), ("cm", 0.01), ("m", 1.)], name)
}
pub fn normal(s: &str, name: &str) -> Result<V3, SurfaceError> {
    let p: Vec<_> = s.split_whitespace().map(str::parse::<f32>).collect();
    if p.len() != 3 || p.iter().any(|v| !matches!(v, Ok(n) if n.is_finite())) {
        return Err(error(name, "expected three finite numbers"));
    }
    let n = norm([
        p[0].clone().unwrap(),
        p[1].clone().unwrap(),
        p[2].clone().unwrap(),
    ]);
    if dot(n, n) == 0. {
        return Err(error(name, "normal must not be zero"));
    }
    Ok(n)
}
pub fn position(s: &str, name: &str) -> Result<[Option<f32>; 3], SurfaceError> {
    let parts: Vec<_> = s.split_whitespace().collect();
    if parts.len() != 3 {
        return Err(error(name, "expected three lengths or auto"));
    }
    let mut p = [None; 3];
    for i in 0..3 {
        if parts[i] != "auto" {
            p[i] = Some(length(parts[i], name)?);
        }
    }
    Ok(p)
}
pub fn resolve(p: [Option<f32>; 3], centre: V3) -> V3 {
    std::array::from_fn(|i| p[i].unwrap_or(centre[i]))
}

#[derive(Clone, Debug, PartialEq)]
pub struct Orbit {
    pub theta: f32,
    pub phi: f32,
    pub radius: f32,
    pub relative: bool,
}
impl Orbit {
    pub fn parse(s: &str) -> Result<Self, SurfaceError> {
        let name = "cameraOrbit";
        let p: Vec<_> = s.split_whitespace().collect();
        if p.len() != 3 {
            return Err(error(name, "expected two angles and a radius"));
        }
        let theta = if p[0] == "auto" {
            0.
        } else {
            angle(p[0], name)?
        };
        let phi = if p[1] == "auto" {
            75f32.to_radians()
        } else {
            angle(p[1], name)?
        };
        let relative = p[2].ends_with('%') || p[2] == "auto";
        let radius = if p[2] == "auto" {
            1.05
        } else if relative {
            quantity(p[2], &[("%", 0.01)], name)?
        } else {
            length(p[2], name)?
        };
        if radius <= 0. {
            return Err(error(name, "radius must be positive"));
        }
        Ok(Self {
            theta,
            phi: phi.clamp(0., std::f32::consts::PI),
            radius,
            relative,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Arguments {
    pub src: String,
    pub controls: bool,
    pub orbit: Orbit,
    pub target: [Option<f32>; 3],
    pub fov: f32,
    pub disable_pan: bool,
    pub disable_zoom: bool,
    pub decay: f32,
    pub nodes: Vec<NodeStyle>,
    pub hotspots: Vec<Hotspot>,
    pub min_hotspot_opacity: f32,
    pub clip_position: [Option<f32>; 3],
    pub clip_normal: V3,
}
impl Arguments {
    pub fn parse(inputs: &[Value]) -> Result<Self, SurfaceError> {
        if inputs.len() > NAMES.len() {
            return Err(error("modelViewer", "too many arguments"));
        }
        let mut v = defaults();
        v[..inputs.len()].clone_from_slice(inputs);
        let fov = match string(&v[4], NAMES[4])? {
            "auto" => 30f32.to_radians(),
            s => angle(s, NAMES[4])?,
        };
        if !(0.0..std::f32::consts::PI).contains(&fov) || fov == 0. {
            return Err(error(
                NAMES[4],
                "expected an angle between 0 and 180 degrees",
            ));
        }
        let decay = number(&v[7], NAMES[7])?;
        if decay < 0. {
            return Err(error(NAMES[7], "expected a nonnegative duration"));
        }
        let normal = string(&v[11], NAMES[11])?;
        let clip_normal = if normal.is_empty() {
            [0.; 3]
        } else {
            self::normal(normal, NAMES[11])?
        };
        Ok(Self {
            src: string(&v[0], NAMES[0])?.into(),
            controls: boolean(&v[1], NAMES[1])?,
            orbit: Orbit::parse(string(&v[2], NAMES[2])?)?,
            target: position(string(&v[3], NAMES[3])?, NAMES[3])?,
            fov,
            disable_pan: boolean(&v[5], NAMES[5])?,
            disable_zoom: boolean(&v[6], NAMES[6])?,
            decay,
            nodes: NodeStyle::parse_list(&v[8])?,
            hotspots: Hotspot::parse_list(&v[9])?,
            min_hotspot_opacity: unit_interval(&v[12], NAMES[12])?,
            clip_position: position(string(&v[10], NAMES[10])?, NAMES[10])?,
            clip_normal,
        })
    }
}
