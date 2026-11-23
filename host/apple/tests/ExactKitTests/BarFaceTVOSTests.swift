#if os(tvOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

final class BarFaceTVOSTests: XCTestCase {
    private let red = [255, 0, 0, 255]
    private let blue = [0, 0, 255, 255]

    private func bar(dot: [String: Any] = [:], button: [String: Any] = [:], collapsedSibling: Bool = false) throws
        -> (HeaderShape.Item, UIBarButtonItem) {
        let p = Presenter()
        defer { p.reset() }
        let dotStyle: [String: Any] = ["width": 8, "height": 6, "background_color": red, "border_radius_top_left": 3]
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view", "props": ["semanticTag": "header"]],
            ["op": "create", "id": 3, "kind": "button", "props": ["accessibilityLabel": "Record"],
             "handlers": ["press"], "style": button],
            ["op": "create", "id": 4, "kind": "view", "style": dotStyle.merging(dot) { _, value in value }],
        ] + (collapsedSibling ? [
            ["op": "create", "id": 5, "kind": "view", "style": dotStyle.merging(["visibility": "collapse"]) { _, value in value }],
        ] : []) + [
            ["op": "children", "id": 3, "ids": collapsedSibling ? [4, 5] : [4]],
            ["op": "children", "id": 2, "ids": [3]],
            ["op": "children", "id": 1, "ids": [2]],
            ["op": "roots", "ids": [1]],
        ]))
        let route = try XCTUnwrap(p.views[1])
        let shape = try XCTUnwrap(HeaderShape(route: route, back: nil, projected: true))
        let navigation = NavigationHost(presenter: p)
        let face = try XCTUnwrap(shape.trailing.first)
        let item = try XCTUnwrap(navigation.barItems(shape.trailing, RouteController(route)).first)
        return (face, item)
    }

    private func pixel(_ image: CGImage, x: Int? = nil) throws -> [Int] {
        let point = CGRect(x: x ?? image.width / 2, y: image.height / 2, width: 1, height: 1)
        let sample = try XCTUnwrap(image.cropping(to: point))
        let space = try XCTUnwrap(CGColorSpace(name: CGColorSpace.sRGB))
        let context = try XCTUnwrap(CGContext(data: nil, width: 1, height: 1, bitsPerComponent: 8, bytesPerRow: 4,
                                              space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        context.draw(sample, in: CGRect(x: 0, y: 0, width: 1, height: 1))
        let bytes = try XCTUnwrap(context.data).assumingMemoryBound(to: UInt8.self)
        return (0..<4).map { Int(bytes[$0]) }
    }

    func testADotOnlyButtonHasAnImageInTheBar() throws {
        let (_, item) = try bar()
        XCTAssertTrue(item.isEnabled)
        XCTAssertEqual(item.accessibilityLabel, "Record")
        let image = try XCTUnwrap(item.image, "the focusable item shows its dot")
        XCTAssertEqual(image.size, CGSize(width: 8, height: 6))
        XCTAssertEqual(image.renderingMode, .alwaysOriginal)
        XCTAssertEqual(try pixel(XCTUnwrap(image.cgImage)), red)
    }

    func testAnOversizedDotIsDrawnInsideThePixelLimit() throws {
        let (_, item) = try bar(dot: ["width": 640, "height": 320], button: ["padding_left": 160, "padding_right": 160])
        let image = try XCTUnwrap(item.image?.cgImage)
        XCTAssertLessThanOrEqual(max(image.width, image.height), 256)
        XCTAssertEqual(Double(image.width) / Double(image.height), 3, accuracy: 0.05)
        XCTAssertEqual(try pixel(image), red)
        XCTAssertEqual(try pixel(image, x: 0), [0, 0, 0, 0], "scaled padding stays clear")
    }

    func testACollapsedDotDoesNotCountAsShown() throws {
        let (_, visible) = try bar(collapsedSibling: true)
        XCTAssertEqual(try pixel(XCTUnwrap(visible.image?.cgImage)), red)
        let (_, collapsed) = try bar(dot: ["visibility": "collapse"])
        XCTAssertNil(collapsed.image)
    }

    func testADotUsesItsOwnColorScheme() throws {
        let (_, item) = try bar(dot: ["color_scheme": "dark", "background_color": [red, blue]],
                                button: ["color_scheme": "light"])
        XCTAssertEqual(try pixel(XCTUnwrap(item.image?.cgImage)), blue)
    }

    func testADotHasNoActiveTitleColorOrTitleAttributes() throws {
        let (face, item) = try bar(button: ["press_color": blue, "text_color": red, "written": ["text_color"]])
        XCTAssertNotNil(item.image)
        XCTAssertFalse(face.supportsActiveColor)
        XCTAssertNil(face.activeTitleColor)
        XCTAssertNil(item.tintColor)
        XCTAssertNil(item.titleTextAttributes(for: .highlighted))
    }
}
#endif
