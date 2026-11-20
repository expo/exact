#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNativeViews

/// A platform view in a box that clips keeps a size of its own, centred in the
/// box, while UIKit animates the box's size (`NativeCrop`). Its layout margins
/// meanwhile take what of it lies past the box, so what it places by them (a
/// map's logo and legal link) stands inside the box and moves with it; they are
/// its own again once the animation ends.
final class NativeCropIOSTests: XCTestCase {
    func testAHeldViewsMarginsTakeWhatLiesPastItsBox() throws {
        let presenter = Presenter()
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        presenter.viewport.frame = window.bounds
        window.addSubview(presenter.viewport)
        window.makeKeyAndVisible()
        defer { window.isHidden = true }
        let owner = NodeView(id: 1, kind: "native", presenter: presenter)
        owner.frame = CGRect(x: 50, y: 150, width: 300, height: 100)
        owner.clipsToBounds = true
        window.addSubview(owner)
        let view = UIView(frame: owner.bounds)
        view.layoutMargins = UIEdgeInsets(top: 8, left: 8, bottom: 8, right: 8)
        owner.addSubview(view)
        let crop = NativeCrop()
        var ended = false
        UIView.animate(withDuration: 0.3) {
            crop.resize(view, in: owner, { owner.frame.size.width = 200 }, end: { ended = true; view.frame = owner.contentBox() })
        }
        XCTAssertTrue(crop.holds(view))
        XCTAssertEqual(view.bounds.size, CGSize(width: 300, height: 100), "its old, larger size")
        XCTAssertEqual(view.center, CGPoint(x: 100, y: 50), "centred in the box")
        XCTAssertEqual(view.layoutMargins, UIEdgeInsets(top: 8, left: 58, bottom: 8, right: 58), "its own and the 50 points past each side of the box")
        XCTAssertFalse(view.insetsLayoutMarginsFromSafeArea)
        let deadline = Date().addingTimeInterval(3)
        while !ended, Date() < deadline { RunLoop.main.run(until: Date().addingTimeInterval(0.05)) }
        XCTAssertTrue(ended, "the box's animation ended")
        XCTAssertEqual(view.layoutMargins, UIEdgeInsets(top: 8, left: 8, bottom: 8, right: 8), "its own again")
        XCTAssertTrue(view.insetsLayoutMarginsFromSafeArea)
    }
}
#endif
