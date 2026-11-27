//! The reader measures each resolved font segment before Parley stores its
//! clusters. Android 10 exposes widths by UTF-16 offset. Android 12 also
//! exposes the positions of individual glyphs inside a cluster.
mod ascii;

use super::*;
use jni::objects::{JCharArray, JClass, JFloatArray, JIntArray, JObject};
use jni::refs::Global;
use jni::{jni_sig, jni_str, EnvUnowned, JValue, JavaVM};
use parley::{AdvanceRequest, GlyphAdvance, RunAdvances};
use std::path::Path;
use std::sync::OnceLock;

struct Reader {
    vm: JavaVM,
    origins: bool,
    class: Global<JClass<'static>>,
}
static READER: OnceLock<Reader> = OnceLock::new();

/// # Safety
/// The reader supplied this environment on its JNI entry thread.
#[allow(unsafe_code)]
pub(crate) unsafe fn attach(raw: *mut jni_sys::JNIEnv) {
    if READER.get().is_some() {
        return;
    }
    EnvUnowned::from_raw(raw.cast())
        .with_env(|env| -> jni::errors::Result<()> {
            let class = env.find_class(jni_str!("dev/exact/canvas/GlyphAdvances"))?;
            let _ = READER.set(Reader {
                origins: env
                    .call_static_method(&class, jni_str!("hasOrigins"), jni_sig!("()Z"), &[])?
                    .z()?,
                vm: env.get_java_vm()?,
                class: env.new_global_ref(class)?,
            });
            Ok(())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>();
}

impl catalog::Catalog {
    pub(super) fn use_density(&mut self, density: f32) {
        if self.density != density {
            self.density = density;
            self.layout = context(&self.files, density);
        }
    }
}

impl TextEngine {
    pub(crate) fn set_advance_density(&mut self, density: f32) -> bool {
        let catalog = self.catalog.borrow();
        if catalog.density == density {
            return false;
        }
        let mut next = catalog.with_locale(&catalog.locale);
        drop(catalog);
        next.use_density(density);
        self.catalog = Rc::new(RefCell::new(next));
        self.paragraphs = cache::Cache::default();
        self.release_held();
        true
    }
}

pub(super) fn context(
    files: &Arc<HashMap<u64, Arc<Path>>>,
    density: f32,
) -> parley::LayoutContext<u32> {
    let mut layout = parley::LayoutContext::new();
    if let Some(reader) = READER.get() {
        let files = files.clone();
        layout.set_advance_measure(Some(Arc::new(move |run| {
            let path = files.get(&run.font.blob.id()).and_then(|p| p.to_str());
            reader.measure(run, path, density)
        })));
    }
    layout
}

impl Reader {
    fn measure(
        &self,
        run: &AdvanceRequest<'_>,
        path: Option<&str>,
        density: f32,
    ) -> Option<RunAdvances> {
        let utf16: Vec<_> = run.text.encode_utf16().collect();
        let mut offsets = Vec::with_capacity(utf16.len() + 1);
        let mut offset = 0;
        for c in run.text.chars() {
            offsets.push(offset);
            offset += c.len_utf16();
        }
        offsets.push(offset);
        // Null spans encode one unoffset glyph per UTF-16 unit in visual order.
        let mut spans = Vec::new();
        let origins = self.origins && !ascii::on_pens(run);
        if origins {
            let simple = run.infos.len() == utf16.len()
                && offsets.len() == utf16.len() + 1
                && run
                    .infos
                    .iter()
                    .zip(run.positions)
                    .enumerate()
                    .all(|(i, (g, p))| {
                        g.cluster as usize == if run.rtl { utf16.len() - 1 - i } else { i }
                            && p.x_offset == 0
                    });
            if !simple {
                for (glyphs, chars) in clusters(run, &offsets) {
                    spans.extend([
                        glyphs.start as i32,
                        glyphs.end as i32,
                        chars.start as i32,
                        chars.end as i32,
                    ]);
                }
            }
        }
        let mut axes: Vec<_> = run
            .font
            .synthesis
            .variation_settings()
            .iter()
            .map(|(tag, value)| (tag.to_be_bytes(), *value))
            .collect();
        for v in run.variations {
            let tag = v.tag.to_bytes();
            if let Some(old) = axes.iter_mut().find(|(t, _)| t == &tag) {
                old.1 = v.value;
            } else {
                axes.push((tag, v.value));
            }
        }
        let axes = axes
            .iter()
            .map(|(tag, v)| format!("'{}' {v}", String::from_utf8_lossy(tag)))
            .collect::<Vec<_>>()
            .join(",");
        let features = run
            .features
            .iter()
            .map(|f| format!("'{}' {}", f.tag, f.value))
            .collect::<Vec<_>>()
            .join(",");
        let extras =
            i32::from(run.text.contains('\t')) | (i32::from(run.text.contains('\u{ad}')) << 1);
        let measured = self.vm.attach_current_thread(|env| -> jni::errors::Result<Vec<f32>> {
            let chars = JCharArray::new(env, utf16.len())?;
            chars.set_region(env, 0, &utf16)?;
            let (ids, spans): (JObject, JObject) = if !origins {
                (JObject::null(), JObject::null())
            } else {
                let ids: Vec<_> = run.infos.iter().map(|g| g.glyph_id as i32).collect();
                let id_array = JIntArray::new(env, ids.len())?;
                id_array.set_region(env, 0, &ids)?;
                let span_array = if spans.is_empty() { JObject::null() } else {
                    let array = JIntArray::new(env, spans.len())?;
                    array.set_region(env, 0, &spans)?;
                    array.into()
                };
                (id_array.into(), span_array)
            };
            // Kotlin copies the bytes into a Java-owned buffer on a typeface
            // cache miss. The borrowed buffer never escapes this call.
            let bytes = run.font.blob.data();
            #[allow(unsafe_code)]
            let buffer: JObject = unsafe {
                env.new_direct_byte_buffer(bytes.as_ptr().cast_mut(), bytes.len())?
            }.into();
            let path = env.new_string(path.unwrap_or(""))?;
            let axes = env.new_string(&axes)?;
            let features = env.new_string(&features)?;
            let locale = env.new_string(run.locale.unwrap_or(""))?;
            let result = env.call_static_method(
                &*self.class, jni_str!("measure"),
                jni_sig!("(JLjava/lang/String;Ljava/nio/ByteBuffer;ILjava/lang/String;Ljava/lang/String;Ljava/lang/String;F[CZI[I[I)[F"),
                &[JValue::Long(run.font.blob.id() as i64), JValue::Object(&path), JValue::Object(&buffer), JValue::Int(run.font.index as i32), JValue::Object(&axes),
                  JValue::Object(&features), JValue::Object(&locale), JValue::Float(run.size * density),
                  JValue::Object(&chars), JValue::Bool(run.rtl), JValue::Int(extras), JValue::Object(&ids), JValue::Object(&spans)],
            )?.l()?;
            let array = JFloatArray::cast_local(env, result)?;
            let mut values = vec![0.; array.len(env)?];
            array.get_region(env, 0, &mut values)?;
            Ok(values)
        }).inspect_err(|e| eprintln!("exact: Android text measurement: {e}")).ok()?;
        let (extra, widths) = measured.split_at_checked(2)?;
        let (widths, xs) = widths.split_at_checked(utf16.len())?;
        let factor = run.size / run.units_per_em;
        let mut glyphs: Vec<_> = run
            .positions
            .iter()
            .map(|p| GlyphAdvance {
                advance: p.x_advance as f32 * factor,
                x_offset: p.x_offset as f32 * factor,
            })
            .collect();
        let mut reattach = Vec::new();
        let mut cluster_x = 0.;
        for (range, chars) in clusters(run, &offsets) {
            let width = widths[chars].iter().sum::<f32>();
            let positions = xs.get(range.clone()).filter(|p| p[0].is_finite());
            let cluster = &mut glyphs[range.clone()];
            // GPOS can attach a glyph that retains its own advance.
            if positions.is_none() && cluster.len() > 1 {
                reattach.push(range);
            }
            if cluster.len() == 1 && positions.is_none() {
                cluster[0].advance = width / density;
            } else {
                place_cluster(cluster, width / density, positions, cluster_x, density);
            }
            cluster_x += width;
        }
        Some(RunAdvances {
            glyphs,
            reattach,
            space: (extras & 1 != 0).then_some(extra[0] / density),
            hyphen: (extras & 2 != 0).then_some(extra[1] / density),
        })
    }
}

/// Visual glyphs and their logical UTF-16 source range. HarfRust's cluster
/// indices increase in LTR and decrease in RTL.
fn clusters<'a>(
    run: &'a AdvanceRequest<'a>,
    offsets: &'a [usize],
) -> impl Iterator<Item = (std::ops::Range<usize>, std::ops::Range<usize>)> + 'a {
    let mut end = 0;
    run.infos
        .chunk_by(|a, b| a.cluster == b.cluster)
        .map(move |group| {
            let start = end;
            end += group.len();
            let next = if run.rtl {
                start.checked_sub(1).map(|i| &run.infos[i])
            } else {
                run.infos.get(end)
            };
            let char_end = next.map_or(offsets.len() - 1, |g| g.cluster as usize);
            (
                start..end,
                offsets[group[0].cluster as usize]..offsets[char_end],
            )
        })
}

/// Android 12 exposes ink origins, including mark and kerning offsets. Recover
/// the pen at each advancing glyph and keep zero-advance marks off the pen.
fn place_cluster(
    glyphs: &mut [GlyphAdvance],
    width: f32,
    xs: Option<&[f32]>,
    origin_px: f32,
    density: f32,
) {
    let total: f32 = glyphs.iter().map(|g| g.advance).sum();
    let last = glyphs
        .iter()
        .rposition(|g| g.advance != 0.)
        .unwrap_or(glyphs.len() - 1);
    let mut pen = 0.;
    for i in 0..glyphs.len() {
        let original = glyphs[i].advance;
        let advance = if i == last {
            width - pen
        } else if original == 0. {
            0.
        } else if let Some(xs) = xs {
            let next = i
                + 1
                + glyphs[i + 1..]
                    .iter()
                    .position(|g| g.advance != 0.)
                    .unwrap();
            (xs[next] - origin_px) / density - glyphs[next].x_offset - pen
        } else if total != 0. {
            width * (original / total)
        } else {
            original
        };
        if let Some(xs) = xs {
            glyphs[i].x_offset = (xs[i] - origin_px) / density - pen;
        }
        glyphs[i].advance = advance;
        pen += advance;
    }
}
