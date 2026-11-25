#if os(iOS)
@testable import ExactKit
import UIKit
import XCTest

/// An arranged pane's safe area (LLP 1123 D11): the band nested containment withholds from it is
/// Exact's contribution on its holder's ledger, beside what the app adds there.
extension ContentOwnersIOSTests {
    func testAnAppsOwnInsetOnAnArrangedPaneStaysThroughItsSafeAreaPasses() throws {
        guard #available(iOS 27.1, *) else { throw XCTSkip("arrangements require iOS 27.1") }
        guard let scene = hostedWindowScene() else { throw missing("scene", "arranged panes take their safe area in a window") }
        let (session, _, _, _, _, _) = try arrangedPeers(in: scene)
        let p = session.presenter
        let holder = try XCTUnwrap(p.arrangements.entries.values.first?.holders.first)
        holder.additionalSafeAreaInsets.left += 6
        for _ in 0..<2 { p.arrangements.sync() }
        XCTAssertEqual(holder.additionalSafeAreaInsets.left, 6, accuracy: 0.5, "an app's own inset on a pane stays")
        holder.additionalSafeAreaInsets.left -= 6
    }

    /// A pane is a container of its own (LLP 1123 D9): its content reads what UIKit gives its controller, not what UIKit
    /// gives the route, whose side bar the other pane is clear of.
    func testAnArrangedPanesContentReadsThePanesOwnInsets() throws {
        guard #available(iOS 27.1, *) else { throw XCTSkip("arrangements require iOS 27.1") }
        guard let scene = hostedWindowScene() else { throw missing("scene", "arranged panes take their safe area in a window") }
        let (session, root, first, second, _, _) = try arrangedPeers(in: scene)
        let p = session.presenter, holders = try XCTUnwrap(p.arrangements.entries[root.id]).holders
        let rows = try (node(session, "first-row-1"), node(session, "second-row-1"))
        // Each row's text starts past its padding by the right inset it reads
        func read(_ row: NodeView, in holder: ArrangementPaneController) -> CGFloat { row.convert(row.bounds, to: holder.view).minX - 16 }
        holders[1].additionalSafeAreaInsets.right += 84
        until("each pane's content reads its own right inset") {
            self.window?.layoutIfNeeded()
            return abs(read(rows.0, in: holders[0]) - holders[0].view.safeAreaInsets.right) < 0.5
                && abs(read(rows.1, in: holders[1]) - holders[1].view.safeAreaInsets.right) < 0.5
        }
        XCTAssertEqual(read(rows.1, in: holders[1]) - read(rows.0, in: holders[0]), 84, accuracy: 0.5)
        for (pane, holder) in [(first, holders[0]), (second, holders[1])] {
            let safe = holder.view.safeAreaInsets
            XCTAssertEqual(p.arrangements.insets.wanted[pane.id], [safe.top, safe.right, safe.bottom, safe.left])
        }
        holders[1].additionalSafeAreaInsets.right -= 84
    }
}
#endif
