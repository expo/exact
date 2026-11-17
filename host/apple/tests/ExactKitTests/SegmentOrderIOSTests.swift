// @ref LLP 1035.003 — a segmented control's segments are found through public API
// alone: the elements UIKit's accessibility container declares, put in segment order
// in the control's own space and labelled there, and a point's segment by equal shares.
#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

final class SegmentOrderIOSTests: XCTestCase {
    /// Each tab's test id lands on its own segment, the one UIKit names with
    /// that tab's title, whatever turns the control on screen (an ancestor
    /// rotated half a turn, a right-to-left layout, or both), and a layout
    /// writes changed ids on the segments an assistive client already holds,
    /// with no read of the container.
    func testEachSegmentCarriesItsOwnTabsMetadataUnderARotationAndRightToLeft() throws {
        guard Presenter.axRuntimeLoaded else { throw missing("accessibility", "UIKit makes a segment's element only for an assistive client") }
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.first as? UIWindowScene)
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        window.rootViewController = UIViewController()
        window.makeKeyAndVisible()
        defer { window.isHidden = true }
        for (rotated, rightToLeft) in [(false, false), (true, false), (false, true), (true, true)] {
            let holder = UIView(frame: CGRect(x: 40, y: 200, width: 300, height: 32))
            if rotated { holder.transform = CGAffineTransform(rotationAngle: .pi) }
            let control = LabelledSegmentedControl(items: ["One", "Two", "Three"])
            control.segmentIDs = ["tab-1", "tab-2", "tab-3"]
            control.frame = holder.bounds
            if rightToLeft { control.semanticContentAttribute = .forceRightToLeft }
            holder.addSubview(control)
            window.rootViewController?.view.addSubview(holder)
            window.layoutIfNeeded()
            let end = Date(timeIntervalSinceNow: 5)
            while control.accessibilitySegments.isEmpty, Date() < end { RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.05)) }
            let held = control.accessibilitySegments, at = "rotated \(rotated), right to left \(rightToLeft)"
            // UIKit's own names, which no authored label replaces, tell each segment
            XCTAssertEqual(held.map { "\($0.accessibilityLabel ?? "-") \($0.readIdentifier ?? "-")" }, ["One tab-1", "Two tab-2", "Three tab-3"], at)
            control.segmentIDs = ["tab-a", "tab-b", "tab-c"]
            control.layoutIfNeeded()
            XCTAssertEqual(held.map(\.readIdentifier), ["tab-a", "tab-b", "tab-c"], "after a layout alone, \(at)")
            holder.removeFromSuperview()
        }
    }

    /// A point's segment is its share of the bounds, counted from the leading
    /// edge: from the right, right to left, and from the bounds' own origin.
    func testAPointsSegmentIsItsShareFromTheLeadingEdgeOfTheBounds() {
        let control = UISegmentedControl(items: ["One", "Two", "Three"])
        control.frame = CGRect(x: 0, y: 0, width: 300, height: 32)
        XCTAssertEqual([50, 150, 250].map { control.segment(at: CGPoint(x: $0, y: 16)) }, [0, 1, 2])
        XCTAssertNil(control.segment(at: CGPoint(x: 301, y: 16)), "outside the control")
        control.bounds.origin.x = 100
        XCTAssertEqual([150, 250, 350].map { control.segment(at: CGPoint(x: $0, y: 16)) }, [0, 1, 2], "bounds that start at 100")
        control.semanticContentAttribute = .forceRightToLeft
        XCTAssertEqual([150, 250, 350].map { control.segment(at: CGPoint(x: $0, y: 16)) }, [2, 1, 0], "and right to left")
        control.bounds.origin.x = 0
        XCTAssertEqual([50, 150, 250].map { control.segment(at: CGPoint(x: $0, y: 16)) }, [2, 1, 0])
    }
}
#endif
