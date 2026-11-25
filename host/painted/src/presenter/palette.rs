//! The device's colours, from its shell (LLP 1115.001 D4): on Android the
//! reader resolves Material 3's colour roles for the device, light and
//! dark, from the system's dynamic palette, and tells them whole at start
//! and whenever they change. Here each of Exact's colour roles is given the
//! Material role D4's table names for it, and the set is reported through
//! the door the Apple host reports its own through
//! (`roles::set_reported`), so that a role an author wrote, and one the
//! platform's defaults name, resolve to the device's colour.
//!
//! The names an author writes do not change, and nothing is tabled: the
//! values are the device's, computed there. A host whose shell tells no
//! palette (desktop Linux, a headless run) reports nothing and keeps each
//! role's fallback pair.
pub use super::contrast::ContrastPalette;
use super::*;
use exact_kernel::style::{roles, Color};
use exact_kernel::ColorValue;

/// Material 3's colour roles, every role of Compose's `ColorScheme`, in the
/// order a shell tells them (`Native.colors`: all of them light, then all of
/// them dark). The first twelve are those D4 maps Exact's own roles to; the
/// rest the Material look reads by Material's names ([`material`]; LLP
/// 1115.001.001 D2.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Material {
    /// `primary`.
    Primary,
    /// `onPrimary`.
    OnPrimary,
    /// `secondaryContainer`.
    SecondaryContainer,
    /// `surface`.
    Surface,
    /// `onSurface`.
    OnSurface,
    /// `onSurfaceVariant`.
    OnSurfaceVariant,
    /// `surfaceContainerLow`.
    SurfaceContainerLow,
    /// `surfaceContainer`.
    SurfaceContainer,
    /// `surfaceContainerHigh`.
    SurfaceContainerHigh,
    /// `surfaceContainerHighest`.
    SurfaceContainerHighest,
    /// `outlineVariant`.
    OutlineVariant,
    /// `error`.
    Error,
    /// `primaryContainer`.
    PrimaryContainer,
    /// `onPrimaryContainer`.
    OnPrimaryContainer,
    /// `inversePrimary`.
    InversePrimary,
    /// `secondary`.
    Secondary,
    /// `onSecondary`.
    OnSecondary,
    /// `onSecondaryContainer`.
    OnSecondaryContainer,
    /// `tertiary`.
    Tertiary,
    /// `onTertiary`.
    OnTertiary,
    /// `tertiaryContainer`.
    TertiaryContainer,
    /// `onTertiaryContainer`.
    OnTertiaryContainer,
    /// `background`.
    Background,
    /// `onBackground`.
    OnBackground,
    /// `surfaceVariant`.
    SurfaceVariant,
    /// `surfaceTint`.
    SurfaceTint,
    /// `inverseSurface`.
    InverseSurface,
    /// `inverseOnSurface`.
    InverseOnSurface,
    /// `onError`.
    OnError,
    /// `errorContainer`.
    ErrorContainer,
    /// `onErrorContainer`.
    OnErrorContainer,
    /// `outline`.
    Outline,
    /// `scrim`.
    Scrim,
    /// `surfaceBright`.
    SurfaceBright,
    /// `surfaceDim`.
    SurfaceDim,
    /// `surfaceContainerLowest`.
    SurfaceContainerLowest,
}

/// How many roles a palette holds for each scheme.
pub const MATERIAL_ROLES: usize = 36;

/// The device's palette: each [`Material`] role's colour under the light
/// scheme and under the dark one, as Android has a colour (`0xAARRGGBB`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// The light scheme's, indexed by [`Material`].
    pub light: [u32; MATERIAL_ROLES],
    /// The dark scheme's.
    pub dark: [u32; MATERIAL_ROLES],
}

/// Each family’s tones 10 through 90, 95 and 99, then its light and dark palette keys.
pub(crate) type BarSwatches = [[u32; 13]; 3];

/// The reader's role tables, contrast setting and available wallpaper swatches.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReaderPalette {
    pub(crate) roles: Palette,
    pub(crate) contrast: f32,
    pub(crate) swatches: BarSwatches,
}

impl ReaderPalette {
    /// Read the two role tables, optionally followed by contrast and the three accent families.
    pub fn from_words(words: &[i32]) -> Option<Self> {
        let (table, tail) = words.split_at_checked(2 * MATERIAL_ROLES)?;
        let roles = Palette::from_words(table)?;
        let (contrast, extra) = match tail {
            [] => (0., &[][..]),
            [bits, extra @ ..] if extra.is_empty() || extra.len() == 3 * 13 => {
                (f32::from_bits(*bits as u32), extra)
            }
            _ => return None,
        };
        if !(-1.0..=1.0).contains(&contrast) {
            return None;
        }
        let mut swatches = BarSwatches::default();
        for (dst, &src) in swatches.iter_mut().flatten().zip(extra) {
            *dst = src as u32;
        }
        Some(Self {
            roles,
            contrast,
            swatches,
        })
    }
}

use Material::*;

/// Exact's roles as Material's (LLP 1115.001 D4's table): the name an
/// author writes, the Material role, and its opacity in percent.
///
/// Four rows are not in D4's table and were chosen here: `FieldText` and
/// `HighlightText` are text on a surface (`onSurface`),
/// `-exact-opaque-separator` is the separator (`outlineVariant`, which is
/// opaque already), and `-exact-tertiary-grouped-background` is the step
/// above the secondary one (`surfaceContainerHigh`). The system hues other
/// than red and blue are left out, as D4 rules: they keep the table's pair.
const ROLES: &[(&str, Material, u32)] = &[
    ("Canvas", Surface, 100),
    ("-exact-background", Surface, 100),
    ("CanvasText", OnSurface, 100),
    ("-exact-label", OnSurface, 100),
    ("FieldText", OnSurface, 100),
    ("HighlightText", OnSurface, 100),
    ("-exact-secondary-label", OnSurfaceVariant, 100),
    ("-exact-placeholder", OnSurfaceVariant, 100),
    // Material's disabled content.
    ("-exact-tertiary-label", OnSurface, 38),
    ("GrayText", OnSurface, 38),
    ("-exact-quaternary-label", OnSurface, 12),
    ("-exact-separator", OutlineVariant, 100),
    ("-exact-opaque-separator", OutlineVariant, 100),
    ("AccentColor", Primary, 100),
    ("LinkText", Primary, 100),
    ("-exact-link", Primary, 100),
    ("ButtonText", Primary, 100),
    ("-exact-system-blue", Primary, 100),
    // A symbol nobody tinted (LLP 1115.000 D3.9): `primary`, as it was
    // while its default was `AccentColor`. Material's own answer for an
    // icon is LLP 1115.001's to give.
    ("-exact-symbol", Primary, 100),
    ("AccentColorText", OnPrimary, 100),
    ("Field", SurfaceContainerHighest, 100),
    ("-exact-fill", SurfaceContainerHighest, 100),
    ("-exact-secondary-fill", SurfaceContainerHighest, 100),
    ("-exact-tertiary-fill", SurfaceContainerHighest, 100),
    ("-exact-quaternary-fill", SurfaceContainerHighest, 100),
    ("ButtonFace", SecondaryContainer, 100),
    ("-exact-secondary-background", SurfaceContainerLow, 100),
    ("-exact-grouped-background", SurfaceContainerLow, 100),
    ("-exact-secondary-grouped-background", SurfaceContainer, 100),
    ("-exact-tertiary-background", SurfaceContainer, 100),
    (
        "-exact-tertiary-grouped-background",
        SurfaceContainerHigh,
        100,
    ),
    ("Highlight", Primary, 40),
    ("-exact-system-red", Error, 100),
];

/// The Material role, and the opacity in percent, D4's table gives one of
/// Exact's roles by name: what a widget names for the launcher to resolve
/// (LLP 1126.003 D2). `None` for a role the table leaves out.
pub(crate) fn material_of(name: &str) -> Option<(Material, u32)> {
    ROLES
        .iter()
        .find(|(role, _, _)| *role == name)
        .map(|(_, material, percent)| (*material, *percent))
}

impl Palette {
    /// Decode the reader's two role tables and its Material contrast level.
    pub fn from_reader_words(words: &[i32]) -> Option<(Self, f32)> {
        let read = ReaderPalette::from_words(words)?;
        Some((read.roles, read.contrast))
    }

    /// A palette from the shell's colours, every role light and then
    /// every role dark; `None` for any other count.
    pub fn from_words(words: &[i32]) -> Option<Palette> {
        let (light, dark) = words.split_at_checked(MATERIAL_ROLES)?;
        let colors = |half: &[i32]| -> Option<[u32; MATERIAL_ROLES]> {
            let half: [i32; MATERIAL_ROLES] = half.try_into().ok()?;
            Some(half.map(|argb| argb as u32))
        };
        Some(Palette {
            light: colors(light)?,
            dark: colors(dark)?,
        })
    }

    /// Material's tonal-spot scheme seeded by an app's accent (LLP 1115.001
    /// D4), light from its light value and dark from its dark one, at a
    /// Material contrast level: every role, as Compose's `ColorScheme` built
    /// from that colour has it, and the swatches the bars read a readable tone
    /// from, in the reader's order ([`ReaderPalette::from_words`]).
    pub(crate) fn seeded(seed: [[u8; 3]; 2], contrast: f32) -> (Palette, BarSwatches) {
        use material_colors::{
            color::Rgb, dynamic_color::DynamicScheme, palette::TonalPalette,
            scheme::variant::SchemeTonalSpot,
        };
        let [light, dark] = [0, 1].map(|i| {
            let [r, g, b] = seed[i];
            let contrast = Some(f64::from(contrast));
            SchemeTonalSpot::new(Rgb::new(r, g, b).into(), i == 1, contrast).scheme
        });
        let argb = |c: Rgb| u32::from_be_bytes([0xFF, c.red, c.green, c.blue]);
        // In [`Material`]'s order.
        let roles = |s: &DynamicScheme| {
            [
                s.primary(),
                s.on_primary(),
                s.secondary_container(),
                s.surface(),
                s.on_surface(),
                s.on_surface_variant(),
                s.surface_container_low(),
                s.surface_container(),
                s.surface_container_high(),
                s.surface_container_highest(),
                s.outline_variant(),
                s.error(),
                s.primary_container(),
                s.on_primary_container(),
                s.inverse_primary(),
                s.secondary(),
                s.on_secondary(),
                s.on_secondary_container(),
                s.tertiary(),
                s.on_tertiary(),
                s.tertiary_container(),
                s.on_tertiary_container(),
                s.background(),
                s.on_background(),
                s.surface_variant(),
                s.surface_tint(),
                s.inverse_surface(),
                s.inverse_on_surface(),
                s.on_error(),
                s.error_container(),
                s.on_error_container(),
                s.outline(),
                s.scrim(),
                s.surface_bright(),
                s.surface_dim(),
                s.surface_container_lowest(),
            ]
            .map(argb)
        };
        // Tones 10 to 90, 95 and 99, then the light and dark palette keys.
        let family = |of: fn(&DynamicScheme) -> &TonalPalette, key: fn(&DynamicScheme) -> Rgb| {
            let tones = [10, 20, 30, 40, 50, 60, 70, 80, 90, 95, 99].map(|t| of(&light).tone(t));
            let mut out = [0; 13];
            for (o, c) in out
                .iter_mut()
                .zip(tones.into_iter().chain([key(&light), key(&dark)]))
            {
                *o = argb(c);
            }
            out
        };
        let swatches = [
            family(
                |s| &s.primary_palette,
                DynamicScheme::primary_palette_key_color,
            ),
            family(
                |s| &s.secondary_palette,
                DynamicScheme::secondary_palette_key_color,
            ),
            family(|s| &s.error_palette, DynamicScheme::error_palette_key_color),
        ];
        let palette = Palette {
            light: roles(&light),
            dark: roles(&dark),
        };
        (palette, swatches)
    }

    /// What `role` is on this device under a scheme, at an opacity in
    /// percent: the device's colour is opaque, and the opacity is the role's.
    fn color(&self, role: Material, dark: bool, percent: u32) -> Color {
        let table = if dark { &self.dark } else { &self.light };
        let [_, r, g, b] = table[role as usize].to_be_bytes();
        let alpha = ((255 * percent.min(100) + 50) / 100) as u8;
        Color(u32::from_be_bytes([r, g, b, alpha]))
    }

    /// Each of Exact's roles on this device, light and dark: what
    /// `roles::set_reported` takes.
    pub fn report(&self) -> Vec<(ColorValue, bool, Color)> {
        let mut out = Vec::with_capacity(ROLES.len() * 2);
        for (name, material, percent) in ROLES {
            let Some(id) = roles::role(name) else {
                debug_assert!(false, "{name} is no colour role");
                continue;
            };
            for dark in [false, true] {
                let color = self.color(*material, dark, *percent);
                out.push((ColorValue::Role(id), dark, color));
            }
        }
        out
    }

    /// Report this palette for the process (`roles::set_reported`), before
    /// a host boots, so everything boot resolves is the device's from the
    /// start. A running presenter is told through
    /// [`Presenter::set_palette`], which also paints again.
    pub fn tell(&self) -> u64 {
        *TOLD.lock().unwrap_or_else(|e| e.into_inner()) = Some(*self);
        roles::set_reported(self.report())
    }
}

/// The palette this process was last told.
static TOLD: std::sync::Mutex<Option<Palette>> = std::sync::Mutex::new(None);

/// A Material role as the shell told it, under a scheme: what the Material
/// look reads where Exact has no role of its own for it (`outline`,
/// `onSecondaryContainer`). `None` where no palette was told.
pub fn material(role: Material, dark: bool) -> Option<[u8; 4]> {
    let told = (*TOLD.lock().unwrap_or_else(|e| e.into_inner()))?;
    Some(told.rgba(role, dark))
}

impl Palette {
    /// `role` under a scheme, as RGBA.
    pub(super) fn rgba(&self, role: Material, dark: bool) -> [u8; 4] {
        let table = if dark { self.dark } else { self.light };
        let [a, r, g, b] = table[role as usize].to_be_bytes();
        [r, g, b, a]
    }
}

/// The accent a control paints in where its node sets no `accent-color`
/// (LLP 1115.001 D4, "one accent"), and what is drawn on it: the
/// `AccentColor` and `AccentColorText` roles as the host reported them. A
/// design that replaces `AccentColor` gives the accent, with its own
/// `AccentColorText` or else the ink that reads on it, as an authored
/// `accent-color` has. `None` on a host that reports nothing, whose painter
/// keeps its own.
pub(crate) fn reported_accent(dark: bool) -> Option<([u8; 4], [u8; 4])> {
    static TINT: std::sync::LazyLock<Option<(u8, u8)>> = std::sync::LazyLock::new(|| {
        Some((roles::role("AccentColor")?, roles::role("AccentColorText")?))
    });
    let (accent, on) = (*TINT)?;
    let reported = roles::reported(ColorValue::Role(accent), dark)?;
    let on_color = || crate::paint::rgba(ColorValue::Role(on).resolve(dark));
    Some(match roles::painted_override(ColorValue::Role(accent)) {
        None => (crate::paint::rgba(reported), on_color()),
        Some(design) => {
            let design = crate::paint::rgba(design.resolve(dark));
            let ink = match roles::overridden(on) {
                Some(_) => on_color(),
                None => crate::paint::look::ink_on(design),
            };
            (design, ink)
        }
    })
}

/// What a presenter keeps of the reported colours.
#[derive(Default)]
pub(super) struct State {
    pub contrast: super::contrast::State,
    pub material_contrast: f32,
    /// The device's palette as the reader last told it, and the app's
    /// accent that replaces it with a scheme of its own.
    device: Option<ReaderPalette>,
    seed: Option<[[u8; 3]; 2]>,
    /// The reported table's generation this presenter last painted from,
    /// and the palette, whose roles past the table's (read by [`material`])
    /// can change while the table does not.
    seen: Option<(u64, Palette)>,
}

impl<D: DataSource> Presenter<D> {
    #[cfg(any(target_os = "android", test))]
    pub(crate) fn set_reader_palette(&mut self, palette: &ReaderPalette) {
        if self.colors.material_contrast != palette.contrast {
            self.colors.material_contrast = palette.contrast;
            self.dirty = true;
        }
        self.set_device_palette(*palette);
    }

    fn set_device_palette(&mut self, palette: ReaderPalette) {
        self.colors.device = Some(palette);
        self.colors.seed = self.accent_seed();
        self.apply_reader_palette();
    }

    /// After a commit: an accent the design now says, changes or drops
    /// seeds the scheme again, or gives the device's back.
    pub(super) fn sync_accent_seed(&mut self) {
        if self.colors.device.is_none() {
            return;
        }
        let seed = self.accent_seed();
        if seed != self.colors.seed {
            self.colors.seed = seed;
            self.apply_reader_palette();
        }
    }

    /// The design's `AccentColor`, light and dark, that seeds Material's
    /// scheme (LLP 1115.001 D4). A role the palette maps is no seed: it
    /// would be seeded by its own seeded colour.
    fn accent_seed(&self) -> Option<[[u8; 3]; 2]> {
        let _roles = self.host.runner().bind_roles();
        let mapped = |c: &ColorValue| match c {
            ColorValue::Role(id) => ROLES
                .iter()
                .any(|(name, ..)| roles::role(name) == Some(*id)),
            _ => false,
        };
        let accent = roles::overridden(roles::accent()).filter(|c| !mapped(c))?;
        Some([false, true].map(|dark| {
            let [r, g, b, _] = crate::paint::rgba(accent.resolve(dark));
            [r, g, b]
        }))
    }

    fn apply_reader_palette(&mut self) {
        let Some(device) = self.colors.device else {
            return;
        };
        let (roles, swatches) = match self.colors.seed {
            Some(seed) => Palette::seeded(seed, self.colors.material_contrast),
            None => (device.roles, device.swatches),
        };
        self.apply_palette(&roles, &swatches);
    }

    /// The device's palette, as its shell resolved it (LLP 1115.001 D4):
    /// told before the first frame and again, whole, whenever it changes (a
    /// new wallpaper changes the dynamic palette). Every role resolves to
    /// it from here on, and when anything resolves differently the page is
    /// painted again: paint motion re-targets (the first report corrects
    /// in place, a later one transitions, LLP 1095 D6) and nothing kept
    /// from a paint under the old colours stands.
    pub fn set_palette(&mut self, palette: &Palette) {
        self.set_device_palette(ReaderPalette {
            roles: *palette,
            contrast: self.colors.material_contrast,
            swatches: BarSwatches::default(),
        });
    }

    fn apply_palette(&mut self, palette: &Palette, swatches: &BarSwatches) {
        if self.bars.set_device_inks(palette, swatches) {
            self.dirty = true;
        }
        let generation = palette.tell();
        if self.colors.seen == Some((generation, *palette)) {
            return;
        }
        let first = self.colors.seen.is_none();
        self.colors.seen = Some((generation, *palette));
        self.host.colors_changed(first);
        self.brush.colors_changed();
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seed_gives_materials_tonal_spot_scheme_in_the_readers_order() {
        let (palette, swatches) = Palette::seeded([[0x24, 0x5c, 0x46]; 2], 0.);
        // Google's Material Color Utilities, SchemeTonalSpot(#245c46), standard contrast.
        assert_eq!(
            palette.rgba(Material::Primary, false),
            [0x1e, 0x6a, 0x4f, 0xff]
        );
        assert_eq!(
            palette.rgba(Material::SecondaryContainer, true),
            [0x35, 0x4b, 0x41, 0xff]
        );
        assert_eq!(
            palette.rgba(Material::OnSecondaryContainer, false),
            [0x35, 0x4b, 0x41, 0xff]
        );
        // The primary family's tone 40 is the light scheme's `primary`.
        assert_eq!(swatches[0][3], palette.light[Material::Primary as usize]);
    }

    #[test]
    fn role_words_keep_the_device_palette_and_carry_optional_contrast() {
        let mut roles = vec![0; 2 * MATERIAL_ROLES];
        let palette = Palette::from_words(&roles).unwrap();
        assert_eq!(Palette::from_reader_words(&roles), Some((palette, 0.)));
        roles.push(0.5f32.to_bits() as i32);
        assert_eq!(Palette::from_reader_words(&roles), Some((palette, 0.5)));
        *roles.last_mut().unwrap() = f32::NAN.to_bits() as i32;
        assert_eq!(Palette::from_reader_words(&roles), None);
    }

    #[test]
    fn a_palette_is_every_material_role_light_then_dark_and_reads_by_its_name() {
        assert_eq!(
            Palette::from_words(&[0; 24]),
            None,
            "twelve roles a scheme is the old count"
        );
        let mut words = vec![0xFF00_0000u32 as i32; MATERIAL_ROLES * 2];
        words[Material::Outline as usize] = 0xFF79_747Eu32 as i32;
        words[MATERIAL_ROLES + Material::Outline as usize] = 0xFF93_8F99u32 as i32;
        // Not told: telling a palette changes every role in the process.
        let palette = Palette::from_words(&words).expect("every role, light and dark");
        assert_eq!(
            palette.rgba(Material::Outline, false),
            [0x79, 0x74, 0x7E, 0xFF]
        );
        assert_eq!(
            palette.rgba(Material::Outline, true),
            [0x93, 0x8F, 0x99, 0xFF]
        );
    }
}
