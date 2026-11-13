//! Which texture payload a device fetches. The world names each texture by
//! its authored `.tex` name; the host fetches the file this surface asks for.
//! @ref llp/1046.003-game-engine-as-built.explainer.md#compressed-textures-2026-09-23
use exact_game::asset::TextureFamily;
use exact_gpu::wgpu;

/// The family chosen from the device's granted features, and every requested
/// file name still answered or in flight, with the authored name it carries.
pub(crate) struct Payloads {
    pub family: TextureFamily,
    /// The compressed families the files on offer hold (`TextureFamily::BUNDLED`).
    held: &'static [TextureFamily],
    files: Vec<(String, String)>,
    /// Files of a previous family, which the host forgets with the next drain.
    stale: Vec<String>,
    /// Authored names whose family payload was missing: fetch the RGBA8 file.
    fallbacks: Vec<String>,
}

impl Default for Payloads {
    fn default() -> Self {
        Self {
            family: TextureFamily::Rgba8,
            held: TextureFamily::BUNDLED,
            files: Vec::new(),
            stale: Vec::new(),
            fallbacks: Vec::new(),
        }
    }
}

/// BC where granted (desktop GPUs, Apple silicon), else ASTC (phones), else
/// RGBA8, among the families `held`. BC is preferred where both exist: BC4
/// halves one-channel maps and the measured quality of the others is equal.
pub(crate) fn family(features: wgpu::Features, held: &[TextureFamily]) -> TextureFamily {
    [
        (TextureFamily::Bc, wgpu::Features::TEXTURE_COMPRESSION_BC),
        (
            TextureFamily::Astc,
            wgpu::Features::TEXTURE_COMPRESSION_ASTC,
        ),
    ]
    .into_iter()
    .find(|(family, feature)| held.contains(family) && features.contains(*feature))
    .map_or(TextureFamily::Rgba8, |(family, _)| family)
}

impl<G: exact_game::Game, P: super::Executor, const ASSETS: bool, H: crate::Hooks>
    super::WorldSurface<G, P, ASSETS, H>
{
    /// The compressed texture families the files on offer hold, in place of
    /// this target's bundle's (`TextureFamily::BUNDLED`): a test's, which
    /// serves every family the bake wrote.
    #[doc(hidden)]
    pub fn texture_families(mut self, held: &'static [TextureFamily]) -> Self {
        self.payloads.held = held;
        self
    }
}

impl Payloads {
    /// Adopt a device's family. On a change every earlier file is retired;
    /// the caller re-requests the textures, which then name the new family.
    pub fn choose(&mut self, features: wgpu::Features) -> bool {
        let next = family(features, self.held);
        if next == self.family {
            return false;
        }
        self.family = next;
        self.stale.extend(
            std::mem::take(&mut self.files)
                .into_iter()
                .map(|(file, _)| file),
        );
        true
    }
    fn position(&self, file: &str) -> Option<usize> {
        self.files.iter().position(|(f, _)| f == file)
    }
    /// A family payload was missing (an authored `.tex` without a bake's
    /// variants): the next drain fetches the authored RGBA8 file instead.
    pub fn missing(&mut self, file: &str) -> bool {
        let Some(at) = self.position(file) else {
            return false;
        };
        let authored = self.files[at].1.clone();
        if authored == file {
            return false;
        }
        self.files[at] = (authored.clone(), authored.clone());
        self.fallbacks.push(authored);
        true
    }
    /// Authored names to the files this device fetches.
    pub fn request(&mut self, names: Vec<String>) -> Vec<String> {
        let mut files = std::mem::take(&mut self.fallbacks);
        for name in names {
            if files.contains(&name) {
                continue;
            }
            // A missing family payload keeps fetching the authored file.
            let file = if self.position(&name).is_some() {
                name.clone()
            } else {
                self.family.name(&name)
            };
            if name.ends_with(".tex") && self.position(&file).is_none() {
                self.files.push((file.clone(), name));
            }
            files.push(file);
        }
        files
    }
    /// Authored names to every file requested for them, which the host forgets.
    pub fn retire(&mut self, names: Vec<String>) -> Vec<String> {
        let mut files = std::mem::take(&mut self.stale);
        for name in names {
            let before = files.len();
            self.files.retain(|(file, authored)| {
                let retired = *authored == name;
                if retired {
                    files.push(file.clone());
                }
                !retired
            });
            if files.len() == before {
                files.push(name);
            }
        }
        files
    }
    /// The authored name a delivered file carries.
    pub fn authored<'a>(&'a self, file: &'a str) -> &'a str {
        self.position(file).map_or(file, |at| &self.files[at].1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::Features as F;
    const BOTH: &[TextureFamily] = &[TextureFamily::Bc, TextureFamily::Astc];
    /// Payloads where both families are on offer, as on the web.
    fn payloads() -> Payloads {
        Payloads {
            held: BOTH,
            ..Payloads::default()
        }
    }
    #[test]
    fn requests_name_the_device_family_and_retire_every_file() {
        let mut p = payloads();
        assert!(p.choose(F::TEXTURE_COMPRESSION_ASTC));
        let files = p.request(vec!["fox.model".into(), "fox/0.tex".into()]);
        assert_eq!(files, ["fox.model", "fox/0.astc.tex"]);
        assert_eq!(p.authored("fox/0.astc.tex"), "fox/0.tex");
        assert_eq!(p.authored("fox.model"), "fox.model");
        // BC is preferred where both exist; the ASTC file is retired at once.
        assert!(p.choose(F::TEXTURE_COMPRESSION_BC | F::TEXTURE_COMPRESSION_ASTC));
        assert!(!p.choose(F::TEXTURE_COMPRESSION_BC));
        assert_eq!(p.request(vec!["fox/0.tex".into()]), ["fox/0.bc.tex"]);
        let retired = p.retire(vec!["fox/0.tex".into(), "fox.model".into()]);
        assert_eq!(retired, ["fox/0.astc.tex", "fox/0.bc.tex", "fox.model"]);
        assert_eq!(p.authored("fox/0.bc.tex"), "fox/0.bc.tex");
        // Without either family the authored RGBA8 file is fetched.
        p.choose(F::empty());
        assert_eq!(p.request(vec!["a.bc.tex".into()]), ["a.bc.tex"]);
        assert_eq!(p.retire(vec!["a.bc.tex".into()]), ["a.bc.tex"]);
    }
    #[test]
    fn a_missing_family_payload_falls_back_to_the_authored_file() {
        let mut p = payloads();
        p.choose(F::TEXTURE_COMPRESSION_BC);
        assert_eq!(p.request(vec!["hand.tex".into()]), ["hand.bc.tex"]);
        assert!(p.missing("hand.bc.tex"));
        assert!(!p.missing("hand.tex"));
        assert_eq!(p.request(Vec::new()), ["hand.tex"]);
        assert_eq!(p.authored("hand.tex"), "hand.tex");
        // Retirement forgets the fallback; a later request tries the family again.
        assert_eq!(p.retire(vec!["hand.tex".into()]), ["hand.tex"]);
        assert_eq!(p.request(vec!["hand.tex".into()]), ["hand.bc.tex"]);
    }
    /// A device chooses among the families its bundle holds: an iPad that
    /// reads BC and ASTC takes the ASTC its bundle holds, a Mac BC. A bundle
    /// holds one its every device reads; without it, the authored RGBA8.
    #[test]
    fn a_device_chooses_among_the_families_its_bundle_holds() {
        let pad = F::TEXTURE_COMPRESSION_BC | F::TEXTURE_COMPRESSION_ASTC;
        assert_eq!(family(pad, BOTH), TextureFamily::Bc);
        assert_eq!(family(pad, &[TextureFamily::Astc]), TextureFamily::Astc);
        assert_eq!(family(pad, &[TextureFamily::Bc]), TextureFamily::Bc);
        assert_eq!(
            family(F::TEXTURE_COMPRESSION_ASTC, &[TextureFamily::Bc]),
            TextureFamily::Rgba8
        );
        assert_eq!(family(F::empty(), BOTH), TextureFamily::Rgba8);
        let mut p = Payloads {
            held: &[TextureFamily::Astc],
            ..Payloads::default()
        };
        assert!(p.choose(pad));
        assert_eq!(p.request(vec!["fox/0.tex".into()]), ["fox/0.astc.tex"]);
        let target = if cfg!(target_os = "macos") {
            &[TextureFamily::Bc][..]
        } else if cfg!(any(target_os = "ios", target_os = "tvos")) {
            &[TextureFamily::Astc][..]
        } else {
            BOTH
        };
        assert_eq!(Payloads::default().held, target);
    }
}
