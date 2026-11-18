use serde_json::Value as J;

pub(crate) type Result<T> = std::result::Result<T, String>;

pub(crate) fn integer(v: &J) -> Result<usize> {
    v.as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| "expected an unsigned integer".into())
}

pub(crate) fn required(v: &J, key: &str) -> Result<usize> {
    integer(&v[key]).map_err(|e| format!("{key}: {e}"))
}

pub(crate) fn optional(v: &J, key: &str, default: usize) -> Result<usize> {
    v.get(key)
        .map(integer)
        .transpose()
        .map(|n| n.unwrap_or(default))
}

pub(crate) fn slice(b: &[u8], off: usize, len: usize) -> Result<&[u8]> {
    let end = off.checked_add(len).ok_or("byte range overflow")?;
    b.get(off..end)
        .ok_or_else(|| "byte range runs past its buffer".into())
}

pub(crate) fn vector<const N: usize>(v: Option<&J>, default: [f32; N]) -> Result<[f32; N]> {
    let Some(v) = v else { return Ok(default) };
    let a = v
        .as_array()
        .filter(|a| a.len() == N)
        .ok_or_else(|| format!("expected {N} numbers"))?;
    let mut out = default;
    for (to, from) in out.iter_mut().zip(a) {
        *to = from.as_f64().ok_or("expected a number")? as f32;
        if !to.is_finite() {
            return Err("number is not finite".into());
        }
    }
    Ok(out)
}

pub(crate) struct Reader<'a> {
    json: &'a J,
    bin: &'a [u8],
    decoded: Vec<Option<Vec<u8>>>,
}

struct Accessor<'a> {
    bytes: Option<&'a [u8]>,
    count: usize,
    stride: usize,
    size: usize,
    component: usize,
    normalized: bool,
}

impl Accessor<'_> {
    fn raw(&self, i: usize, c: usize) -> u32 {
        let Some(bytes) = self.bytes else { return 0 };
        let at = i * self.stride + c * self.size;
        match self.size {
            1 => u32::from(bytes[at]),
            2 => u32::from(u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())),
            _ => u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()),
        }
    }

    fn float(&self, i: usize, c: usize) -> f32 {
        let raw = self.raw(i, c);
        match (self.component, self.normalized) {
            (5120, true) => ((raw as i8 as f32) / 127.0).max(-1.0),
            (5120, false) => raw as i8 as f32,
            (5121, true) => raw as f32 / 255.0,
            (5121, false) => raw as f32,
            (5122, true) => ((raw as i16 as f32) / 32767.0).max(-1.0),
            (5122, false) => raw as i16 as f32,
            (5123, true) => raw as f32 / 65535.0,
            (5123, false) => raw as f32,
            _ => f32::from_bits(raw),
        }
    }
}

impl<'a> Reader<'a> {
    pub fn new(json: &'a J, bin: &'a [u8]) -> Result<Self> {
        let views = json["bufferViews"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let mut reader = Self {
            json,
            bin,
            decoded: vec![None; views.len()],
        };
        if let Some(accessors) = json["accessors"].as_array() {
            for (i, a) in accessors.iter().enumerate() {
                if a.get("sparse").is_some() {
                    return Err(format!("accessor {i}: sparse accessors are not read"));
                }
            }
        }
        for (i, v) in views.iter().enumerate() {
            if let Some(e) = v
                .get("extensions")
                .and_then(|e| e.get("EXT_meshopt_compression"))
            {
                reader.decoded[i] = Some(
                    reader
                        .decode(v, e)
                        .map_err(|e| format!("bufferView {i}: EXT_meshopt_compression: {e}"))?,
                );
            }
        }
        Ok(reader)
    }

    fn buffer(&self, index: usize) -> Result<&'a [u8]> {
        let b = self.json["buffers"]
            .get(index)
            .ok_or_else(|| format!("no buffer {index}"))?;
        if index != 0 || b.get("uri").is_some() {
            return Err(format!(
                "buffer {index}: external buffers are not read; use a .glb file"
            ));
        }
        slice(self.bin, 0, required(b, "byteLength")?)
    }

    fn decode(&self, view: &J, e: &J) -> Result<Vec<u8>> {
        let count = required(e, "count")?;
        let stride = required(e, "byteStride")?;
        let length = count
            .checked_mul(stride)
            .ok_or("decoded byte length overflow")?;
        if length > 256 * 1024 * 1024 {
            return Err("decoded buffer exceeds 256 MiB".into());
        }
        if required(view, "byteLength")? != length
            || optional(view, "byteStride", stride)? != stride
        {
            return Err("decoded byteLength/byteStride does not match its bufferView".into());
        }
        let source = slice(
            self.buffer(required(e, "buffer")?)?,
            optional(e, "byteOffset", 0)?,
            required(e, "byteLength")?,
        )?;
        exact_meshopt::decode(
            count,
            stride,
            source,
            e["mode"].as_str().ok_or("missing mode")?,
            e.get("filter")
                .map(|v| v.as_str().ok_or("invalid filter"))
                .transpose()?
                .unwrap_or("NONE"),
        )
    }

    fn view(&self, index: usize) -> Result<(&[u8], usize, usize)> {
        let v = self.json["bufferViews"]
            .get(index)
            .ok_or_else(|| format!("no bufferView {index}"))?;
        let offset = optional(v, "byteOffset", 0)?;
        let bytes = if let Some(bytes) = &self.decoded[index] {
            bytes.as_slice()
        } else {
            slice(
                self.buffer(required(v, "buffer")?)?,
                offset,
                required(v, "byteLength")?,
            )?
        };
        Ok((bytes, optional(v, "byteStride", 0)?, offset))
    }

    pub fn count(&self, index: usize) -> Result<usize> {
        required(
            self.json["accessors"]
                .get(index)
                .ok_or_else(|| format!("no accessor {index}"))?,
            "count",
        )
        .map_err(|e| format!("accessor {index}: {e}"))
    }

    fn accessor(&self, index: usize, kind: &str, components: usize) -> Result<Accessor<'_>> {
        let a = self.json["accessors"]
            .get(index)
            .ok_or_else(|| format!("no accessor {index}"))?;
        if a["type"].as_str() != Some(kind) {
            return Err(format!("type must be {kind}"));
        }
        let component = required(a, "componentType")?;
        let size = match component {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            5125 | 5126 => 4,
            _ => return Err(format!("invalid componentType {component}")),
        };
        let normalized = a
            .get("normalized")
            .map(|v| v.as_bool().ok_or("invalid normalized"))
            .transpose()?
            .unwrap_or(false);
        let count = required(a, "count")?;
        if count == 0 {
            return Err("count must be positive".into());
        }
        let element = size * components;
        let offset = optional(a, "byteOffset", 0)?;
        let (bytes, stride) = if let Some(view) = a.get("bufferView") {
            let vi = integer(view)?;
            let (bytes, stride, view_offset) =
                self.view(vi).map_err(|e| format!("bufferView {vi}: {e}"))?;
            let stride = if stride == 0 { element } else { stride };
            if stride < element
                || stride % size != 0
                || offset % size != 0
                || view_offset % size != 0
            {
                return Err("misaligned byteOffset or invalid byteStride".into());
            }
            let length = (count - 1)
                .checked_mul(stride)
                .and_then(|n| n.checked_add(element))
                .ok_or("byte range overflow")?;
            (
                Some(slice(bytes, offset, length).map_err(|_| "runs past its bufferView")?),
                stride,
            )
        } else {
            if offset != 0 {
                return Err("byteOffset without bufferView".into());
            }
            (None, element)
        };
        Ok(Accessor {
            bytes,
            count,
            stride,
            size,
            component,
            normalized,
        })
    }

    pub fn vectors(&self, index: usize, normal: bool) -> Result<Vec<[f32; 3]>> {
        let read = || -> Result<_> {
            let a = self.accessor(index, "VEC3", 3)?;
            let valid = if normal {
                (a.component == 5126 && !a.normalized)
                    || (matches!(a.component, 5120 | 5122) && a.normalized)
            } else {
                (a.component == 5126 && !a.normalized) || matches!(a.component, 5120..=5123)
            };
            if !valid {
                return Err("componentType/normalized is not allowed for this attribute".into());
            }
            let mut values = Vec::with_capacity(a.count);
            for i in 0..a.count {
                let v = std::array::from_fn(|c| a.float(i, c));
                if v.iter().any(|n| !n.is_finite()) {
                    return Err("attribute contains a non-finite value".into());
                }
                values.push(v);
            }
            Ok(values)
        };
        read().map_err(|e| format!("accessor {index}: {e}"))
    }

    pub fn indices(&self, index: usize, vertices: usize) -> Result<Vec<u32>> {
        let read = || -> Result<_> {
            let a = self.accessor(index, "SCALAR", 1)?;
            if !matches!(a.component, 5121 | 5123 | 5125) || a.normalized {
                return Err("indices must be unsigned integers without normalization".into());
            }
            let mut values = Vec::with_capacity(a.count);
            for i in 0..a.count {
                let value = a.raw(i, 0);
                if value as usize >= vertices {
                    return Err("index runs past the vertex count".into());
                }
                values.push(value);
            }
            Ok(values)
        };
        read().map_err(|e| format!("accessor {index}: {e}"))
    }
}
