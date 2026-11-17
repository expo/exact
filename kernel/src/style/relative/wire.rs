//! Retained lengths on incoming style patches. Exports still carry computed pixels.

use super::{Relative, Unit};
use crate::wire::codec::{Reader, Writer};
use crate::{DecodeError, StyleId, StyleProps, StyleValue};

pub(crate) fn encode(relative: &Relative, w: &mut Writer) {
    w.u16(relative.0.len() as u16);
    let mut bytes = Vec::new();
    relative.encode(&mut bytes);
    w.bytes(&bytes);
}

pub(crate) fn decode(style: &mut StyleProps, r: &mut Reader<'_>) -> Result<(), DecodeError> {
    let count = usize::from(r.u16()?);
    if count > style.mask.iter().count() {
        return Err(DecodeError::InvalidRelativeLength);
    }
    let mut previous = None;
    for _ in 0..count {
        let row = r.u32()?;
        let id = StyleId::from_bit(row).ok_or(DecodeError::InvalidRelativeLength)?;
        if !style.mask.has(id) || previous.is_some_and(|p| row <= p) {
            return Err(DecodeError::InvalidRelativeLength);
        }
        previous = Some(row);
        let unit = match r.u8()? {
            0 => Unit::Rem,
            1 => Unit::Em,
            2 => Unit::TextStyle(r.u8()?),
            3 => Unit::Hairline,
            4 => Unit::Ramp(r.u8()?),
            5 => Unit::TextStyleAtMost(r.u8()?),
            6 => Unit::Viewport(
                *crate::ViewportUnit::ALL
                    .get(usize::from(r.u8()?))
                    .ok_or(DecodeError::InvalidRelativeLength)?,
            ),
            _ => return Err(DecodeError::InvalidRelativeLength),
        };
        let n = r.f32()?;
        let num = exact_num::Shortest(f64::from(n));
        let text = match unit {
            Unit::Rem => format!("{num}rem"),
            Unit::Em => format!("{num}em"),
            Unit::Viewport(u) => format!("{num}{}", u.name()),
            Unit::Hairline if n == 1.0 => "-exact-hairline".into(),
            Unit::TextStyle(i) | Unit::TextStyleAtMost(i) => {
                let name = crate::TEXT_STYLES
                    .get(usize::from(i))
                    .ok_or(DecodeError::InvalidRelativeLength)?
                    .name;
                if matches!(unit, Unit::TextStyleAtMost(_)) {
                    format!("min(-exact-{name}, {num}px)")
                } else {
                    format!("-exact-{name}")
                }
            }
            Unit::Ramp(i) if n == 1.0 => format!(
                "-exact-{}",
                crate::LENGTH_RAMPS
                    .get(usize::from(i))
                    .ok_or(DecodeError::InvalidRelativeLength)?
                    .0
            ),
            _ => return Err(DecodeError::InvalidRelativeLength),
        };
        if super::of(id, &StyleValue::Text(text.into())).ok().flatten() != Some((unit, n)) {
            return Err(DecodeError::InvalidRelativeLength);
        }
        style.relative.put(id, Some((unit, n)));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incoming_patches_keep_all_retained_units() {
        for text in [
            "1.25rem",
            "2em",
            "-exact-title1",
            "min(-exact-body, 20px)",
            "3vmin",
        ] {
            let mut style = StyleProps::default();
            style
                .set_dynamic(StyleId::FontSize, &StyleValue::Text(text.into()))
                .unwrap();
            round_trip(style);
        }
        for text in ["-exact-hairline", "-exact-accessory-checkmark-width"] {
            let mut style = StyleProps::default();
            style
                .set_dynamic(StyleId::BorderWidthTop, &StyleValue::Text(text.into()))
                .unwrap();
            round_trip(style);
        }
    }

    fn round_trip(style: StyleProps) {
        let op = crate::Op::SetStyle {
            id: 1,
            patch: Box::new(style),
        };
        let frame = crate::wire::encode(1, 0, std::slice::from_ref(&op));
        assert_eq!(crate::wire::decode(&frame).unwrap().ops, [op]);
    }

    #[test]
    fn malformed_retained_lengths_are_refused() {
        for (row, tag, unit, n) in [
            (StyleId::FontSize, 6, 255, 1.0),
            (StyleId::FontSize, 6, 0, -1.0),
            (StyleId::FontSize, 6, 0, f32::INFINITY),
            (StyleId::FontSize, 2, 255, 1.0),
            (StyleId::FontSize, 5, 0, 0.0),
            (StyleId::FontSize, 3, 0, 1.0),
            (StyleId::Width, 6, 0, 1.0),
            (StyleId::Opacity, 6, 0, 1.0),
        ] {
            let mut style = StyleProps::default();
            style.mask.set(row);
            let mut w = Writer::new();
            w.u16(1);
            w.u32(row.bit());
            w.u8(tag);
            if matches!(tag, 2 | 4 | 5 | 6) {
                w.u8(unit);
            }
            w.f32(n);
            assert_eq!(
                decode(&mut style, &mut Reader::new(w.as_slice())),
                Err(DecodeError::InvalidRelativeLength)
            );
        }
    }
}
