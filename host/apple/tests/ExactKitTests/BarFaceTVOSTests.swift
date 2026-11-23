#if os(tvOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

final class BarFaceTVOSTests: XCTestCase {
    func testADotOnlyButtonHasAnImageInTheBar() throws {
        let p = Presenter()
        defer { p.reset() }
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view", "props": ["semanticTag": "header"]],
            ["op": "create", "id": 3, "kind": "button", "props": ["accessibilityLabel": "Record"], "handlers": ["press"]],
            ["op": "create", "id": 4, "kind": "view",
             "style": ["width": 8, "height": 6, "background_color": [255, 0, 0, 255], "border_radius_top_left": 3]],
            ["op": "children", "id": 3, "ids": [4]],
            ["op": "children", "id": 2, "ids": [3]],
            ["op": "children", "id": 1, "ids": [2]],
            ["op": "roots", "ids": [1]],
        ]))
        let route = try XCTUnwrap(p.views[1])
        let shape = try XCTUnwrap(HeaderShape(route: route, back: nil, projected: true))
        let navigation = NavigationHost(presenter: p)
        let item = try XCTUnwrap(navigation.barItems(shape.trailing, RouteController(route)).first)
        XCTAssertTrue(item.isEnabled)
        XCTAssertEqual(item.accessibilityLabel, "Record")
        let image = try XCTUnwrap(item.image, "the focusable item shows its dot")
        XCTAssertEqual(image.size, CGSize(width: 8, height: 6))
        XCTAssertEqual(image.renderingMode, .alwaysOriginal)
    }
}
#endif
