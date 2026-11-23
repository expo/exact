//! The shader uniform reader shared by builds and GPU hosts (LLP 1181 D3–D8).
//! WGSL validation belongs to naga or the device; this reader inspects declarations.

mod reader;
pub use reader::parse;

/// The surface's own arguments in positional order, which cannot name
/// uniform members (LLP 1181 D1, D4).
pub const ARGUMENTS: [&str; 4] = ["src", "maxFps", "animationPlayState", "alphaMode"];

/// How the page composes the fragment's output: WebGPU's `GPUCanvasAlphaMode`
/// (LLP 1181 D8).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AlphaMode {
    /// Every pixel's alpha is one, whatever the fragment returns.
    #[default]
    Opaque,
    /// The fragment returns colour already multiplied by its alpha.
    Premultiplied,
}

/// `maxFps`: frames a second at most, 0 for the display's rate.
pub fn max_fps(value: Value<'_>) -> Result<u32, String> {
    match value {
        Value::Number(n) if n.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(&n) => {
            Ok(n as u32)
        }
        _ => Err("maxFps: expected a whole number from 0 to 4294967295".into()),
    }
}

/// `animationPlayState`: `Some(true)` running, `Some(false)` paused, `None` unsaid.
pub fn animation_play_state(value: Value<'_>) -> Result<Option<bool>, String> {
    match value {
        Value::Null => Ok(None),
        Value::String("running") => Ok(Some(true)),
        Value::String("paused") => Ok(Some(false)),
        _ => Err("animationPlayState: expected running or paused".into()),
    }
}

/// `alphaMode`, opaque when unsaid.
pub fn alpha_mode(value: Value<'_>) -> Result<AlphaMode, String> {
    match value {
        Value::Null | Value::String("opaque") => Ok(AlphaMode::Opaque),
        Value::String("premultiplied") => Ok(AlphaMode::Premultiplied),
        _ => Err("alphaMode: expected opaque or premultiplied".into()),
    }
}

/// Checks a value of `maxFps`, `animationPlayState` or `alphaMode` as the
/// surface reads it when bound: the build holds a literal to the same rule.
pub fn check_argument(name: &str, value: Value<'_>) -> Result<(), String> {
    match name {
        "maxFps" => max_fps(value).map(drop),
        "animationPlayState" => animation_play_state(value).map(drop),
        "alphaMode" => alpha_mode(value).map(drop),
        _ => Ok(()),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UniformType {
    F32,
    I32,
    U32,
    Vec2f,
    Vec3f,
    Vec4f,
}

impl UniformType {
    pub const fn name(self) -> &'static str {
        match self {
            Self::F32 => "f32",
            Self::I32 => "i32",
            Self::U32 => "u32",
            Self::Vec2f => "vec2f",
            Self::Vec3f => "vec3f",
            Self::Vec4f => "vec4f",
        }
    }

    pub const fn alignment(self) -> usize {
        match self {
            Self::F32 | Self::I32 | Self::U32 => 4,
            Self::Vec2f => 8,
            Self::Vec3f | Self::Vec4f => 16,
        }
    }

    pub const fn size(self) -> usize {
        match self {
            Self::F32 | Self::I32 | Self::U32 => 4,
            Self::Vec2f => 8,
            Self::Vec3f => 12,
            Self::Vec4f => 16,
        }
    }
}

/// A byte offset and one-based line and character column in the original WGSL.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Location {
    pub offset: usize,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub message: String,
    pub location: Option<Location>,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(at) = self.location {
            write!(f, "{}:{}: ", at.line, at.column)?;
        }
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Member {
    pub name: String,
    pub ty: UniformType,
    pub offset: usize,
    pub location: Location,
}

/// No uniform binding is represented by an unnamed, empty layout of size zero.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Layout {
    pub name: Option<String>,
    pub members: Vec<Member>,
    pub alignment: usize,
    pub size: usize,
}

/// Values independent of the host's value representation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value<'a> {
    Null,
    Number(f64),
    Bool(bool),
    String(&'a str),
    List(&'a [Value<'a>]),
    /// Caller-parsed, sRGB-encoded RGBA, not premultiplied; each channel is 0–1.
    Color([f64; 4]),
}

/// Host-supplied values, already in seconds and device pixels (LLP 1181 D6).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Builtins {
    pub time: f64,
    pub resolution: [f64; 2],
    pub device_pixel_ratio: f64,
}

impl Default for Builtins {
    fn default() -> Self {
        Self {
            time: 0.0,
            resolution: [0.0; 2],
            device_pixel_ratio: 1.0,
        }
    }
}

impl Layout {
    /// Pack a complete binding snapshot. Callers retain values omitted by a changed bind.
    /// Failure leaves the caller's previous bytes intact. Padding and unbound values are zero.
    pub fn pack(&self, values: &[(&str, Value<'_>)], builtins: Builtins) -> Result<Vec<u8>, Error> {
        for (i, (name, _)) in values.iter().enumerate() {
            if !self.members.iter().any(|m| m.name == *name) {
                let members = self
                    .members
                    .iter()
                    .map(|m| m.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(Error {
                    message: format!("unknown uniform argument `{name}`; members: [{members}]"),
                    location: None,
                });
            }
            if values[..i].iter().any(|(previous, _)| previous == name) {
                return Err(Error {
                    message: format!("duplicate uniform argument `{name}`"),
                    location: None,
                });
            }
        }
        let mut bytes = vec![0; self.size];
        let resolution = builtins.resolution.map(Value::Number);
        for member in &self.members {
            let value = values
                .iter()
                .find(|(name, _)| *name == member.name)
                .map(|(_, value)| *value)
                .or(match member.name.as_str() {
                    "time" => Some(Value::Number(builtins.time)),
                    "resolution" => Some(Value::List(&resolution)),
                    "devicePixelRatio" => Some(Value::Number(builtins.device_pixel_ratio)),
                    _ => None,
                });
            if let Some(value) = value {
                let out = &mut bytes[member.offset..member.offset + member.ty.size()];
                member.write(value, out)?;
            }
        }
        Ok(bytes)
    }
}

impl Member {
    fn error(&self, message: &str) -> Error {
        Error {
            message: format!("uniform argument `{}`: {message}", self.name),
            location: None,
        }
    }

    fn float(&self, value: Value<'_>) -> Result<f32, Error> {
        match value {
            Value::Number(n) if n.is_finite() && n.abs() <= f64::from(f32::MAX) => Ok(n as f32),
            _ => Err(self.error("expected a finite number in the f32 range")),
        }
    }

    fn write(&self, value: Value<'_>, bytes: &mut [u8]) -> Result<(), Error> {
        match self.ty {
            UniformType::F32 => bytes.copy_from_slice(&self.float(value)?.to_le_bytes()),
            UniformType::I32 | UniformType::U32 => {
                let n = match value {
                    Value::Number(n) => n,
                    Value::Bool(b) if self.ty == UniformType::U32 => f64::from(b),
                    _ => return Err(self.error("expected a whole number (or a bool for u32)")),
                };
                let (min, max) = if self.ty == UniformType::I32 {
                    (f64::from(i32::MIN), f64::from(i32::MAX))
                } else {
                    (0.0, f64::from(u32::MAX))
                };
                if !n.is_finite() || n.fract() != 0.0 || !(min..=max).contains(&n) {
                    return Err(self.error(&format!("expected a whole number from {min} to {max}")));
                }
                let word = if self.ty == UniformType::I32 {
                    (n as i32).to_le_bytes()
                } else {
                    (n as u32).to_le_bytes()
                };
                bytes.copy_from_slice(&word);
            }
            UniformType::Vec2f | UniformType::Vec3f | UniformType::Vec4f => {
                if let Value::Color(rgba) = value {
                    if self.ty == UniformType::Vec4f {
                        if !rgba.iter().all(|n| (0.0..=1.0).contains(n)) {
                            return Err(self.error("expected finite colour channels from 0 to 1"));
                        }
                        for (out, n) in bytes.as_chunks_mut::<4>().0.iter_mut().zip(rgba) {
                            out.copy_from_slice(&(n as f32).to_le_bytes());
                        }
                        return Ok(());
                    }
                }
                let count = self.ty.size() / 4;
                let Value::List(items) = value else {
                    return Err(self.error(&format!("expected a list of {count} numbers")));
                };
                if items.len() != count {
                    return Err(self.error(&format!("expected a list of {count} numbers")));
                }
                for (out, value) in bytes.as_chunks_mut::<4>().0.iter_mut().zip(items) {
                    out.copy_from_slice(&self.float(*value)?.to_le_bytes());
                }
            }
        }
        Ok(())
    }
}
