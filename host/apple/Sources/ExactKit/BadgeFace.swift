#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// A bar item's face drawn from a button whose one child is a filled box
/// holding a text, a symbol or a picture (an avatar, a badge): CSS's colours and
/// corners at the bar's image size, light and dark, as the platform draws an image that retains its colours.
package struct BadgeFace: Equatable {
    /// A bar item's image size; a title's avatar takes the author's.
    package static let size: CGFloat = 36
    package let size: CGFloat
    let text: String, symbol: String?
    /// The box's picture (an avatar's photo) once it has loaded, and how it
    /// fills the box (`object-fit`). Until then the face is the box's fill,
    /// with its text or symbol if it has one.
    let picture: CGImage?, natural: CGSize, fit: String
    /// The picture's node, loaded or not.
    package let pictureNode: UInt32?
    let light: [[Double]], dark: [[Double]]
    let corners: [CGSize]
    #if !os(macOS)
    private let darkScheme: Bool
    /// The fill's and the ink's share of the tint where each is the tint (`AccentColor` 1,
    /// `Highlight` 0.2), nil otherwise: the parts drawn in UIKit's grey while it dims tints.
    package let shares: [Double?]
    #endif
    package init?(_ button: NodeView) {
        // A child hidden as authored is no part of the face.
        let kids = button.childNodes.filter { $0.style["display"]?.string != "none" }
        guard kids.count == 1, let box = kids.first else { return nil }
        self.init(box: box)
    }
    /// The face of a filled box holding a text, a symbol or a picture (a title's
    /// avatar, §9.10, too). `authored`: draw it at the size the author gave
    /// the box (`faceBoxSize`, the authored points a replaced header has no frame for), between 20
    /// and 44, as a title's avatar is; otherwise at a bar item's 36.
    package init?(box: NodeView, authored: Bool = false) {
        guard box.channels("background_color") != nil else { return nil }
        var text = "", symbol: String?, ink: NodeView?, pictured: NodeView?
        // Shown parts only: a part hidden as authored, or one `visibility`
        // hides (a paragraph keeps a run that computes `visible`), is not drawn.
        func walk(_ node: NodeView) {
            for child in node.childNodes where child.style["display"]?.string != "none"
                && !(child.isParagraph ? !child.accessibilityExposed : child.kind == "image" && child.cssVisibilityHidden) {
                if child.kind == "image", let name = child.faceSymbol { symbol = symbol ?? name; ink = ink ?? child }
                else if child.kind == "image", child.imageSource != nil { pictured = pictured ?? child }
                else if child.isParagraph, text.isEmpty { text = child.accessibleText; ink = ink ?? child }
                else { walk(child) }
            }
        }
        walk(box)
        guard !text.isEmpty || symbol != nil || pictured != nil else { return nil }
        let key = "text_color"
        func colours(_ dark: Bool) -> [[Double]] {
            [box.carriedChannels("background_color", dark: dark) ?? [0, 0, 0, 0], ink?.carriedChannels(key, dark: dark) ?? (dark ? [1, 1, 1, 1] : [0, 0, 0, 1])]
        }
        let given = authored ? box.props["faceBoxSize"].flatMap { s -> CGFloat? in
            let parts = s.split(separator: "x").compactMap { Double($0) }
            return parts.count == 2 ? CGFloat(min(parts[0], parts[1])) : nil
        } : nil
        size = min(max(given ?? Self.size, 20), 44)
        self.text = text; self.symbol = symbol
        pictureNode = pictured?.id
        picture = pictured?.raster?.image.image
        natural = pictured?.raster?.image.naturalSize ?? .zero
        fit = pictured?.style["object_fit"]?.string ?? "fill"
        light = colours(false); dark = colours(true)
        corners = box.cornerSizes(in: CGRect(x: 0, y: 0, width: size, height: size))
        #if !os(macOS)
        darkScheme = box.drawsDark
        shares = [box.style["background_color"]?.tintAlpha, ink?.style[key]?.tintAlpha]
        #endif
    }
    // Joined by hand: interpolating an array goes through reflection, and
    // every batch builds each route's source.
    package var source: String {
        let rgba = { (c: [[Double]]) in c.map { $0.map { String($0) }.joined(separator: ",") }.joined(separator: ";") }
        var source = "\(text)|\(symbol ?? "")|\(picture.map { "\(ObjectIdentifier($0))" } ?? "")|\(fit)|\(rgba(light))|\(rgba(dark))|\(corners.map { "\($0.width)x\($0.height)" }.joined(separator: ","))|\(size)"
        #if !os(macOS)
        source += "|\(darkScheme)"
        #endif
        return source
    }

    #if os(macOS)
    package var image: NSImage {
        NSImage(size: CGSize(width: size, height: size), flipped: true) { rect in
            let dark = NSAppearance.current.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
            let colours = dark ? self.dark : self.light
            guard let context = NSGraphicsContext.current?.cgContext else { return false }
            context.addPath(BorderPaint.roundedRect(rect, self.corners, shape: nil))
            context.setFillColor(TextEngine.color(colours[0]).cgColor)
            context.fillPath()
            if let picture = self.picture {
                context.addPath(BorderPaint.roundedRect(rect, self.corners, shape: nil))
                context.clip()
                NSImage(cgImage: picture, size: self.natural).draw(in: RasterGeometry.rect(natural: self.natural, content: rect, fit: self.fit),
                                                                  from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: nil)
                return true
            }
            let ink = TextEngine.color(colours[1])
            // A palette colours a symbol; a path's template image is drawn through in the ink.
            if let symbol = self.symbol, let glyph = FaceSymbol.image(symbol, points: self.size * 0.42).flatMap({
                FaceSymbol.isOriginal(symbol) ? $0 : FaceSymbol.isCustom(symbol) ? FaceSymbol.tinted($0, ink) : $0.withSymbolConfiguration(NSImage.SymbolConfiguration(paletteColors: [ink]))
            }) {
                glyph.draw(in: CGRect(x: (rect.width - glyph.size.width) / 2, y: (rect.height - glyph.size.height) / 2, width: glyph.size.width, height: glyph.size.height))
            } else {
                let attrs: [NSAttributedString.Key: Any] = [.font: NSFont.systemFont(ofSize: self.size * 0.42, weight: .medium), .foregroundColor: ink]
                let fit = (self.text as NSString).size(withAttributes: attrs)
                (self.text as NSString).draw(at: CGPoint(x: (rect.width - fit.width) / 2, y: (rect.height - fit.height) / 2), withAttributes: attrs)
            }
            return true
        }
    }
    #else
    package var image: UIImage { image(dimmed: false) }
    /// The face drawn; `dimmed`, its tint parts in the grey UIKit dims a tint to, which an
    /// original bitmap does not follow.
    package func image(dimmed: Bool) -> UIImage {
        draw(darkScheme ? dark : light, dimmed: dimmed).withRenderingMode(.alwaysOriginal)
    }
    private func draw(_ c: [[Double]], dimmed: Bool) -> UIImage {
        let rect = CGRect(x: 0, y: 0, width: size, height: size)
        let colour = { (i: Int) -> UIColor in
            let color = TextEngine.color(c[i])
            guard dimmed, let share = shares[i] else { return color }
            return color.dimmedTint(share: share)
        }
        return UIGraphicsImageRenderer(size: rect.size).image { _ in
            let shape = UIBezierPath(cgPath: BorderPaint.roundedRect(rect, corners, shape: nil))
            colour(0).setFill()
            shape.fill()
            if let picture {
                shape.addClip()
                UIImage(cgImage: picture).draw(in: RasterGeometry.rect(natural: natural, content: rect, fit: fit))
                return
            }
            let ink = colour(1)
            if let symbol, let source = FaceSymbol.image(symbol, points: size * 0.42, configuration: UIImage.SymbolConfiguration(pointSize: size * 0.42)) {
                let glyph = FaceSymbol.isOriginal(symbol) ? source : source.withTintColor(ink, renderingMode: .alwaysOriginal)
                glyph.draw(at: CGPoint(x: (rect.width - glyph.size.width) / 2, y: (rect.height - glyph.size.height) / 2))
            } else {
                let attrs: [NSAttributedString.Key: Any] = [.font: UIFont.systemFont(ofSize: size * 0.42, weight: .medium), .foregroundColor: ink]
                let s = (text as NSString).size(withAttributes: attrs)
                (text as NSString).draw(at: CGPoint(x: (rect.width - s.width) / 2, y: (rect.height - s.height) / 2), withAttributes: attrs)
            }
        }
    }
    #endif

}

#if !os(macOS)
extension UIColor {
    /// The grey UIKit dims this tint to behind a modal (`tintAdjustmentMode = .dimmed`), at a tint
    /// role's `share` of it (`Highlight` 0.2). UIKit chooses it, including for a node's own accent.
    package func dimmedTint(share: Double) -> UIColor {
        let probe = UIView(); probe.tintColor = self; probe.tintAdjustmentMode = .dimmed
        return share < 1 ? probe.tintColor.withAlphaComponent(share) : probe.tintColor
    }
}
#endif
