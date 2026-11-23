// @ref LLP 1141.004 D6 — a dialog's header in its sheet's rows on the Mac,
// as a modal route's is (RouteSheetMacTests): the heading the title row, the
// button that closes the dialog and the prominent one the push buttons at
// the bottom right, each pressing its authored button. A dialog with no
// header keeps its sheet as before (SheetHostMacTests).
#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

final class DialogHeaderSheetMacTests: XCTestCase {
    /// #1316's form, and the same form with a field in its header.
    static let page = """
    component Proof
      state invited = 0
      state cancels = 0
      action invite
        invited = invited + 1
      action cancelled
        cancels = cancels + 1
      view
        column testId="page" width="100%" height="100%" padding=20 gap=12
          button testId="open" commandfor="invite" command="show-modal"
            text "Invite"
          button testId="open-filtered" commandfor="filtered" command="show-modal"
            text "Filtered"
          text `invited ${invited} cancels ${cancels}` testId="status"
          dialog id="invite" role="dialog" aria-labelledby="invite-title" width=360 testId="invite" cancel=cancelled
            header testId="invite-header"
              button testId="invite-cancel" commandfor="invite" command="close"
                text "Cancel"
              text "Invite people" id="invite-title" role="heading"
              button testId="invite-go" buttonStyle="filled" press=invite
                text "Invite"
            column padding=20 testId="invite-body"
              input testId="invite-email" width=300 height=24
          dialog id="filtered" width=360 testId="filtered"
            header testId="filtered-header"
              text "Filtered" role="heading"
              input testId="filtered-field" width=100 height=24
            box height=40
    """

    private var fixture: SheetFixture?
    override func tearDown() { fixture?.close(); fixture = nil; super.tearDown() }

    func testTheHeaderIsTheSheetsTitleRowAndItsTwoButtons() throws {
        let f = try SheetFixture(page: Self.page)
        fixture = f
        try f.press("open")
        let window = try XCTUnwrap(f.presenter.sheets(on: f.window).last, "the dialog is a sheet")
        let chrome = try XCTUnwrap(window.contentView as? RouteChrome, "with the rows a modal route's sheet has")
        XCTAssertEqual(chrome.title.stringValue, "Invite people")
        XCTAssertEqual(window.title, "Invite people", "the sheet's window is named as its title row says")
        XCTAssertEqual([chrome.cancel.title, chrome.confirm.title], ["Cancel", "Invite"])
        XCTAssertTrue(chrome.back.isHidden, "a dialog has no Back")
        XCTAssertEqual(chrome.confirm.keyEquivalent, "\r", "the prominent one is the default button")
        XCTAssertEqual(chrome.cancel.keyEquivalent, "", "Escape stays the dialog's own close request")
        let header = try f.node("invite-header")
        XCTAssertTrue(header.isHidden, "the header shows nothing else: the rows stand for all of it")
        XCTAssertEqual(f.presenter.covers.delivered[header.id], .whole)
        XCTAssertEqual(try f.node("invite-body").frame.minY, 0, "the dialog lays out without its header")

        XCTAssertTrue(f.presenter.dialogs.defaultButton(of: try f.node("invite")) === (try f.node("invite-go")), "Return presses the row that confirms")
        XCTAssertEqual(f.presenter.dialogs.activate(try f.node("invite-go")), true, "the driver's tap on the authored button is its row's click")
        f.settle()
        XCTAssertTrue(f.status.contains("invited 1"), f.status)
        XCTAssertNotNil(f.window.attachedSheet, "Invite's press does not close the dialog")
        chrome.cancel.performClick(nil)
        f.settle()
        XCTAssertNil(f.window.attachedSheet, "Cancel closed it")
        XCTAssertFalse(header.isHidden, "its header is back in the closed dialog")
        XCTAssertNil(f.presenter.covers.delivered[header.id])
        XCTAssertTrue(f.status.contains("cancels 0"), "a press of Cancel is no cancel event: \(f.status)")
    }

    func testAHeaderTheRowsDoNotStandForStaysInTheDialog() throws {
        let f = try SheetFixture(page: Self.page)
        fixture = f
        try f.press("open-filtered")
        let window = try XCTUnwrap(f.presenter.sheets(on: f.window).last)
        let chrome = try XCTUnwrap(window.contentView as? RouteChrome)
        XCTAssertEqual(chrome.title.stringValue, "Filtered", "the heading is the title row")
        XCTAssertTrue(chrome.cancel.isHidden && chrome.confirm.isHidden, "no buttons to stand for")
        XCTAssertFalse(try f.node("filtered-header").isHidden, "the field keeps the header in the dialog")
        XCTAssertFalse(try f.node("filtered-field").isHidden)
    }

    func testADialogThatPaintsItsOwnSurfaceKeepsItsHeader() throws {
        let page = Self.page.replacingOccurrences(of: "width=360 testId=\"invite\"", with: "width=360 testId=\"invite\" background-color=\"#FFFFFF\"")
        let f = try SheetFixture(page: page)
        fixture = f
        try f.press("open")
        let window = try XCTUnwrap(f.presenter.sheets(on: f.window).last)
        XCTAssertFalse(window.contentView is RouteChrome, "the author's box has no rows of the platform's")
        XCTAssertFalse(try f.node("invite-header").isHidden)
    }
}
#endif
