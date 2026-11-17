#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

final class TintEdgesIOSTests: XCTestCase {
    private var window: UIWindow!
    private var controller: UIViewController!
    private var session: ExactSession!
    private let tint: [String: Any] = ["sys": "@tint", "c": [0, 122, 255, 255]]
    private let red = UIColor(red: 1, green: 0, blue: 0, alpha: 1)
    private let green = UIColor(red: 0, green: 1, blue: 0, alpha: 1)

    override func tearDown() {
        session?.destroy(); session = nil
        window?.isHidden = true; window = nil; controller = nil
        super.tearDown()
    }

    private func mount() -> Presenter {
        session = ExactApp.shared.makeSession(label: "tint-edges-\(UUID().uuidString)")
        let scene = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first
        window = scene.map(UIWindow.init(windowScene:)) ?? UIWindow()
        window.frame = CGRect(x: 0, y: 0, width: 400, height: 600)
        window.overrideUserInterfaceStyle = .light
        window.tintColor = .blue
        controller = UIViewController()
        window.rootViewController = controller
        let p = session.presenter
        p.viewport.frame = window.bounds
        controller.view.addSubview(p.viewport)
        window.makeKeyAndVisible()
        return p
    }

    private func alert(_ body: () throws -> Void) rethrows {
        let alert = UIAlertController(title: "Pause", message: nil, preferredStyle: .alert)
        alert.addAction(UIAlertAction(title: "OK", style: .default))
        var shown = false
        controller.present(alert, animated: false) { shown = true }
        waitFor("the alert is presented") { shown }
        try body()
        var dismissed = false
        controller.dismiss(animated: false) { dismissed = true }
        waitFor("the alert is dismissed") { dismissed }
    }

    private func pixel(_ view: UIView, x: Double = 0.5) throws -> [Double] {
        view.layoutIfNeeded()
        let w = Int(ceil(view.bounds.width)), h = Int(ceil(view.bounds.height))
        var bytes = [UInt8](repeating: 0, count: w * h * 4)
        let context = try XCTUnwrap(CGContext(data: &bytes, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4,
            space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        view.layer.render(in: context)
        let at = ((h / 2) * w + Int(Double(w) * x)) * 4
        let alpha = Double(bytes[at + 3])
        XCTAssertGreaterThan(alpha, 0, "the image draws at its centre")
        return (0..<3).map { Double(bytes[at + $0]) * 255 / max(1, alpha) } + [alpha]
    }

    private func assertColor(_ got: [Double], _ expected: UIColor, alpha: Double = 255,
                             file: StaticString = #filePath, line: UInt = #line) {
        var r: CGFloat = 0, g: CGFloat = 0, b: CGFloat = 0, a: CGFloat = 0
        XCTAssertTrue(expected.getRed(&r, green: &g, blue: &b, alpha: &a), file: file, line: line)
        for (value, want) in zip(got, [Double(r) * 255, Double(g) * 255, Double(b) * 255, alpha]) {
            XCTAssertEqual(value, want, accuracy: 3, file: file, line: line)
        }
    }

    // An original image retains a tint-derived colour, resolved in its image view's traits.
    // This is the UIKit oracle for the page symbol and readout below.
    func testOriginalImagesResolveTintDerivedInkWithoutForcingNormal() throws {
        guard !UIApplication.shared.connectedScenes.isEmpty else { throw missing("scene", "an alert needs a scene window") }
        _ = mount()
        let parent = UIView(frame: CGRect(x: 0, y: 0, width: 100, height: 100))
        parent.tintColor = red
        controller.view.addSubview(parent)
        let glyph = try XCTUnwrap(UIImage(systemName: "square.fill"))
        let images = [UIColor.tintColor, UIColor.tintColor.withAlphaComponent(0.2), green].enumerated().map { i, ink in
            let view = UIImageView(image: glyph.withTintColor(ink, renderingMode: .alwaysOriginal))
            view.frame = CGRect(x: i * 30, y: 0, width: 24, height: 24)
            parent.addSubview(view)
            return view
        }
        func check() throws {
            let current = parent.tintColor.resolvedColor(with: parent.traitCollection)
            assertColor(try pixel(images[0]), current, alpha: Double(current.cgColor.alpha) * 255)
            assertColor(try pixel(images[1]), current, alpha: 51)
            assertColor(try pixel(images[2]), green)
            for image in images { XCTAssertEqual(image.tintAdjustmentMode, parent.tintAdjustmentMode) }
        }
        try check()
        try alert { XCTAssertEqual(parent.tintAdjustmentMode, .dimmed); try check() }
        try check()
    }

    func testButtonFaceColoursUseTheThemeAndEmbedderTint() throws {
        guard !UIApplication.shared.connectedScenes.isEmpty else { throw missing("scene", "an alert needs a scene window") }
        let p = mount()
        var face = ButtonFace()
        face.title = "Go"; face.subtitle = "Now"; face.symbol = "square.fill"
        face.style = "gray"; face.ios = "gray"; face.iosBefore26 = "gray"
        face.rows.title = ["text_color": .object(["sys": .string("@tint"), "c": .array([0, 122, 255, 255].map { .number(Double($0)) })])]
        face.rows.subtitle = face.rows.title
        face.rows.symbol = ["tint_color": face.rows.title["text_color"]!]
        p.buttonFace = { _ in face }
        p.viewport.tintColor = green
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "control", "props": ["type": "button", "disabled": "true"], "style": ["appearance": "auto"]],
            ["op": "frame", "id": 1, "x": 0, "y": 100, "w": 160, "h": 70], ["op": "roots", "ids": [1]],
        ]))
        func check(_ ink: UIColor) throws {
            p.controls.sync()
            let button = try XCTUnwrap(p.controls.controls[1] as? NativeButtonIOS)
            let config = try XCTUnwrap(button.configuration)
            let foreground = try XCTUnwrap(config.baseForegroundColor).resolvedColor(with: button.traitCollection)
            var r: CGFloat = 0, g: CGFloat = 0, b: CGFloat = 0, a: CGFloat = 0
            XCTAssertTrue(foreground.getRed(&r, green: &g, blue: &b, alpha: &a))
            assertColor([r, g, b, a].map { Double($0) * 255 }, ink)
            let image = UIImageView(image: config.image); image.frame.size = CGSize(width: 24, height: 24)
            button.addSubview(image)
            defer { image.removeFromSuperview() }
            assertColor(try pixel(image), ink)
        }
        try check(green)
        try alert {
            p.apply(wireBatch([["op": "props", "id": 1, "set": ["accessibilityLabel": "Go now"]]]))
            try check(green)
        }
        try check(green)
        p.viewport.tintColor = nil
        session.apply(wireBatch([["op": "roles", "roles": ["AccentColor": [255, 0, 0, 255]]]]))
        try check(red)
        try alert { try check(red) }
        try check(red)
    }

    func testGradientTintStopsDimAndRecoverWithoutChangingExplicitStops() throws {
        guard !UIApplication.shared.connectedScenes.isEmpty else { throw missing("scene", "an alert needs a scene window") }
        let p = mount()
        p.viewport.tintColor = red
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "style": ["background_image": [
                "tint": "linear-gradient(to right, AccentColor 0%, AccentColor 50%, #00ff00 50%, #00ff00 100%)", "sys": "@tint",
                "linear": 90, "stops": [0, 0, 0, 255, 255, 1, 0, 0, 255, 255]]]],
            ["op": "frame", "id": 1, "x": 0, "y": 150, "w": 100, "h": 50], ["op": "roots", "ids": [1]],
        ]))
        let node = try XCTUnwrap(p.views[1])
        func check() throws {
            node.layer.displayIfNeeded()
            let current = node.tintAdjustmentMode == .dimmed ? dimmed(red) : red
            let stops = try XCTUnwrap(Gradient(node.style["background_image"], tint: node.gradientTint)).stops(dark: false)
            XCTAssertGreaterThanOrEqual(stops.0.count, 4)
            for (at, color) in zip(stops.0, stops.1) {
                if at < 0.5, let c = color.components {
                    assertColor(c.map { Double($0) * 255 }, current, alpha: Double(current.cgColor.alpha) * 255)
                }
            }
            let reference = UIView(frame: node.bounds), ramp = CAGradientLayer()
            ramp.frame = reference.bounds
            ramp.startPoint = CGPoint(x: 0, y: 0.5); ramp.endPoint = CGPoint(x: 1, y: 0.5)
            ramp.locations = [0, 0.5, 0.5, 1]
            ramp.colors = [current.cgColor, current.cgColor, green.cgColor, green.cgColor]
            reference.layer.addSublayer(ramp)
            for x in [0.25, 0.75] {
                for (got, want) in zip(try pixel(node, x: x), try pixel(reference, x: x)) {
                    XCTAssertEqual(got, want, accuracy: 3, "the gradient matches UIKit at \(x), adjustment \(node.tintAdjustmentMode.rawValue)")
                }
            }
        }
        try check()
        try alert { XCTAssertEqual(node.tintAdjustmentMode, .dimmed); try check() }
        try check()
    }

    func testBarFacesDimOnlyTheirTintPartsAndRecover() throws {
        guard !UIApplication.shared.connectedScenes.isEmpty else { throw missing("scene", "an alert needs a scene window") }
        let p = mount()
        p.viewport.tintColor = red
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "button", "handlers": ["press"]],
            ["op": "create", "id": 3, "kind": "view", "style": ["background_color": tint, "width": 24, "height": 24]],
            ["op": "create", "id": 4, "kind": "view", "style": ["background_color": [0, 255, 0, 255], "width": 24, "height": 24]],
            ["op": "create", "id": 5, "kind": "button", "handlers": ["press"]],
            ["op": "create", "id": 6, "kind": "text", "props": ["text": "Go"],
             "style": ["font_size": 24, "written": ["font_size"]]],
            ["op": "create", "id": 7, "kind": "image", "props": ["imageSource": "symbol:sf/square.fill", "symbolName": "square.fill"],
             "style": ["font_size": 24]],
            ["op": "children", "id": 5, "ids": [6, 7]],
            ["op": "children", "id": 2, "ids": [3, 4]], ["op": "children", "id": 1, "ids": [2, 5]], ["op": "roots", "ids": [1]],
        ]))
        let route = RouteController(try XCTUnwrap(p.views[1]))
        let nav = p.navigation.makeNavigation(first: nil)
        nav.setViewControllers([route], animated: false)
        let item = try XCTUnwrap(p.navigation.barItems([HeaderShape.Item(try XCTUnwrap(p.views[2]))], route).first)
        let uncoloured = try XCTUnwrap(p.navigation.barItems([HeaderShape.Item(try XCTUnwrap(p.views[5]))], route).first)
        route.navigationItem.rightBarButtonItems = [item, uncoloured]
        window.rootViewController = nav
        controller = route
        nav.view.layoutIfNeeded()
        func check() throws {
            let image = UIImageView(image: try XCTUnwrap(item.image)); image.frame.size = CGSize(width: 48, height: 24)
            let current = nav.navigationBar.tintAdjustmentMode == .dimmed ? dimmed(red) : red
            assertColor(try pixel(image, x: 0.25), current, alpha: Double(current.cgColor.alpha) * 255)
            assertColor(try pixel(image, x: 0.75), green)
            let plain = UIImageView(image: try XCTUnwrap(uncoloured.image))
            var ink = current
            if #available(iOS 26.0, *) { ink = UIColor.label.resolvedColor(with: nav.traitCollection) }
            assertColor(try symbolInkChannels(plain, original: true), ink, alpha: Double(ink.cgColor.alpha) * 255)
        }
        try check()
        try alert { XCTAssertEqual(nav.navigationBar.tintAdjustmentMode, .dimmed); try check() }
        try check()
    }

    // A theme's `AccentColor = "color(display-p3 1 0 0)"` is the view's tint; the stop keeps it
    // unclipped, as the session's CSS beside it paints.
    func testGradientTintStopsKeepAWideAccent() throws {
        guard !UIApplication.shared.connectedScenes.isEmpty else { throw missing("scene", "an alert needs a scene window") }
        let p = mount()
        let wide = UIColor(displayP3Red: 1, green: 0, blue: 0, alpha: 1)
        p.viewport.tintColor = wide
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "style": ["background_image": [
                "css": "linear-gradient(90deg, color(display-p3 1 0 0) 0px, color(display-p3 1 0 0) 100px)",
                "tint": "linear-gradient(90deg in oklab, AccentColor 0px, AccentColor 100px)", "sys": "@tint"]]],
            // The session's colours alone, as a snapshot paints them: the oracle for the undimmed accent.
            ["op": "create", "id": 2, "kind": "view", "style": ["background_image": [
                "css": "linear-gradient(90deg, color(display-p3 1 0 0) 0px, color(display-p3 1 0 0) 100px)"]]],
            ["op": "frame", "id": 1, "x": 0, "y": 150, "w": 100, "h": 50],
            ["op": "frame", "id": 2, "x": 0, "y": 250, "w": 100, "h": 50], ["op": "roots", "ids": [1, 2]],
        ]))
        let node = try XCTUnwrap(p.views[1]), session = try XCTUnwrap(p.views[2])
        let space = try XCTUnwrap(CGColorSpace(name: CGColorSpace.extendedSRGB))
        func check() throws {
            node.layer.displayIfNeeded()
            let current = node.tintAdjustmentMode == .dimmed ? dimmed(wide) : wide
            let want = try XCTUnwrap(current.cgColor.converted(to: space, intent: .defaultIntent, options: nil)?.components)
            let stops = try XCTUnwrap(Gradient(node.style["background_image"], tint: node.gradientTint)).stops(dark: false)
            XCTAssertFalse(stops.1.isEmpty)
            for color in stops.1 {
                let got = try XCTUnwrap(color.converted(to: space, intent: .defaultIntent, options: nil)?.components)
                for (g, w) in zip(got, want) { XCTAssertEqual(Double(g), Double(w), accuracy: 0.01, "\(got) against \(want)") }
            }
            guard node.tintAdjustmentMode != .dimmed else { return }
            session.layer.displayIfNeeded()
            for (got, want) in zip(try p3Pixel(node), try p3Pixel(session)) {
                XCTAssertEqual(Double(got), Double(want), accuracy: 2, "the tint paints as the session's colours do")
            }
        }
        try check()
        XCTAssertGreaterThan(try XCTUnwrap(Gradient(node.style["background_image"], tint: node.gradientTint)?.stops(dark: false).1.first?
            .converted(to: space, intent: .defaultIntent, options: nil)?.components?.first), 1.05, "outside sRGB")
        try alert { XCTAssertEqual(node.tintAdjustmentMode, .dimmed); try check() }
        try check()
    }

    /// The centre pixel in Display P3, 8 bits a channel.
    private func p3Pixel(_ view: UIView) throws -> [UInt8] {
        view.layoutIfNeeded()
        let w = Int(ceil(view.bounds.width)), h = Int(ceil(view.bounds.height))
        var bytes = [UInt8](repeating: 0, count: w * h * 4)
        let context = try XCTUnwrap(CGContext(data: &bytes, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4,
            space: CGColorSpace(name: CGColorSpace.displayP3)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        view.layer.render(in: context)
        let at = ((h / 2) * w + w / 2) * 4
        return Array(bytes[at..<at + 4])
    }

    // A button whose one box is filled in the tint is a badge face, baked into its item's image.
    func testBarBadgeFacesDimTheirTintAndRecover() throws {
        guard !UIApplication.shared.connectedScenes.isEmpty else { throw missing("scene", "an alert needs a scene window") }
        let p = mount()
        p.viewport.tintColor = red
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "button", "handlers": ["press"]],
            ["op": "create", "id": 3, "kind": "view", "style": ["background_color": tint]],
            ["op": "create", "id": 4, "kind": "text", "props": ["text": "A"], "style": ["text_color": [0, 255, 0, 255]]],
            ["op": "children", "id": 3, "ids": [4]], ["op": "children", "id": 2, "ids": [3]],
            ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
        ]))
        let route = RouteController(try XCTUnwrap(p.views[1]))
        let nav = p.navigation.makeNavigation(first: nil)
        nav.setViewControllers([route], animated: false)
        let shape = HeaderShape.Item(try XCTUnwrap(p.views[2]))
        XCTAssertNotNil(shape.badge, "the box is the item's badge")
        let item = try XCTUnwrap(p.navigation.barItems([shape], route).first)
        route.navigationItem.rightBarButtonItems = [item]
        window.rootViewController = nav
        controller = route
        nav.view.layoutIfNeeded()
        func check(_ item: UIBarButtonItem) throws {
            let image = UIImageView(image: try XCTUnwrap(item.image)); image.frame.size = CGSize(width: 36, height: 36)
            let current = nav.navigationBar.tintAdjustmentMode == .dimmed ? dimmed(red) : red
            assertColor(try pixel(image, x: 0.1), current, alpha: Double(current.cgColor.alpha) * 255)
        }
        try check(item)
        var made = item
        try alert {
            XCTAssertEqual(nav.navigationBar.tintAdjustmentMode, .dimmed)
            try check(item)
            // Made while the tint is dimmed: drawn dimmed from the first.
            route.barItemsMade[2] = nil
            made = try XCTUnwrap(p.navigation.barItems([shape], route).first)
            XCTAssertFalse(made === item)
            route.navigationItem.rightBarButtonItems = [made]
            try check(made)
        }
        try check(made)
    }

    private func dimmed(_ color: UIColor) -> UIColor {
        let view = UIView(); view.tintColor = color; view.tintAdjustmentMode = .dimmed
        return view.tintColor
    }

    func testPathInkSurvivesLayoutAtItsNaturalAndFittedSizes() throws {
        guard !UIApplication.shared.connectedScenes.isEmpty else { throw missing("scene", "an alert needs a scene window") }
        let p = mount()
        p.viewport.tintColor = red
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "image", "props": ["imageSource": "symbol:icon", "symbolName": "",
                "symbolPath": "M 2 2 L 22 2 L 22 22 L 2 22 Z", "symbolViewBox": "0 0 24 24"],
             "style": ["font_size": 24, "text_color": [0, 255, 0, 255], "object_fit": "none"]],
            ["op": "create", "id": 2, "kind": "image", "props": ["imageSource": "symbol:icon", "symbolName": "",
                "symbolPath": "M 2 2 L 22 2 L 22 22 L 2 22 Z", "symbolViewBox": "0 0 24 24"],
             "style": ["font_size": 24, "text_color": ["sys": "@tint/0.2", "c": [255, 0, 0, 51]]]],
            ["op": "frame", "id": 1, "x": 0, "y": 100, "w": 44, "h": 44],
            ["op": "frame", "id": 2, "x": 100, "y": 100, "w": 24, "h": 24], ["op": "roots", "ids": [1, 2]],
        ]))
        let node = try XCTUnwrap(p.views[1]), image = try XCTUnwrap(node.symbolView)
        func check() throws {
            let share = try XCTUnwrap(p.views[2]?.symbolView)
            let current = node.tintAdjustmentMode == .dimmed ? dimmed(red) : red
            assertColor(try symbolInkChannels(share, original: true), current, alpha: 51)
            for fit in ["none", "contain", "cover", "scale-down"] {
                p.apply(wireBatch([["op": "style", "id": 1, "style": ["font_size": 24,
                    "text_color": [0, 255, 0, 255], "object_fit": fit]]]))
                for side in [44, 64, 24] {
                    p.apply(wireBatch([["op": "frame", "id": 1, "x": 0, "y": 100, "w": side, "h": side]]))
                    node.layoutSymbol()
                    assertColor(try pixel(image), green)
                    XCTAssertEqual(image.image?.renderingMode, .alwaysOriginal)
                    XCTAssertEqual(image.tintAdjustmentMode, node.tintAdjustmentMode)
                }
            }
        }
        try check()
        try alert { XCTAssertEqual(node.tintAdjustmentMode, .dimmed); try check() }
        try check()
    }

    func testResolvedConicPlacementIncludesTheGradientBoxOrigin() throws {
        let gradient = try XCTUnwrap(Gradient(["conic": [0, 50, 0, 50, 0], "arc": [50, 50, 0],
            "stops": [0, 255, 0, 0, 255, 1, 255, 255, 255, 255]] as BatchValue))
        XCTAssertEqual(gradient.placement(in: CGRect(x: 10, y: 10, width: 100, height: 100)), .conic(CGPoint(x: 60, y: 60), 0))
    }

    func testExplicitSymbolInkLeavesTintAdjustmentToItsOwner() throws {
        guard !UIApplication.shared.connectedScenes.isEmpty else { throw missing("scene", "an alert needs a scene window") }
        let p = mount()
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "image", "props": ["imageSource": "symbol:sf/square.fill", "symbolName": "square.fill"],
             "style": ["font_size": 24, "text_color": [0, 255, 0, 255], "written": ["text_color"], "tint_adjustment": "dimmed"]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 24, "h": 24], ["op": "roots", "ids": [1]],
        ]))
        let node = try XCTUnwrap(p.views[1]), image = try XCTUnwrap(node.symbolView)
        let face = try XCTUnwrap(ChipFace(node, readout: true))
        let readout = ReadoutView(); readout.show(face, ReadoutFacts())
        controller.view.addSubview(readout)
        readout.tintAdjustmentMode = .dimmed
        let readoutImage = try XCTUnwrap(readout.stack.arrangedSubviews.first as? UIImageView)
        readoutImage.frame.size = CGSize(width: 24, height: 24)
        func check() throws {
            readout.frame.size = readout.intrinsicContentSize
            readout.layoutIfNeeded()
            for view in [image, readoutImage] {
                view.frame.size = CGSize(width: 24, height: 24)
                XCTAssertEqual(view.tintAdjustmentMode, .dimmed, "explicit ink must not override the owner's mode")
                assertColor(try pixel(view), green)
            }
        }
        try check()
        try alert { try check() }
        try check()
    }
}
#endif
