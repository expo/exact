// Port of meshoptimizer v0.25's WASM filters; see lib.rs and ../LICENSE.md.
// https://github.com/zeux/meshoptimizer/blob/v0.25/src/vertexfilter.cpp
use super::Result;

pub(super) fn validate(mode: &str, stride: usize, filter: &str) -> Result<()> {
    let valid = match filter {
        "NONE" => true,
        "OCTAHEDRAL" => mode == "ATTRIBUTES" && matches!(stride, 4 | 8),
        "QUATERNION" => mode == "ATTRIBUTES" && stride == 8,
        "EXPONENTIAL" => mode == "ATTRIBUTES" && stride.is_multiple_of(4),
        _ => return Err(format!("unsupported meshopt filter {filter}")),
    };
    if valid {
        Ok(())
    } else {
        Err(format!("invalid meshopt {filter} mode/byteStride"))
    }
}

pub(super) fn apply(target: &mut [u8], stride: usize, filter: &str) {
    if filter == "EXPONENTIAL" {
        for bytes in target.as_chunks_mut::<4>().0 {
            let v = i32::from_le_bytes(*bytes);
            let value = (2f64.powi(v >> 24) * f64::from((v << 8) >> 8)) as f32;
            bytes.copy_from_slice(&value.to_le_bytes());
        }
    } else if matches!(filter, "OCTAHEDRAL" | "QUATERNION") {
        let width = stride / 4;
        for bytes in target.chunks_exact_mut(stride) {
            let mut v = [0f32; 4];
            for (i, value) in v.iter_mut().enumerate() {
                *value = if width == 1 {
                    f32::from(bytes[i] as i8)
                } else {
                    f32::from(i16::from_le_bytes([bytes[i * 2], bytes[i * 2 + 1]]))
                };
            }
            if filter == "OCTAHEDRAL" {
                let (mut x, mut y) = (v[0], v[1]);
                let z = v[2] - (x.abs() + y.abs());
                let t = (-z).max(0.);
                x -= if x >= 0. { t } else { -t };
                y -= if y >= 0. { t } else { -t };
                let h = (if width == 1 { 127. } else { 32767. }) / (x * x + (y * y + z * z)).sqrt();
                v[..3].copy_from_slice(&[x * h, y * h, z * h]);
            } else {
                let input_w = v[3] as i16;
                let max = (input_w & 3) as usize;
                let scale = (1. / 2f32.sqrt()) / f32::from(input_w | 3);
                let (x, y, z) = (v[0] * scale, v[1] * scale, v[2] * scale);
                let w = (1. - (x * x + (y * y + z * z))).max(0.).sqrt();
                for (i, value) in [w, x, y, z].into_iter().enumerate() {
                    v[(max + i) % 4] = value * 32767.;
                }
            }
            for (i, value) in v.into_iter().enumerate() {
                // npm 0.25 selects the WASM SIMD decoder: f32 operations and
                // ties-to-even, unlike the JavaScript reference's Math.round.
                let rounded = value.round_ties_even() as i32;
                bytes[i * width..(i + 1) * width].copy_from_slice(&rounded.to_le_bytes()[..width]);
            }
        }
    }
}
