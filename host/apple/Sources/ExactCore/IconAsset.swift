// Build-time SVG renditions shared by page, control and snapshot images.
import Foundation
import CryptoKit
#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif

package enum IconAsset {
    /// The symbol-measure seam carries a source instead of an SF name.
    package static let measurePrefix = "icon-asset:"

    package static func name(_ svg: String) -> String {
        "ExactIcon-" + SHA256.hash(data: Data(svg.utf8)).map { String(format: "%02x", $0) }.joined()
    }

    package static func containingApp(of bundle: Bundle) -> Bundle? {
        var url = bundle.bundleURL
        guard url.pathExtension == "appex" else { return nil }
        #if os(macOS)
        url.deleteLastPathComponent()
        #endif
        return Bundle(url: url.deletingLastPathComponent().deletingLastPathComponent())
    }

    package static func bundles(for bundle: Bundle) -> [Bundle] {
        [bundle] + (containingApp(of: bundle).map { [$0] } ?? [])
    }

    package static func size(_ natural: CGSize, points: CGFloat) -> CGSize {
        let fit = max(0, points) / max(natural.width, natural.height, 1)
        return CGSize(width: natural.width * fit, height: natural.height * fit)
    }

    #if canImport(UIKit)
    package static func image(named name: String, bundles: [Bundle], scale: CGFloat? = nil) -> UIImage? {
        #if os(watchOS)
        return bundles.lazy.compactMap { UIImage(named: name, in: $0, with: nil) }.first
        #else
        let traits = scale.map { UITraitCollection(displayScale: $0) }
        return bundles.lazy.compactMap { UIImage(named: name, in: $0, compatibleWith: traits) }.first
        #endif
    }
    #else
    package static func image(named name: String, bundles: [Bundle], scale: CGFloat? = nil) -> NSImage? {
        bundles.lazy.compactMap { $0.image(forResource: name)?.copy() as? NSImage }.first
    }
    #endif
}
