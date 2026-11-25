//! Accent ink placed directly on an authored bar fill (LLP 1115.001 D11).
use super::{opaque, told, Inputs, Role};
use crate::paint::look::{ink_on, luminance};
use crate::presenter::palette::{BarSwatches, Material, Palette};
use material_colors::{
    color::Rgb,
    dynamic_color::{DynamicColor, DynamicScheme, MaterialDynamicColors},
};

pub(super) const TEXT: f32 = 4.5;
pub(super) const SYMBOL: f32 = 3.0;

pub(in crate::presenter::bars) fn over(fill: [u8; 4], base: [u8; 4]) -> [u8; 4] {
    let a = u32::from(fill[3]);
    let mut result = [255; 4];
    for i in 0..3 {
        result[i] = ((u32::from(fill[i]) * a + u32::from(base[i]) * (255 - a) + 127) / 255) as u8;
    }
    result
}

fn rgb([r, g, b, _]: [u8; 4]) -> Rgb {
    Rgb::new(r, g, b)
}

#[derive(Default)]
pub(in crate::presenter) struct DeviceInks {
    primary: DeviceFamily,
    secondary: DeviceFamily,
    error: DeviceFamily,
}

#[derive(Default, PartialEq, Eq)]
struct DeviceFamily {
    tones: Vec<[u8; 4]>,
    keys: [u32; 2],
}

impl super::super::State {
    pub(in crate::presenter) fn set_device_inks(
        &mut self,
        palette: &Palette,
        swatches: &BarSwatches,
    ) -> bool {
        use Material::*;
        let family = |roles: &[Material], extra: &[u32; 13]| {
            let tones = roles
                .iter()
                .flat_map(|&r| [false, true].map(|d| palette.rgba(r, d)))
                .chain(extra[..11].iter().map(|v| {
                    let [a, r, g, b] = v.to_be_bytes();
                    [r, g, b, a]
                }))
                .filter(|c| c[3] == 255)
                .collect();
            DeviceFamily {
                tones,
                keys: [extra[11], extra[12]],
            }
        };
        let primary = family(
            &[
                Primary,
                OnPrimary,
                PrimaryContainer,
                OnPrimaryContainer,
                InversePrimary,
            ],
            &swatches[0],
        );
        let secondary = family(
            &[
                Secondary,
                OnSecondary,
                SecondaryContainer,
                OnSecondaryContainer,
            ],
            &swatches[1],
        );
        let error = family(
            &[Error, OnError, ErrorContainer, OnErrorContainer],
            &swatches[2],
        );
        let changed = [
            primary != self.device_inks.primary,
            secondary != self.device_inks.secondary,
            error != self.device_inks.error,
        ];
        for (dirty, change) in self.ink_changed.iter_mut().zip(changed) {
            *dirty |= change;
        }
        self.device_inks = DeviceInks {
            primary,
            secondary,
            error,
        };
        changed.contains(&true)
    }
}

#[derive(Clone, Copy)]
pub(super) enum Accent {
    Primary,
    Secondary,
    OnSecondaryContainer,
    Error,
}

impl Accent {
    fn device(self, dark: bool) -> [u8; 4] {
        match self {
            Self::Primary => Role::Primary.color(dark),
            Self::Secondary => told(Material::Secondary, dark, 0xFF62_5B71, 0xFFCC_C2DC),
            Self::OnSecondaryContainer => told(
                Material::OnSecondaryContainer,
                dark,
                0xFF1D_192B,
                0xFFE8_DEF8,
            ),
            Self::Error => Role::Error.color(dark),
        }
    }

    fn role(self) -> DynamicColor<'static> {
        match self {
            Self::Primary => MaterialDynamicColors::primary(),
            Self::Secondary => MaterialDynamicColors::secondary(),
            Self::OnSecondaryContainer => MaterialDynamicColors::on_secondary_container(),
            Self::Error => MaterialDynamicColors::error(),
        }
    }
}

impl Inputs<'_> {
    pub(super) fn accent_ink(
        &self,
        scheme: Option<&DynamicScheme>,
        accent: Accent,
        ground: Option<[u8; 4]>,
        minimum: f32,
    ) -> [u8; 4] {
        let role = accent.role();
        let ink = scheme.map_or_else(|| accent.device(self.dark), |s| opaque(role.get_rgb(s)));
        let Some(background) = ground else { return ink };
        let ground = luminance(background) + 0.05;
        let contrast = |ink| {
            let y = luminance(over(ink, background)) + 0.05;
            y.max(ground) / y.min(ground)
        };
        let reads = |ink| contrast(ink) >= minimum;
        if reads(ink) {
            return ink;
        }
        let counterpart = scheme.map_or_else(
            || accent.device(!self.dark),
            |s| {
                let other = DynamicScheme {
                    is_dark: !s.is_dark,
                    ..s.clone()
                };
                opaque(role.get_rgb(&other))
            },
        );
        if reads(counterpart) {
            return counterpart;
        }

        let black = [0, 0, 0, 255];
        let white = [255; 4];
        let plain = ink_on(background);
        let palette = if let Some(scheme) = scheme {
            *role.palette(scheme)
        } else {
            let family = match accent {
                Accent::Primary => &self.device_inks.primary,
                Accent::Secondary | Accent::OnSecondaryContainer => &self.device_inks.secondary,
                Accent::Error => &self.device_inks.error,
            };
            if let Some(ink) = family
                .tones
                .iter()
                .copied()
                .filter(|&c| c != black && c != white && reads(c))
                .min_by(|a, b| contrast(*a).total_cmp(&contrast(*b)))
            {
                return ink;
            }
            let [alpha, r, g, b] = family.keys[self.dark as usize].to_be_bytes();
            if alpha == 0 {
                return plain;
            }
            material_colors::palette::TonalPalette::from_hct(Rgb::new(r, g, b).into())
        };
        let ground_tone = rgb(background).as_lstar();
        let at = |tone| opaque(palette.get_hct(tone).into());
        let suggested = at(DynamicColor::foreground_tone(
            ground_tone,
            f64::from(minimum),
        ));
        if suggested != black && suggested != white && reads(suggested) {
            return suggested;
        }

        // A midtone can leave less than one HCT tone between readable ink and black or white.
        let ends = if plain == black {
            [0., 100.]
        } else {
            [100., 0.]
        };
        for mut end in ends {
            let endpoint = at(end);
            if !reads(endpoint) {
                continue;
            }
            let mut near = ground_tone;
            loop {
                let mid = (near + end) / 2.;
                if mid == near || mid == end {
                    break;
                }
                let ink = at(mid);
                if ink == endpoint {
                    end = mid;
                } else if reads(ink) {
                    return ink;
                } else {
                    near = mid;
                }
            }
        }
        plain
    }
}
