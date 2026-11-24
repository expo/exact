import QuartzCore
import XCTest
@testable import ExactKit
@testable import ExactNavigation
#if os(iOS)
import UIKit
#endif

/// A route's part as the Apple hosts play it (LLP 1013.001 D1): the poses
/// the Rust side writes, each about the box's centre, and the animators
/// UIKit is handed for a full-screen route.
final class RouteTransitionTests: XCTestCase {
    private func part(_ json: String) throws -> RoutePart {
        try XCTUnwrap(RoutePart(json: Data(json.utf8)))
    }

    func testAPartIsItsPosesAndNoPoseIsACut() throws {
        XCTAssertNil(RoutePart(json: Data("null".utf8)), "no part: the platform's transition")
        let slide = try part(#"{"seconds":0.25,"poses":[[0,41,0,1,1,0],[1,0,0,1,1,0]]}"#)
        XCTAssertEqual(slide.seconds, 0.25)
        XCTAssertEqual(slide.poses.count, 2)
        let cut = try part(#"{"seconds":0,"poses":[]}"#)
        XCTAssertFalse(cut.plays)
    }

    /// A turn and a scale about the centre leave the centre where the
    /// translation puts it, whichever point the layer is anchored at.
    func testAPoseTurnsAndScalesAboutTheCentre() {
        let pose: [Double] = [1, 10, 20, 0.5, 0.1, 90]
        let centre = CGPoint(x: 50, y: 30)
        for (t, at) in [(RoutePart.transform(pose), CGPoint.zero), (RoutePart.transform(pose, about: centre), centre)] {
            let moved = CGPointApplyAffineTransform(at, CATransform3DGetAffineTransform(t))
            XCTAssertEqual(moved.x, at.x + 10, accuracy: 1e-9)
            XCTAssertEqual(moved.y, at.y + 20, accuracy: 1e-9)
        }
        // 90° turns +x towards +y, clockwise on a y-down screen.
        let turned = CGPointApplyAffineTransform(CGPoint(x: 1, y: 0), CATransform3DGetAffineTransform(RoutePart.transform([1, 0, 0, 1, 0.1, 90])))
        XCTAssertEqual(turned.x, 0, accuracy: 1e-9)
        XCTAssertEqual(turned.y, 1, accuracy: 1e-9)
        let squashed = CGPointApplyAffineTransform(CGPoint(x: 0, y: 10), CATransform3DGetAffineTransform(RoutePart.transform([1, 0, 0, 1, 0.1, 0])))
        XCTAssertEqual(squashed.y, 1, accuracy: 1e-9)
    }

    func testTheAnimationPlaysThePosesEvenlyAndHoldsItsEnd() throws {
        let fade = try part(#"{"seconds":0.15,"poses":[[1,0,0,1,1,0],[0.5,0,0,1,1,0],[0,0,0,1,1,0]]}"#)
        let group = try XCTUnwrap(fade.animation() as? CAAnimationGroup)
        XCTAssertEqual(group.duration, 0.15)
        XCTAssertEqual(group.fillMode, .both)
        let opacity = try XCTUnwrap(group.animations?.first as? CAKeyframeAnimation)
        XCTAssertEqual(opacity.keyTimes, [0, 0.5, 1])
        XCTAssertEqual(opacity.values as? [Double], [1, 0.5, 0])
    }

    #if os(iOS)
    /// A full-screen route animates only the changes it declares a part for.
    func testAFullScreenRouteIsHandedAnAnimatorOnlyForAPartItDeclares() throws {
        let fade = try part(#"{"seconds":0.18,"poses":[[0,0,0,1,1,0],[1,0,0,1,1,0]]}"#)
        XCTAssertNil(RoutePresenting(entrance: nil, exit: nil), "no part: UIKit's presentation")
        let presenting = try XCTUnwrap(RoutePresenting(entrance: fade, exit: nil))
        let controller = UIViewController()
        let entrance = presenting.animationController(forPresented: controller, presenting: controller, source: controller)
        XCTAssertEqual((entrance as? RouteTransition)?.arriving, true)
        XCTAssertNil(presenting.animationController(forDismissed: controller))
    }
    #endif
}
