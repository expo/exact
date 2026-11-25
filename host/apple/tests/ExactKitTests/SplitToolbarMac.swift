#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

extension SplitMacTests {
    func testSplitToolbarClearsTheListAndOpensAnAlertFromTheDetail() throws {
        let cover = app(tabs: false)
            .replacingOccurrences(of: "testId=\"app\"", with: "testId=\"app\" viewport-fit=\"cover\"")
            .replacingOccurrences(of: "text \"Library\" role=\"heading\"", with: "text \"\" role=\"heading\"")
            .replacingOccurrences(of: "  action count\n", with: "  action askDelete\n    showModal(\"delete-book\")\n  action count\n")
        let withDelete = replacing("button press=count", with: [
            "button press=askDelete destructive=true aria-label=\"Delete\" testId=\"delete\"",
            "  image \"symbol:delete\"",
        ], in: replacing("image \"symbol:delete\"", with: [], in: cover))
        let source = replacing("text `Shelved", with: [
            "dialog id=\"delete-book\" role=\"alertdialog\" aria-label=\"Delete this book?\" testId=\"delete-dialog\"",
            "  text \"This cannot be undone.\"",
            "  button press=back destructive=true commandfor=\"delete-book\" command=\"close\" testId=\"delete-confirm\"",
            "    text \"Delete\"",
            "  button commandfor=\"delete-book\" command=\"close\"",
            "    text \"Keep\"",
        ], in: withDelete)
        let session = try boot(tabs: false, size: CGSize(width: 521, height: 500), source: source)
        let exact = ExactView(session: session), p = session.presenter
        _ = compose(exact, width: 521, sidebar: false, window: ExactAgentWindow.self)
        let window = try XCTUnwrap(self.window)
        window.level = .agentBehind
        window.orderFront(nil)
        let first = try node("row-a")
        p.press(first.id)
        let search = try node("search"), delete = try node("delete"), dialog = try node("delete-dialog")
        let confirm = try node("delete-confirm")
        XCTAssertTrue(p.toolbar.search.node === search)
        XCTAssertFalse(dialog.isDescendant(of: p.root))
        window.layoutIfNeeded()
        exact.syncInsets()
        XCTAssertGreaterThan(exact.safeAreaInsets.top, 0)
        settle("the first row clears the toolbar") {
            first.bounds.height > 0 && first.convert(first.bounds, to: nil).maxY <= window.contentLayoutRect.maxY + 0.5
        }

        let agent = Agent(session: session)
        XCTAssertEqual(agent.tap(["id": Int(delete.id)])["native"] as? String, "NSToolbarItem")
        settle("the toolbar action opens the detail's alert") { p.alerts.presented?.node === dialog }
        XCTAssertNotNil(window.attachedSheet)
        XCTAssertEqual(agent.tap(["id": Int(confirm.id)])["native"] as? String, "NSAlert")
        settle("confirmation returns to the list") { !self.has("route-book-a") && p.alerts.presented == nil }
    }
}
#endif
