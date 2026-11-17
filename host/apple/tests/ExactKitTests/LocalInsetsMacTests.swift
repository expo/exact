// @ref LLP 1123 D9 — `var(--exact-safe-area-inset-*)` on the Mac: a page that
// covers a full-size-content window reads the title bar's height outside every
// owner, as `env()` does, and a modal route's sheet, a content owner with no
// chrome over its content, reads its own, none.
#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

final class LocalInsetsMacTests: XCTestCase {
    static let page = """
    component Proof
      state screen = "home"
      action open
        screen = "add"
      action back
        screen = "home"
      view
        column testId="root" viewport-fit="cover" navigationKey=screen navigationBack="back" width="100%" height="100%" position="relative"
          column navigationKey="home" testId="home" position="absolute" inset=0
            box testId="home-probe" width=10 height="var(--exact-safe-area-inset-top)"
            button testId="open" press=open
              text "Open"
          if screen == "add"
            column navigationKey="add" navigationPresentation="modal" testId="add" position="absolute" inset=0
              header display="flex" padding="8px 16px"
                button id="back" press=back testId="cancel"
                  text "Cancel"
                text "Add" role="heading"
              box testId="sheet-probe" width=10 height="calc(var(--exact-safe-area-inset-top) + 5px)"

    """

    private var fixture: SheetFixture?
    override func tearDown() { fixture?.close(); fixture = nil; super.tearDown() }

    private func until(_ what: String, _ f: SheetFixture, _ done: () -> Bool) {
        let deadline = Date().addingTimeInterval(5)
        while !done(), Date() < deadline { f.settle() }
        XCTAssertTrue(done(), what)
    }

    func testTheWindowsTitleBarOutsideOwnersAndNoneInARouteSheet() throws {
        let f = try SheetFixture(page: Self.page)
        fixture = f
        f.window.styleMask.insert(.fullSizeContentView)
        f.window.layoutIfNeeded()
        f.view.syncInsets()
        let top = f.view.safeAreaInsets.top
        XCTAssertGreaterThan(top, 0, "a full-size-content window's title bar is in the page's safe area")
        until("the page reads the title bar", f) { abs(((try? f.node("home-probe"))?.frame.height ?? -1) - top) < 0.5 }
        try f.press("open")
        until("the sheet's content reads its own inset, none", f) { abs(((try? f.node("sheet-probe"))?.frame.height ?? -1) - 5) < 0.5 }
        XCTAssertEqual(try f.node("home-probe").frame.height, top, accuracy: 0.5, "the page's is unchanged")
    }
}
#endif
