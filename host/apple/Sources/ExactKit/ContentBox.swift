// Every box with padding or a border is sent its CSS content box and the
// padding layout resolved (`fieldContent`; a percentage is of the
// containing block, CSS Box Model 3 §4.1). That applied geometry is the one
// source for what is placed in the box, for its insets and for pointer
// offsets: it is held with the frame beneath a sheet and through a failed
// layout, where a style row is not. A box sent none has no inset.
import CoreGraphics
#if os(iOS) || os(tvOS)
import UIKit
#else
import AppKit
#endif

package struct FlightImagePlacement {
    package var image: CGRect
    package var clip: CGRect

    package init(image: CGRect, clip: CGRect) {
        self.image = image; self.clip = clip
    }

    package func fraction(in box: CGRect) -> Self {
        guard box.width > 0, box.height > 0 else { return Self(image: .zero, clip: .zero) }
        return Self(image: RasterGeometry.unitRect(image, in: box), clip: RasterGeometry.unitRect(clip, in: box))
    }
}

extension NodeView {
    /// Native fields keep their published editor box for field layout (LLP
    /// 1104 D5) and read their padding rows. Other content uses the published
    /// box, or the bounds of a box sent none.
    /// The bounds at the size layout gave: a box drawn on the pixel grid
    /// can be up to a device pixel smaller or larger (`PixelGridIOS.swift`,
    /// `PixelGridMac.swift`), and its text wraps at layout's width, not the
    /// drawn one.
    package var layoutBounds: CGRect { CGRect(origin: bounds.origin, size: grid?.layout.size ?? bounds.size) }
    /// The frame layout sent, where the drawn one is on the pixel grid.
    package var laidOutFrame: CGRect { grid?.layout ?? frame }

    package func contentBox() -> CGRect {
        contentBox(in: layoutBounds)
    }

    private func contentBox(in layout: CGRect) -> CGRect {
        if !isNativeTextControl {
            return kernelContentBox?.offsetBy(dx: layout.minX, dy: layout.minY) ?? layout
        }
        let uniform = number("border_width")
        return layout.insetBy(
            left: number("border_width_left", uniform) + number("padding_left"),
            top: number("border_width_top", uniform) + number("padding_top"),
            right: number("border_width_right", uniform) + number("padding_right"),
            bottom: number("border_width_bottom", uniform) + number("padding_bottom"))
    }

    package func flightImagePlacement(natural: CGSize, in box: CGRect) -> FlightImagePlacement {
        let layout = CGRect(origin: box.origin, size: grid?.layout.size ?? box.size)
        let content = contentBox(in: layout)
        let image = natural.width > 0 && natural.height > 0
            ? RasterGeometry.rect(natural: natural, content: content, fit: style["object_fit"]?.string ?? "fill",
                                  position: style["object_position"]?.array)
            : box
        return FlightImagePlacement(image: image, clip: content).fraction(in: box)
    }

    /// A content-box update can move content without changing the node's frame.
    func applyKernelContentBox(_ box: CGRect?, padding: [CGFloat?]?) {
        kernelContentBox = box; kernelPadding = padding
        layoutField(); layoutTextArea(); video?.layout(); layoutContent()
        #if os(iOS) || os(tvOS)
        setNeedsLayout(); setNeedsDisplay()
        #else
        layerPaintCache = nil
        needsLayout = true; needsDisplay = true
        #endif
    }

    /// Clips `ctx` to the content edge's curve, where CSS trims a replaced
    /// element's content (CSS Backgrounds 3 §5.3, "Corner Clipping"): each
    /// radius less the border and padding beside it, so a bordered rounded
    /// image sits inside its border's inner curve.
    func clipToContentEdge(_ ctx: CGContext, _ content: CGRect) {
        let radii = BorderPaint.contentRadii(cornerSizes(in: layoutBounds), outer: layoutBounds, inner: content)
        ctx.addPath(BorderPaint.roundedRect(content, radii, shape: CornerShape(style["corner_shape"])))
        ctx.clip()
    }

    /// Places what draws in the content box at it, as a video is
    /// (`VideoModule.layout`): an `svg`'s scene, or a 2D canvas's bitmap,
    /// clipped to the content edge's curve as the web clips replaced content
    /// (CSS Backgrounds 3 §4.2, §4.3). Laid out with the view, it waits with
    /// its frame and content box wherever those wait.
    func layoutContent() {
        if props["listMarker"] != nil { layoutProseMarker() }
        guard kind == "svg" || kind == "canvas2d", let presenter else { return }
        let box = contentBox()
        if kind == "svg" { presenter.svg?.place(id, in: box); return }
        let radii = BorderPaint.contentRadii(cornerSizes(in: bounds), outer: bounds, inner: box)
        presenter.canvas2d.place(id, in: box, radii: radii) { [weak self] natural in
            guard let self else { return box }
            return IconPath.placed(natural: natural, fit: self.style["object_fit"]?.string ?? "fill",
                                   position: self.style["object_position"]?.array, in: box)
        }
    }

    /// The padding on `side` ("top", "right", "bottom" or "left") in points,
    /// as layout resolved and sent it.
    package func padding(_ side: String) -> CGFloat {
        if isNativeTextControl { return number("padding_" + side) }
        return writtenPadding(side) ?? 0
    }

    /// The padding on `side` as layout sent it, `nil` where none was sent or
    /// a native button's author left the side unsaid.
    func writtenPadding(_ side: String) -> CGFloat? {
        guard let p = kernelPadding else { return nil }
        switch side {
        case "top": return p[0]
        case "right": return p[1]
        case "bottom": return p[2]
        default: return p[3]
        }
    }
}
