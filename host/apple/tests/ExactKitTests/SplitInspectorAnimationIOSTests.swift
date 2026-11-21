#if os(iOS)
import UIKit
import XCTest
import ExactCore
@testable import ExactKit
@testable import ExactNavigation
@testable import ExactGroupedLists

extension SplitIOSTests {
    /// LLP 1127 D14, as UIKit's own inspector comes and goes, on an iPhone
    /// Duo's width, where it hides the list: the column slides in over
    /// UIKit's animation with its route laid out where it ends, still, while
    /// the detail's content moves to its new width inside that animation and
    /// the list keeps its sidebar's look as it slides away. Closed, the
    /// column slides out with the route's last pixels, the detail moving back
    /// with it.
    func testTheInspectorsColumnMovesWithTheDetailAndKeepsItsRoute() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("the inspector's column is iOS 26's") }
        let session = try boot(size: CGSize(width: 951, height: 669))
        let navigation = session.presenter.navigationHost, agent = Agent(session: session)
        until("a split") { navigation.split != nil && !agent.nativeInFlight() }
        let split = try XCTUnwrap(navigation.split)
        split.controller.traitOverrides.horizontalSizeClass = .regular
        until("expanded") { !split.controller.isCollapsed && !agent.nativeInFlight() }
        XCTAssertNil(agent.tap(["id": Int(try node(session, "row-a").id)])["error"])
        until("the book is in the detail") { split.shown.count == 2 && !agent.nativeInFlight() }
        let books = try node(session, "books"), notes = try node(session, "notes-a")
        let wide = notes.frame
        XCTAssertNil(agent.tap(["id": Int(try node(session, "info-a").id)])["error"])
        // UIKit puts the column in place as it lays the columns out, inside its animation (`SplitIOS.moveAlong`)
        until("its info is in the inspector's column") { split.inspecting?.navigationController?.view.superview != nil }
        let route = try XCTUnwrap(split.inspecting), info = try node(session, "info-page")
        let column = try XCTUnwrap(route.navigationController?.view.superview)
        let slide = try XCTUnwrap(Self.longest(column.layer), "the column slides in")
        XCTAssertEqual(info.frame.width, route.view.bounds.width, "its route is laid out at the column's width")
        for view in [info] + info.subviews { XCTAssertNil(Self.longest(view.layer), "its route is laid out still: \(view)") }
        let moving = try XCTUnwrap(Self.longest(notes.layer), "the detail's content moves inside the column's animation")
        XCTAssertEqual(moving.duration, slide.duration, accuracy: 0.01)
        XCTAssertNotEqual(notes.frame.width, wide.width, "to its width beside the inspector")
        XCTAssertEqual(split.controller.displayMode, .secondaryOnly, "the inspector hides the list")
        XCTAssertTrue(navigation.showsAsSidebar(books), "the list keeps its sidebar's look as it slides away")

        until("the column is in") { split.controller.transitionCoordinator == nil && !agent.nativeInFlight() }
        XCTAssertNil(agent.tap(["id": Int(try node(session, "info-close").id)])["error"])
        until("the info's route is gone") { !session.presenter.views.values.contains { $0.props["testId"] == "info-page" } }
        // UIKit opens the close's animation as it lays the columns out (`SplitIOS.moveAlong`)
        until("the detail's content moves back inside the column's animation") { Self.longest(notes.layer) != nil }
        XCTAssertTrue(route.view.window != nil && !route.view.subviews.isEmpty, "the column goes with the route's last pixels")
        until("closed") { split.controller.transitionCoordinator == nil && !agent.nativeInFlight() }
        XCTAssertEqual(notes.frame.width, wide.width, accuracy: 0.5)
    }

    private static func longest(_ layer: CALayer) -> CAAnimation? {
        (layer.animationKeys() ?? []).compactMap { layer.animation(forKey: $0) }.max { $0.duration < $1.duration }
    }
}
#endif
