//! Read-only, ephemeral performance facts from an opted-in GPU canvas.
use exact_plan::{Plan, TypeKind, TypesId};

/// Runner-owned source, taking one literal surface name.
pub const SOURCE: &str = "exactCanvasPerformance";
/// Maximum UTF-8 snapshot size; the GPU module applies the same bound.
pub const MAX_BYTES: usize = 16 * 1024;
/// Optional numeric metrics. Durations are milliseconds; memory is bytes.
pub const NUMBERS: &[&str] = &[
    "pixelWidth",
    "pixelHeight",
    "renderScale",
    "devicePixelRatio",
    "effectivePixelRatio",
    "samples",
    "draws",
    "triangles",
    "instances",
    "frames",
    "fps",
    "windowMs",
    "lateFrames",
    "missedFrames",
    "longestGapMs",
    "cpuTickMs",
    "cpuFeedMs",
    "cpuEncodeMs",
    "cpuSubmitMs",
    "gpuMs",
    "cpuTickMaxMs",
    "cpuFeedMaxMs",
    "cpuEncodeMaxMs",
    "cpuSubmitMaxMs",
    "gpuMaxMs",
    "processMemoryBytes",
    "gameHeapBytes",
    "textureMemoryBytes",
    "geometryMemoryBytes",
    "maxFps",
];
/// Optional textual platform or renderer facts.
pub const STRINGS: &[&str] = &["audioState", "thermalState", "quality"];

/// Refuse unknown fields and nonoptional metrics, rather than inventing zeroes.
pub fn validate(plan: &Plan, ty: TypesId) -> Result<(), String> {
    let record = plan.type_(ty);
    if record.kind != TypeKind::Record {
        return Err(format!("{SOURCE} answers a record"));
    }
    for f in record.fields.iter() {
        let field = plan.field(f);
        let name = plan.str(field.name);
        let expected = if NUMBERS.contains(&name) {
            TypeKind::Number
        } else if STRINGS.contains(&name) {
            TypeKind::String
        } else if name == "idle" {
            TypeKind::Bool
        } else {
            return Err(format!("{SOURCE} has no field `{name}`"));
        };
        let ty = plan.type_(field.ty);
        if ty.kind != TypeKind::Option || ty.elem.is_none_or(|t| plan.type_(t).kind != expected) {
            return Err(format!(
                "{SOURCE}.{name} must be an option of {expected:?}; unavailable metrics are none"
            ));
        }
    }
    Ok(())
}
