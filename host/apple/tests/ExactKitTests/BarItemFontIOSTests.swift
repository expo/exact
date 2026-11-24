#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// LLP 1141.004 D3: a readout the bar hosts sets what it leaves unsaid as UIKit sets a plain bar item's title.
/// UIKit publishes no such font, so `FieldChromeCache.barItemFont` approximates it. This takes the font the host
/// gives the kernel (control text kind 5), steps its weight under Bold Text as the kernel does, draws the words
/// in the font the text engine makes for that run, and compares the box of its ink with a stock
/// `UIBarButtonItem(title:)`'s at every text size from the smallest to the largest. The width holds the weight:
/// medium and semibold differ by two device pixels or more. The amount of ink does not: UIKit's bar draws its
/// title with more ink than any weight of the same face draws in a label.
final class BarItemFontIOSTests: XCTestCase {
    /// The box around the dark pixels of `image` in `area`, both in points.
    private func ink(_ image: UIImage, in area: CGRect) -> CGRect? {
        guard let cg = image.cgImage, let data = cg.dataProvider?.data, let bytes = CFDataGetBytePtr(data) else { return nil }
        let scale = image.scale, row = cg.bytesPerRow, step = cg.bitsPerPixel / 8
        var box: CGRect?
        for y in max(0, Int(area.minY * scale))..<min(cg.height, Int(area.maxY * scale)) {
            for x in max(0, Int(area.minX * scale))..<min(cg.width, Int(area.maxX * scale)) {
                let p = bytes + y * row + x * step
                guard (Int(p[0]) + Int(p[1]) + Int(p[2])) / 3 < 128, p[3] > 128 else { continue }
                let dot = CGRect(x: CGFloat(x) / scale, y: CGFloat(y) / scale, width: 1 / scale, height: 1 / scale)
                box = box.map { $0.union(dot) } ?? dot
            }
        }
        return box
    }

    /// The kernel's Bold Text step on the system font (LLP 1115.000 D12.3).
    private func bolder(_ weight: Int) -> Int { [100: 200, 200: 300, 300: 400, 400: 600, 500: 700, 600: 800][weight] ?? 900 }

    func testTheBarItemFontDrawsAsUIKitsOwnTitle() throws {
        let sizes: [UIContentSizeCategory] = [.extraSmall, .small, .medium, .large, .extraLarge, .extraExtraLarge,
                                              .extraExtraExtraLarge, .accessibilityMedium, .accessibilityExtraExtraExtraLarge]
        let cache = FieldChromeCache(), engine = TextEngine.pair(resolve: { _ in nil })
        engine.fieldChrome = cache
        Owner.shared.sync { engine.measuring.fieldChrome = cache }
        // Words for the width, capitals for the height: one item at each end of the bar
        let words = ["Updating", "HHHHHH"]
        for category in sizes {
            for bold in [false, true] {
                let root = UIViewController()
                root.view.backgroundColor = .white
                let items = words.map { UIBarButtonItem(title: $0, style: .plain, target: nil, action: nil) }
                if #available(iOS 26.0, *) { items.forEach { $0.hidesSharedBackground = true } }
                (root.navigationItem.leftBarButtonItem, root.navigationItem.rightBarButtonItem) = (items[0], items[1])
                let nav = UINavigationController(rootViewController: root)
                nav.traitOverrides.preferredContentSizeCategory = category
                nav.traitOverrides.legibilityWeight = bold ? .bold : .regular
                nav.navigationBar.tintColor = .black
                let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 300))
                window.rootViewController = nav
                window.makeKeyAndVisible()
                nav.view.layoutIfNeeded()
                // As a session does: the window's traits configure the cache, the body font registers the
                // platform family, and the bar item's font is told in it
                cache.configure(nav.traitCollection)
                _ = Owner.shared.sync { TextEngine.controlText(engine.measuring.opaque, 0) }
                let told = Owner.shared.sync { TextEngine.controlText(engine.measuring.opaque, 5) }
                let weight = bold ? bolder(Int(told.weight)) : Int(told.weight)
                let font = engine.font(size: CGFloat(told.size), weight: weight, family: Int(told.family_id), italic: told.italic != 0)
                let labels = words.enumerated().map { i, text in
                    let label = UILabel()
                    (label.font, label.text, label.textColor, label.backgroundColor) = (font, text, .black, .white)
                    label.sizeToFit()
                    label.frame = CGRect(x: 20, y: 120 + 70 * CGFloat(i), width: label.bounds.width + 20, height: label.bounds.height + 20)
                    window.addSubview(label)
                    return label
                }
                // Both titles and both labels in one capture of the window, each cropped from it
                let bar = nav.navigationBar.convert(nav.navigationBar.bounds, to: window)
                let halves = [CGRect(x: bar.minX, y: bar.minY, width: bar.width / 2, height: bar.height),
                              CGRect(x: bar.midX, y: bar.minY, width: bar.width / 2, height: bar.height)]
                var image = UIImage()
                waitFor("UIKit draws both titles") {
                    image = UIGraphicsImageRenderer(bounds: window.bounds).image { window.layer.render(in: $0.cgContext) }
                    return halves.allSatisfy { ink(image, in: $0) != nil }
                }
                for (i, text) in words.enumerated() {
                    let stock = try XCTUnwrap(ink(image, in: halves[i])), drawn = try XCTUnwrap(ink(image, in: labels[i].frame))
                    let what = "\(category.rawValue)\(bold ? ", Bold Text" : ""), \(text), \(told.size) pt at \(weight)"
                    // As wide within a device pixel and a half on this 3x screen, where medium is a pixel narrower
                    // than UIKit's title and semibold two or more wider; as tall within two
                    XCTAssertEqual(drawn.width, stock.width, accuracy: 0.5, "\(what): as wide as UIKit's title")
                    XCTAssertEqual(drawn.height, stock.height, accuracy: 0.7, "\(what): as tall as UIKit's title")
                }
            }
        }
    }
}
#endif
