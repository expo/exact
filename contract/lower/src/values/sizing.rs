//! The sizing values Chrome parses on a size row that the kernel does not
//! lay out, each refused by name (LLP 1001.001 D1).
use exact_kernel::StyleId;

/// Why a size row refuses `v`, if it is one of the sizing values Chrome
/// parses beyond the three keywords the kernel lays out (LLP 1001.001 D1).
pub(super) fn refusal(row: StyleId, name: &str, v: &str) -> Option<String> {
    let lower = v.to_ascii_lowercase();
    let size = |id: StyleId| id.admits_size_keyword(exact_kernel::SizeKeyword::MinContent);
    if !size(row) {
        return None;
    }
    if let Some(standard) = ["min-content", "max-content", "fit-content"]
        .into_iter()
        .find(|k| lower == format!("-webkit-{k}"))
    {
        return Some(format!(
            "`{name}=\"{v}\"` is a prefixed alias; write `{standard}`"
        ));
    }
    if lower.starts_with("fit-content(") {
        let axis = if name.contains("height") {
            "height"
        } else {
            "width"
        };
        let instead = if row == StyleId::FlexBasis {
            "No combination of rows gives it on a flex item: the function is a base size"
                .to_string()
        } else {
            format!("On a box that does not flex, `{axis}=\"max-content\" min-{axis}=\"min-content\" max-{axis}=\"<length>\"` gives the same size; on a flex item it does not, since the rows bound the size after it grows or shrinks")
        };
        return Some(format!("`{name}=\"{v}\"`: Chrome parses `fit-content()` only in a grid track, so exact2 takes it there and not here. {instead}"));
    }
    if lower == "stretch" || lower == "-webkit-fill-available" {
        return Some(format!("`{name}=\"{v}\"`: `stretch` (CSS Box Sizing 4) is not implemented; the size keywords are `min-content`, `max-content` and `fit-content`"));
    }
    if lower.starts_with("calc-size(") {
        return Some(format!("`{name}=\"{v}\"`: `calc-size()` (CSS Box Sizing 4) is not implemented; the size keywords are `min-content`, `max-content` and `fit-content`"));
    }
    None
}
