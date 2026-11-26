// @ref LLP 1123 D7 — a popover route on AppKit, in a whole session: its subtree is in an `NSPopover` beside the
// element its anchor names, an owner there, its arrows naming the side; AppKit's close is the stack's way back; one
// whose anchor is nowhere in the window is the window sheet; and `closedby="none"` leaves AppKit's own closes out.
#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

final class RoutePopoverMacTests: XCTestCase {
    static let page = """
    component Proof
      state screen = "home"
      action open(s: string)
        screen = s
      action back
        screen = "home"
      view
        column testId="root" navigationKey=screen navigationBack="back" width="100%" height="100%" position="relative"
          column navigationKey="home" testId="home" position="absolute" inset=0 padding=20 gap=12
            button id="anchor" testId="open-pick" press=open("pick")
              text "Pick"
            button testId="open-fixed" press=open("fixed")
              text "Fixed"
            button testId="open-loose" press=open("loose")
              text "Loose"
            text `screen ${screen}` testId="status"
          if screen == "pick"
            column navigationKey="pick" navigationPresentation="popover" navigationPopoverAnchor="anchor" navigationPopoverArrows="down" testId="pick" padding=12 gap=8
              text "Choose one" testId="pick-body"
              button id="back" press=back testId="pick-done"
                text "Done"
          if screen == "fixed"
            column navigationKey="fixed" navigationPresentation="popover" navigationPopoverAnchor="anchor" closedby="none" testId="fixed" padding=12
              button id="back" press=back
                text "Done"
          if screen == "loose"
            column navigationKey="loose" navigationPresentation="popover" navigationPopoverAnchor="nowhere" testId="loose" padding=12
              button id="back" press=back
                text "Done"

    """

    private var fixture: SheetFixture?
    override func tearDown() { fixture?.close(); fixture = nil; super.tearDown() }

    private func make() throws -> SheetFixture {
        let f = try SheetFixture(page: Self.page, size: CGSize(width: 700, height: 500))
        fixture = f
        f.window.orderFront(nil)
        f.settle()
        return f
    }

    func testAPopoverRouteIsAnAppKitPopoverBesideItsAnchorAndItsCloseGoesBack() throws {
        let f = try make()
        try f.press("open-pick")
        let navigation = f.presenter.navigationHost
        let shown = try XCTUnwrap(navigation.routePopover, "an NSPopover, not a sheet")
        let route = try f.node("pick"), anchor = try f.node("open-pick")
        XCTAssertTrue(shown.popover.isShown)
        XCTAssertNil(f.window.attachedSheet, "no window sheet")
        XCTAssertTrue(route.isDescendant(of: shown.content), "the route's own subtree in the popover")
        XCTAssertFalse(route.window === f.window, "in the popover's window")
        XCTAssertTrue(shown.content.isFlipped, "the owner's container")
        XCTAssertEqual(shown.popover.behavior, .transient)
        XCTAssertGreaterThan(shown.popover.contentSize.width, 0, "its size the route's")
        XCTAssertEqual(shown.popover.positioningRect, anchor.bounds, "pointed at its anchor")
        // AppKit's close, as on Escape or a click elsewhere, is the stack's way back, once its close has played
        shown.popover.performClose(nil)
        let limit = Date().addingTimeInterval(3)
        while shown.popover.isShown || f.status != "screen home", Date() < limit { f.settle() }
        XCTAssertFalse(shown.popover.isShown, "AppKit closed it")
        XCTAssertEqual(f.status, "screen home")
        XCTAssertNil(navigation.routePopover)
        XCTAssertNil(navigation.poppedRoute)
    }

    func testClosedByNoneLeavesAppKitsClosesOut() throws {
        let f = try make()
        try f.press("open-fixed")
        XCTAssertEqual(f.presenter.navigationHost.routePopover?.popover.behavior, .applicationDefined)
        // The app's own way back closes it
        f.presenter.press(try XCTUnwrap(f.presenter.views.values.first { $0.props["id"] == "back" && $0.window != f.window }).id)
        f.settle()
        XCTAssertEqual(f.status, "screen home")
        XCTAssertNil(f.presenter.navigationHost.routePopover, "the popover leaves with its route")
    }

    /// The side of its anchor the first arrow direction puts the popover on, in the anchor's flipped coordinates.
    func testTheArrowsNameTheSide() {
        XCTAssertEqual(RoutePopover.edge(nil), .maxY, "below where unsaid")
        XCTAssertEqual(RoutePopover.edge("up"), .maxY)
        XCTAssertEqual(RoutePopover.edge("down left"), .minY, "the first direction")
        XCTAssertEqual(RoutePopover.edge("left"), .maxX)
        XCTAssertEqual(RoutePopover.edge("right"), .minX)
    }

    func testAPopoverRouteWhoseAnchorIsNowhereIsTheWindowSheet() throws {
        let f = try make()
        try f.press("open-loose")
        XCTAssertNil(f.presenter.navigationHost.routePopover)
        let sheet = try XCTUnwrap(f.window.attachedSheet, "the window sheet")
        XCTAssertTrue(try f.node("loose").window === sheet)
    }
}
#endif
