//! Android gives a bare symbol to the first covering family of the same
//! language preference. A family in the system XML names files and TTC indices:
//! two entries with the same OpenType family name can straddle the emoji face.
use fontique::{Attributes, Collection, FamilyId, FontInfo, GenericFamily, SourceKind};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct Face {
    id: (FamilyId, usize),
    font: FontInfo,
}

pub(super) struct Families(Vec<Family>);

struct Family {
    languages: Vec<icu_locale::LanguageIdentifier>,
    compact: bool,
    faces: Vec<(Option<String>, Face)>,
    emoji: bool,
}

/// Keep XML entries distinct even when their OpenType family names match.
impl Families {
    #[cfg(target_os = "android")]
    pub(super) fn system(collection: &mut Collection, files: &HashMap<u64, Arc<Path>>) -> Self {
        ["/system/etc/font_fallback.xml", "/system/etc/fonts.xml"]
            .iter()
            .find_map(|path| {
                Self::read(
                    collection,
                    files,
                    Path::new("/system/fonts"),
                    &std::fs::read_to_string(path).ok()?,
                )
            })
            .unwrap_or(Self(Vec::new()))
    }

    pub(super) fn read(
        collection: &mut Collection,
        files: &HashMap<u64, Arc<Path>>,
        directory: &Path,
        xml: &str,
    ) -> Option<Self> {
        let doc = roxmltree::Document::parse(xml).ok()?;
        let mut listed = HashMap::new();
        for name in super::fonts::family_names(collection) {
            let Some(family) = collection.family_by_name(&name) else {
                continue;
            };
            for (i, font) in family.fonts().iter().enumerate() {
                let SourceKind::Memory(blob) = font.source().kind() else {
                    continue;
                };
                if let Some(path) = files
                    .get(&blob.id())
                    .and_then(|path| path.strip_prefix(directory).ok())
                {
                    listed.insert(
                        (path, font.index()),
                        Face {
                            id: (family.id(), i),
                            font: font.clone(),
                        },
                    );
                }
            }
        }

        let mut groups = Vec::new();
        for family in doc
            .root_element()
            .children()
            .filter(|n| n.has_tag_name("family") && n.attribute("name").is_none())
        {
            let mut faces = Vec::new();
            for node in family.children().filter(|n| n.has_tag_name("font")) {
                let name = node.text()?.trim();
                let index = node.attribute("index").unwrap_or("0").parse().ok()?;
                // A partial chain could move a colour face ahead of a missing text subset.
                let face = listed.get(&(Path::new(name), index))?;
                faces.push((
                    node.attribute("fallbackFor").map(str::to_owned),
                    face.clone(),
                ));
            }
            if !faces.is_empty() {
                let languages = family
                    .attribute("lang")
                    .unwrap_or("")
                    .split(',')
                    .filter_map(expanded)
                    .collect();
                groups.push(Family {
                    languages,
                    emoji: family
                        .attribute("lang")
                        .unwrap_or("")
                        .split(',')
                        .any(|tag| tag.eq_ignore_ascii_case("und-Zsye")),
                    compact: family.attribute("variant") != Some("elegant"),
                    faces,
                });
            }
        }
        (!groups.is_empty()).then_some(Self(groups))
    }

    pub(super) fn configure(self: &Arc<Self>, collection: &mut Collection, locale: &str) {
        if self.0.is_empty() {
            return;
        }
        let style = locale
            .parse::<icu_locale::Locale>()
            .ok()
            .and_then(|locale| {
                use icu_locale::extensions::unicode::key;
                let value = locale
                    .extensions
                    .unicode
                    .keywords
                    .get(&key!("em"))?
                    .as_single_subtag()?;
                match value.as_str() {
                    "text" => Some(false),
                    "emoji" => Some(true),
                    _ => None,
                }
            });
        let locale = expanded(locale);
        let mut order: Vec<_> = (0..self.0.len()).collect();
        order.sort_by_key(|&i| {
            let family = &self.0[i];
            let score = locale.as_ref().map_or(0, |want| {
                family
                    .languages
                    .iter()
                    .map(|font| {
                        if want.script != font.script {
                            0
                        } else if want.language == font.language {
                            3
                        } else {
                            1
                        }
                    })
                    .max()
                    .unwrap_or(0)
            });
            std::cmp::Reverse((
                style.is_some_and(|s| family.emoji == s),
                score,
                family.compact,
            ))
        });
        let families = Arc::clone(self);
        let cache = Mutex::new(HashMap::new());
        collection.set_symbol_fallback(move |ch, attrs: Attributes, base| {
            let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
            let faces = cache.entry((ch, base)).or_insert_with(|| {
                let base = match base {
                    GenericFamily::Serif | GenericFamily::UiSerif => "serif",
                    GenericFamily::Monospace | GenericFamily::UiMonospace => "monospace",
                    _ => "sans-serif",
                };
                for &i in &order {
                    let family = &families.0[i];
                    let specific = family
                        .faces
                        .iter()
                        .any(|(target, _)| target.as_deref() == Some(base));
                    let target = specific.then_some(base);
                    let covered: Vec<_> = family
                        .faces
                        .iter()
                        .filter(|(for_family, _)| for_family.as_deref() == target)
                        .map(|(_, face)| face)
                        .filter(|face| {
                            face.font.load(None).is_some_and(|blob| {
                                face.font
                                    .charmap_index()
                                    .charmap(blob.as_ref())
                                    .and_then(|map| map.map(ch))
                                    .is_some_and(|glyph| glyph != 0)
                            })
                        })
                        .collect();
                    if !covered.is_empty() {
                        return covered.into_iter().map(|f| (f.font.clone(), f.id)).unzip();
                    }
                }
                (Vec::new(), Vec::new())
            });
            let (fonts, ids) = faces;
            let index = fontique::match_font(fonts, attrs.width, attrs.style, attrs.weight, true)?;
            Some(ids[index])
        });
    }
}

fn expanded(tag: &str) -> Option<icu_locale::LanguageIdentifier> {
    let mut locale = tag.parse::<icu_locale::Locale>().ok()?.id;
    let language = locale.language;
    icu_locale::LocaleExpander::new_common().maximize(&mut locale);
    // An und-Script font matches that script in any language.
    locale.language = language;
    Some(locale)
}
