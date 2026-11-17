//! An image's tint: the colour a raster is drawn in when it is a template.

use super::Presented;

// Only an explicit row makes a raster a template; motion may supply its ink.
// `currentcolor` is the node's `color` (feed F1).
pub(super) fn image_tint(
    node: &exact_kernel::NodeRef<'_>,
    presented: &Presented,
    dark: bool,
) -> Option<[u8; 4]> {
    // Icon lowering supplies a currentcolor tint for symbol sources. A
    // non-template icon keeps the file's paint, as on the web.
    if node
        .props
        .str(exact_kernel::PropId::ImageSource)
        .and_then(crate::image::file_icon)
        .is_some()
    {
        return None;
    }
    let style = node.style;
    style.mask.has(exact_kernel::StyleId::TintColor).then(|| {
        presented
            .colors
            .color(exact_motion::Property::TintColor)
            .unwrap_or_else(|| {
                super::rgba(
                    style
                        .tint_color
                        .unwrap_or_else(|| node.text_color())
                        .resolve(dark),
                )
            })
    })
}
