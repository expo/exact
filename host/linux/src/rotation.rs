//! Linux's launch setting and KMS orientation. Rotation math is shared with the GPU blit.
pub(crate) use exact_painted::paint::Rotation;

pub(crate) fn requested() -> Result<Option<Rotation>, String> {
    std::env::var_os("EXACT_ROTATE")
        .map(|v| {
            v.into_string()
                .map_err(|_| "EXACT_ROTATE is not UTF-8".to_string())
                .and_then(|v| Rotation::parse(&v))
        })
        .transpose()
}

#[cfg(any(target_os = "linux", test))]
fn panel_rotation(name: &str) -> Rotation {
    // KMS names the mounted edge. Our angle turns the image clockwise,
    // undoing the panel's physical mounting (DRM's ROTATE_90 is anticlockwise).
    match name {
        "Upside Down" => Rotation::R180,
        "Left Side Up" => Rotation::R270,
        "Right Side Up" => Rotation::R90,
        _ => Rotation::R0,
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn panel(
    card: &impl drm::control::Device,
    connector: drm::control::connector::Handle,
) -> Result<Rotation, String> {
    use drm::control::property::Value;
    let props = card
        .get_properties(connector)
        .map_err(|e| format!("connector properties: {e}"))?;
    for (&handle, &value) in props.iter() {
        let property = card
            .get_property(handle)
            .map_err(|e| format!("connector property: {e}"))?;
        if property.name().to_bytes() == b"panel orientation" {
            if let Value::Enum(Some(value)) = property.value_type().convert_value(value) {
                return Ok(panel_rotation(value.name().to_str().unwrap_or_default()));
            }
        }
    }
    Ok(Rotation::R0)
}

#[cfg(any(target_os = "linux", test))]
pub(crate) fn input(
    rotation: Rotation,
    event: crate::input::InputEvent,
) -> crate::input::InputEvent {
    use crate::input::InputEvent;
    match event {
        InputEvent::Absolute(x, y) => {
            let (x, y) = rotation.input(x, y);
            InputEvent::Absolute(x, y)
        }
        // Relative devices follow the user's axes regardless of panel mounting.
        _ => event,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::InputEvent as E;

    #[test]
    fn panel_mounting_transforms_absolute_input_and_preserves_relative_axes() {
        for (name, angle) in [
            ("Normal", "0"),
            ("Upside Down", "180"),
            ("Left Side Up", "270"),
            ("Right Side Up", "90"),
        ] {
            let r = panel_rotation(name);
            assert_eq!(r, Rotation::parse(angle).unwrap());
            for (x, y) in [(10., 0.), (0., 40.), (-12., 24.)] {
                assert_eq!(input(r, E::Motion(x, y)), E::Motion(x, y));
                assert_eq!(input(r, E::Wheel(x, y)), E::Wheel(x, y));
            }
            for (x, y) in [(Some(0.25), None), (None, Some(0.75)), (Some(0.), Some(1.))] {
                let (x2, y2) = r.input(x, y);
                assert_eq!(input(r, E::Absolute(x, y)), E::Absolute(x2, y2));
            }
            for event in [
                E::Button(true),
                E::Button(false),
                E::Aux(8, true),
                E::Cancel,
                E::Key {
                    code: 30,
                    shift: true,
                    down: true,
                    repeat: false,
                },
                E::Pad {
                    index: 0,
                    connected: true,
                    reading: Default::default(),
                    at: 1.,
                },
            ] {
                assert_eq!(input(r, event), event);
            }
        }
        assert_eq!(panel_rotation(""), Rotation::R0);
    }
}
