//! Per-line geometry for the Android text specimen. The label and the probe
//! use the same painted paragraph builder and its visual-order glyph records.
use exact_kernel::StyleProps;
use exact_painted::canvas::{field, jni_sys::*};
use exact_painted::{paint::text_spec, text::TextEngine};
use std::cell::RefCell;

thread_local! { static ENGINE: RefCell<TextEngine> = RefCell::new(TextEngine::new()); }

/// Install the specimen face in this thread's probe engine. The platform
/// reference reads the same file from the APK's extracted assets.
#[no_mangle]
unsafe extern "system" fn Java_exact_app_TextReference_clusterFont(
    env: *mut JNIEnv,
    _: jclass,
    assets: jstring,
) -> jint {
    use exact_plan::{
        builder::{Face, PlanBuilder},
        StackMemberKind,
    };
    let mut plan = PlanBuilder::new(0, 0);
    let face = Face {
        source: "assets/ClusterProbe.ttf",
        weight: [400; 2],
        stretch: [1000; 2],
        italic: false,
    };
    let family = plan.font_family("Cluster Probe", &[face]);
    let stack = plan.font_stack(&[(StackMemberKind::Family, Some(family))]);
    let assets = field::text(env, assets);
    ENGINE
        .with_borrow_mut(|engine| engine.install_plan(plan.plan(), std::path::Path::new(&assets)));
    stack.0 as jint
}

#[no_mangle]
unsafe extern "system" fn Java_exact_app_TextReference_lines(
    env: *mut JNIEnv,
    _: jclass,
    text: jstring,
    size: jfloat,
    width: jfloat,
    scale: jfloat,
    family: jint,
    tabular: jboolean,
    glyphs: jboolean,
) -> jfloatArray {
    let text = field::text(env, text);
    let mut spec = text_spec(
        &StyleProps::default().with_rows(|style| {
            style.font_size = size;
            style.font_family = family as u16;
        }),
        &text,
    );
    spec.runs[0].font_variant_numeric = u8::from(tabular != 0);
    let data = ENGINE.with_borrow_mut(|engine| {
        let paragraph = engine.paragraph(&spec, (width >= 0.).then_some(width / scale));
        let lines = paragraph.lines();
        let mut out = Vec::new();
        for line in &lines.lines {
            if glyphs != 0 {
                for g in lines.glyphs_of(line) {
                    out.extend([
                        g.glyph_id as f32,
                        g.x * scale,
                        g.w * scale,
                        text[..g.start as usize].encode_utf16().count() as f32,
                    ]);
                }
                continue;
            }
            let end = lines
                .glyphs_of(line)
                .iter()
                .map(|g| g.end)
                .max()
                .unwrap_or(0);
            let utf16 = text[..end as usize].encode_utf16().count();
            out.extend([line.w * scale, utf16 as f32]);
        }
        out
    });
    let array = ((**env).NewFloatArray.unwrap())(env, data.len() as i32);
    if !array.is_null() {
        ((**env).SetFloatArrayRegion.unwrap())(env, array, 0, data.len() as i32, data.as_ptr());
    }
    array
}

/// Cold text, warm fonts. A different edit each iteration misses the paragraph
/// cache. Reading the lines includes the glyph placement a paint consumes.
#[no_mangle]
unsafe extern "system" fn Java_exact_app_TextReference_document(
    _: *mut JNIEnv,
    _: jclass,
    revision: jint,
) -> jlong {
    let mut document =
        "A paragraph has words, numbers 123.45 and pairs like AVA To Wa. ".repeat(160);
    document.push_str(&revision.to_string());
    let spec = text_spec(
        &StyleProps::default().with_rows(|style| style.font_size = 16.),
        &document,
    );
    ENGINE.with_borrow_mut(|engine| {
        let start = std::time::Instant::now();
        let paragraph = engine.paragraph(&spec, Some(320.));
        std::hint::black_box(paragraph.lines().glyphs.len());
        start.elapsed().as_nanos() as jlong
    })
}
