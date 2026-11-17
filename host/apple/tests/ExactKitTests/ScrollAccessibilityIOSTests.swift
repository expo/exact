#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

final class ScrollAccessibilityIOSTests: XCTestCase {
    func testIndicatorsFollowTheRangeOnEachAxis() {
        for scroll in [ScrollView(), Viewport()] {
            scroll.contentInsetAdjustmentBehavior = .never
            scroll.frame = CGRect(x: 0, y: 0, width: 200, height: 300)
            func indicators(_ size: CGSize, x: Bool, y: Bool) {
                scroll.contentSize = size
                scroll.layoutIfNeeded()
                XCTAssertEqual(scroll.showsHorizontalScrollIndicator, x)
                XCTAssertEqual(scroll.showsVerticalScrollIndicator, y)
            }
            indicators(CGSize(width: 200, height: 300), x: false, y: false)
            indicators(CGSize(width: 201, height: 301), x: true, y: true)
            scroll.scrollsX = false
            indicators(CGSize(width: 400, height: 600), x: false, y: true)
            scroll.scrollsX = true
            scroll.scrollsY = false
            indicators(CGSize(width: 400, height: 600), x: true, y: false)
            scroll.scrollsY = true
            scroll.contentInset = UIEdgeInsets(top: 10, left: 10, bottom: 10, right: 10)
            indicators(CGSize(width: 180, height: 280), x: false, y: false)
            indicators(CGSize(width: 181, height: 281), x: true, y: true)
            scroll.bounds.size = CGSize(width: 400, height: 600)
            scroll.layoutIfNeeded()
            XCTAssertFalse(scroll.showsHorizontalScrollIndicator)
            XCTAssertFalse(scroll.showsVerticalScrollIndicator)
        }
    }

    func testHiddenIndicatorsStayHiddenAsContentGrows() throws {
        let p = Presenter(), node = NodeView(id: 1, kind: "view", presenter: p)
        node.frame = CGRect(x: 0, y: 0, width: 200, height: 300)
        node.applyStyle(["overflow_x": .string("auto"), "overflow_y": .string("auto"), "scrollbar_width": .string("none")])
        let scroll = try XCTUnwrap(node.scroll)
        scroll.contentSize = CGSize(width: 400, height: 600)
        scroll.layoutIfNeeded()
        XCTAssertFalse(scroll.showsHorizontalScrollIndicator)
        XCTAssertFalse(scroll.showsVerticalScrollIndicator)
        node.applyStyle(["overflow_x": .string("auto"), "overflow_y": .string("auto")])
        scroll.contentSize = CGSize(width: 400, height: 600)
        scroll.layoutIfNeeded()
        XCTAssertTrue(scroll.showsHorizontalScrollIndicator)
        XCTAssertTrue(scroll.showsVerticalScrollIndicator)
        // A hatch's explicit assignment survives later layouts too.
        scroll.showsHorizontalScrollIndicator = false
        scroll.showsVerticalScrollIndicator = false
        scroll.contentSize = CGSize(width: 800, height: 900)
        scroll.layoutIfNeeded()
        XCTAssertFalse(scroll.showsHorizontalScrollIndicator)
        XCTAssertFalse(scroll.showsVerticalScrollIndicator)
    }

    func testAFixedDocumentHasNoViewportEdgeEffect() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("scroll edge effects require iOS 26") }
        let p = Presenter()
        p.viewport.frame = CGRect(x: 0, y: 0, width: 200, height: 300)
        p.viewport.contentSize = CGSize(width: 200, height: 600)
        p.hostDocument(fixed: true)
        XCTAssertFalse(p.viewport.isScrollEnabled)
        XCTAssertTrue(p.viewport.topEdgeEffect.isHidden)
        XCTAssertTrue(p.viewport.accessibilityElementsHidden, "the empty viewport and its UIKit wrappers expose no elements")
        p.hostDocument(fixed: false)
        XCTAssertTrue(p.viewport.isScrollEnabled)
        XCTAssertFalse(p.viewport.topEdgeEffect.isHidden, "the scrolling document keeps its edge effect")
        XCTAssertFalse(p.viewport.accessibilityElementsHidden)
    }
}
#endif
