// HTML list counters come from the kernel. Keeping the marker on the item's
// layer lets it follow the item's transform without entering its accessibility children.
#if os(macOS)
import AppKit
#else
import UIKit
#endif
import QuartzCore

extension NodeView {
    func layoutProseMarker() {
        guard let label = props["listMarker"], let text else {
            proseMarker?.removeFromSuperlayer()
            proseMarker = nil
            return
        }
        #if os(macOS)
        wantsLayer = true
        guard let owner = layer else { return }
        let scale = window?.backingScaleFactor ?? owner.contentsScale
        #else
        let owner = layer
        let scale = traitCollection.displayScale
        #endif
        let size = number("font_size", PageFacts.defaultRootFontSize)
        let font = text.font(size: size, weight: Int(number("font_weight", 400)),
                             family: Int(number("font_family")), italic: style["font_style"]?.string == "italic",
                             numeric: Int(number("font_variant_numeric")))
        let words = NSAttributedString(string: label, attributes: [.font: font, .foregroundColor: fieldInk])
        let measured = words.size()
        let width = ceil(measured.width), gap = size / 4
        let x = style["direction"]?.string == "rtl" ? bounds.maxX + gap : bounds.minX - width - gap
        let first = structuralChildren.first { !$0.isHidden && $0.style["position_type"]?.string != "absolute" }
        let top = first.map { $0.convert($0.bounds, to: self).minY } ?? bounds.minY
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }
        let marker = proseMarker ?? CATextLayer()
        if proseMarker == nil { owner.addSublayer(marker); proseMarker = marker }
        marker.contentsScale = scale
        marker.isHidden = cssVisibilityHidden
        marker.string = words
        marker.frame = CGRect(x: x, y: top, width: width, height: ceil(measured.height))
    }
}
