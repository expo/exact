// Native control glyphs: SF Symbols, template paths and build-time SVG assets.
#if os(macOS)
import AppKit
#else
import UIKit
#endif
import ExactCore

package enum FaceSymbol {
    /// What a path token starts with; no SF Symbol or role name holds a `:`.
    static let prefix = "icon-path:"

    static let assetPrefix = "icon-asset:"

    package static func assetToken(_ svg: String?, multicolor: Bool) -> String? {
        guard let svg, !svg.isEmpty else { return nil }
        return assetPrefix + (multicolor ? "original|" : "template|") + IconAsset.name(svg)
    }

    package static func isAsset(_ symbol: String) -> Bool { symbol.hasPrefix(assetPrefix) }
    package static func isOriginal(_ symbol: String) -> Bool { symbol.hasPrefix(assetPrefix + "original|") }
    package static func isCustom(_ symbol: String) -> Bool { isPath(symbol) || isAsset(symbol) }

    /// What follows the prefix in an even-odd icon's token (stage 14 C2); a
    /// nonzero icon's token is as it was. No viewBox starts with a letter.
    static let evenOdd = "evenodd|"

    /// The token of an icon's path, viewBox and fill rule (`symbolFillRule`);
    /// nil when there is no path.
    package static func token(path: String?, viewBox: String?, rule: String? = nil) -> String? {
        guard let path, !path.isEmpty else { return nil }
        return prefix + (rule == "evenodd" ? evenOdd : "") + (viewBox ?? "") + "|" + path
    }

    /// A face's SF name, or its path or asset token when the name is empty.
    package static func of(_ json: [String: Any]) -> String? {
        guard let name = json["symbol"] as? String else { return nil }
        guard name.isEmpty else { return name }
        let rows = json["rows"] as? [String: Any]
        let symbol = rows?["symbol"] as? [String: Any]
        if let asset = assetToken(json["symbolAsset"] as? String,
                                  multicolor: json["symbolMulticolor"] as? Bool == true || symbol?["symbol_rendering"] as? String == "multicolor") { return asset }
        return token(path: json["symbolPath"] as? String, viewBox: json["symbolViewBox"] as? String, rule: json["symbolFillRule"] as? String) ?? name
    }

    /// Whether `symbol` is a path's token, not a name.
    package static func isPath(_ symbol: String) -> Bool { symbol.hasPrefix(prefix) }

    /// A face symbol as the agent reports it: the platform's name; for a
    /// path, the icon token's name as its author wrote it (`icon.reply`)
    /// where the face says it, else "" (the token is the face's drawing).
    package static func observed(_ symbol: String?, icon: String? = nil) -> String? {
        symbol.map { isCustom($0) ? icon ?? "" : $0 }
    }

    /// The template image of a path's token, an em square `points` on a
    /// side (the point size the site gives an SF Symbol); nil for a name.
    package static func pathImage(_ symbol: String, points: CGFloat) -> IconImage? {
        guard isPath(symbol) else { return nil }
        var body = symbol.dropFirst(prefix.count)
        let rule: CGPathFillRule = body.hasPrefix(evenOdd) ? .evenOdd : .winding
        if rule == .evenOdd { body = body.dropFirst(evenOdd.count) }
        guard let bar = body.firstIndex(of: "|") else { return nil }
        let side = max(1, points.rounded(.up))
        return IconPath.image(String(body[body.index(after: bar)...]), viewBox: String(body[..<bar]),
                              size: CGSize(width: side, height: side), rule: rule)
    }

    package static func configuration(_ base: SymbolMeasure.Configuration, rendering: String?, tint: PlatformColor) -> SymbolMeasure.Configuration {
        switch rendering {
        case "monochrome": return base.applying(SymbolMeasure.Configuration.preferringMonochrome())
        case "multicolor": return base.applying(SymbolMeasure.Configuration.preferringMulticolor())
        case "hierarchical": return base.applying(SymbolMeasure.Configuration(hierarchicalColor: tint))
        case "palette": return base.applying(SymbolMeasure.Configuration(paletteColors: [tint]))
        default: return base
        }
    }

    #if os(macOS)
    /// The point size of an SF Symbol drawn with no configuration (the body text's).
    package static var defaultPoints: CGFloat { NSFont.systemFontSize }
    /// A toolbar item's glyph: AppKit draws a symbol there at its own size,
    /// an image as it is. Measured (FaceSymbolTests, macOS 27, by a screen
    /// capture): an SF Symbol's inked box there is a 16.7-point symbol's.
    package static let toolbarPoints: CGFloat = 17

    /// A face's image at its site's point size. Raster assets keep their
    /// viewport's aspect and are original only under `multicolor`.
    package static func image(_ symbol: String, points: CGFloat = defaultPoints, accessibilityDescription: String? = nil, bundle: Bundle = .main) -> NSImage? {
        if isAsset(symbol) {
            let name = String(symbol.split(separator: "|").last ?? "")
            guard let image = IconAsset.image(named: name, bundles: IconAsset.bundles(for: bundle)) else { return nil }
            let fit = max(1, points) / max(image.size.width, image.size.height)
            image.size = CGSize(width: image.size.width * fit, height: image.size.height * fit)
            image.isTemplate = !isOriginal(symbol)
            image.accessibilityDescription = accessibilityDescription
            return image
        }
        if isPath(symbol) {
            let image = pathImage(symbol, points: points)
            image?.accessibilityDescription = accessibilityDescription
            return image
        }
        return symbol.isEmpty ? nil : NSImage(systemSymbolName: symbol, accessibilityDescription: accessibilityDescription)
    }

    /// A path's template image drawn in `tint`, as a palette colour draws a
    /// symbol in its own colour (an authored symbol tint on a button face).
    package static func tinted(_ template: NSImage, _ tint: NSColor) -> NSImage {
        let image = NSImage(size: template.size, flipped: false) { rect in
            template.draw(in: rect)
            tint.set()
            rect.fill(using: .sourceIn) // the tint through the path's alpha, its own alpha kept
            return true
        }
        image.isTemplate = false
        image.accessibilityDescription = template.accessibilityDescription
        return image
    }
    #else
    /// The point size of an SF Symbol drawn with no configuration (the body text's).
    package static var defaultPoints: CGFloat { UIFont.preferredFont(forTextStyle: .body).pointSize }
    /// A tab bar item's glyph: UIKit sizes a symbol there itself, and an
    /// image as it is. Measured (FaceSymbolIOSTests and a plain UIKit app,
    /// iOS 27 simulators): an SF Symbol's inked box there is a 23-point
    /// symbol's on an iPhone at every width, stacked or inline (an iPhone 18
    /// Pro Max turned, the iPhone Duo's inner display), and a 17-point one's in
    /// a standalone tab bar given an iPad's idiom and a regular width on an
    /// iPhone, standing in for an iPad's inline items (an iPad itself shows
    /// iPadOS's floating tab bar, not measured here).
    package static func tabBarPoints(_ traits: UITraitCollection) -> CGFloat {
        traits.userInterfaceIdiom == .pad && traits.horizontalSizeClass == .regular ? 17 : 23
    }
    /// A navigation bar item's glyph, measured as the tab bar's: a 22-point
    /// symbol's (UIKit draws bar symbols larger than the body's 17).
    package static let barItemPoints: CGFloat = 22

    /// A face's image at its site's point size, using the catalog rendition
    /// for the display scale. Only `multicolor` assets keep their colours.
    package static func image(_ symbol: String, points: CGFloat = defaultPoints, configuration: UIImage.SymbolConfiguration? = nil, bundle: Bundle = .main, traits: UITraitCollection? = nil) -> UIImage? {
        if isAsset(symbol) {
            let name = String(symbol.split(separator: "|").last ?? "")
            guard let image = IconAsset.image(named: name, bundles: IconAsset.bundles(for: bundle), scale: traits?.displayScale) else { return nil }
            let fit = max(1, points) / max(image.size.width, image.size.height)
            let size = CGSize(width: image.size.width * fit, height: image.size.height * fit)
            // Changing the logical scale keeps the selected catalog pixels for page layout.
            guard let pixels = image.cgImage else { return nil }
            let drawn = UIImage(cgImage: pixels, scale: CGFloat(pixels.width) / size.width, orientation: image.imageOrientation)
            return drawn.withRenderingMode(isOriginal(symbol) ? .alwaysOriginal : .alwaysTemplate)
        }
        if isPath(symbol) { return pathImage(symbol, points: points) }
        return symbol.isEmpty ? nil : UIImage(systemName: symbol, withConfiguration: configuration)
    }
    #endif
}

extension NodeView {
    /// The glyph this image shows on a control: an SF name, template path
    /// or raster asset token; nil when none is available.
    var faceSymbol: String? {
        if let name = props["symbolName"], !name.isEmpty { return name }
        if let asset = FaceSymbol.assetToken(props["symbolAsset"], multicolor: style["symbol_rendering"]?.string == "multicolor") { return asset }
        return FaceSymbol.token(path: props["symbolPath"], viewBox: props["symbolViewBox"], rule: props["symbolFillRule"])
    }
}
